import { create } from "zustand";
import { openUrl } from "@tauri-apps/plugin-opener";
import { invoke, isTauri } from "../tauri";
import {
  AUTH_NETWORK_UNAVAILABLE,
  authErrorText,
  isTransientAuthNetworkError,
  planAuthSessionRetry,
} from "./sessionRestore";

export interface AuthSession {
  loggedIn: boolean;
  // The access token stays in Rust; Helix calls go through the helix_fetch proxy.
  userId?: string | null;
  login?: string | null;
  displayName?: string | null;
  profileImageUrl?: string | null;
  scopes: string[];
}

export interface DeviceCodeResponse {
  deviceCode: string;
  expiresIn: number;
  interval: number;
  userCode: string;
  verificationUri: string;
}

/** Tagged union returned by auth_poll_device_login. */
export type DevicePoll =
  | { state: "pending" }
  | { state: "slowDown" }
  | { state: "done"; session: AuthSession };

interface AuthState {
  session: AuthSession | null;
  loading: boolean;
  device: DeviceCodeResponse | null;
  error: string | null;
  refreshSession: (opts?: { quiet?: boolean }) => Promise<void>;
  startLogin: () => Promise<void>;
  cancelLogin: () => void;
  logout: () => Promise<void>;
}

let pollTimer: ReturnType<typeof setTimeout> | null = null;
let sessionRetryTimer: ReturnType<typeof setTimeout> | null = null;
let sessionRetryAttempt = 0;
let sessionRefreshGeneration = 0;
let onlineRetryBound = false;

function clearPoll() {
  if (pollTimer) {
    clearTimeout(pollTimer);
    pollTimer = null;
  }
}

function clearSessionRetry() {
  if (sessionRetryTimer) {
    clearTimeout(sessionRetryTimer);
    sessionRetryTimer = null;
  }
  sessionRetryAttempt = 0;
}

function bindOnlineSessionRetry() {
  if (onlineRetryBound || typeof window === "undefined") {
    return;
  }
  onlineRetryBound = true;
  window.addEventListener("online", () => {
    sessionRetryAttempt = 0;
    if (sessionRetryTimer) {
      clearTimeout(sessionRetryTimer);
      sessionRetryTimer = null;
    }
    void useAuthStore.getState().refreshSession({ quiet: true });
  });
}

function scheduleSessionRetry() {
  if (sessionRetryTimer) {
    clearTimeout(sessionRetryTimer);
    sessionRetryTimer = null;
  }
  const { session, error } = useAuthStore.getState();
  const plan = planAuthSessionRetry({
    loggedIn: Boolean(session?.loggedIn),
    error,
    attempt: sessionRetryAttempt,
  });
  if (!plan) {
    sessionRetryAttempt = 0;
    return;
  }
  sessionRetryAttempt += 1;
  sessionRetryTimer = setTimeout(() => {
    sessionRetryTimer = null;
    void useAuthStore.getState().refreshSession({ quiet: true });
  }, plan.delayMs);
}

function restartViewerPresence() {
  void import("../streaming/store").then(({ syncViewerPresence }) => {
    syncViewerPresence(true);
  });
}

export const useAuthStore = create<AuthState>((set, get) => ({
  session: null,
  loading: true,
  device: null,
  error: null,

  refreshSession: async (opts) => {
    bindOnlineSessionRetry();
    const generation = ++sessionRefreshGeneration;
    if (!opts?.quiet) {
      set({ loading: true, error: null });
    }
    if (!isTauri()) {
      clearSessionRetry();
      set({
        session: { loggedIn: false, scopes: [] },
        loading: false,
        error: null,
      });
      return;
    }
    try {
      const session = await invoke<AuthSession>("auth_get_session");
      if (generation !== sessionRefreshGeneration) {
        return;
      }
      clearSessionRetry();
      set({ session, loading: false, error: null });
      if (session.loggedIn) {
        restartViewerPresence();
      }
    } catch (err) {
      if (generation !== sessionRefreshGeneration) {
        return;
      }
      const message = authErrorText(err);
      const error = isTransientAuthNetworkError(message)
        ? AUTH_NETWORK_UNAVAILABLE
        : message;
      set({
        session: { loggedIn: false, scopes: [] },
        loading: false,
        error,
      });
      scheduleSessionRetry();
    }
  },

  startLogin: async () => {
    sessionRefreshGeneration += 1;
    clearPoll();
    clearSessionRetry();
    set({ error: null, device: null, loading: true });
    if (!isTauri()) {
      set({
        loading: false,
        error:
          "Run `npm run tauri:dev` to log in — browser Vite has no desktop APIs.",
      });
      return;
    }
    try {
      const device = await invoke<DeviceCodeResponse>("auth_start_device_login");
      set({ device, loading: false });
      await openUrl(device.verificationUri);

      // RFC 8628: poll at `interval`, and add 5 s each time Twitch answers
      // slow_down (capped) instead of hammering the token endpoint.
      let pollIntervalMs = Math.max(device.interval, 1) * 1000;
      const poll = async () => {
        if (!get().device) {
          return;
        }
        try {
          const result = await invoke<DevicePoll>("auth_poll_device_login", {
            deviceCode: device.deviceCode,
          });
          if (result.state === "done" && result.session?.loggedIn) {
            clearPoll();
            set({ session: result.session, device: null, error: null });
            restartViewerPresence();
            return;
          }
          if (result.state === "slowDown") {
            pollIntervalMs = Math.min(pollIntervalMs + 5000, 30_000);
          }
          pollTimer = setTimeout(() => {
            void poll();
          }, pollIntervalMs);
        } catch (err) {
          clearPoll();
          set({
            device: null,
            error: err instanceof Error ? err.message : String(err),
          });
        }
      };
      pollTimer = setTimeout(() => {
        void poll();
      }, pollIntervalMs);
    } catch (err) {
      set({
        loading: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  cancelLogin: () => {
    clearPoll();
    set({ device: null });
  },

  logout: async () => {
    sessionRefreshGeneration += 1;
    clearPoll();
    clearSessionRetry();
    if (isTauri()) {
      await invoke("auth_logout");
    }
    set({ session: { loggedIn: false, scopes: [] }, device: null });
  },
}));
