import { invoke, isTauri } from "../tauri";

export const TWITCH_WEB_AUTH_CHANGED_EVENT = "twitch-web-auth-changed";

export interface TwitchWebsiteAuthStatus {
  configured: boolean;
  login: string | null;
  userId: string | null;
  playbackReady: boolean;
}

export interface TwitchWebsiteAuthUiState {
  token: string;
  status: TwitchWebsiteAuthStatus;
}

export function completeWebsiteAuthSave(
  _submittedToken: string,
  status: TwitchWebsiteAuthStatus,
): TwitchWebsiteAuthUiState {
  return { token: "", status };
}

export function websiteAuthLabel(status: TwitchWebsiteAuthStatus): string {
  if (!status.configured) return "Authenticated playback not connected";
  const account = status.login ? ` for ${status.login}` : "";
  const suffix = status.playbackReady ? "connected" : "needs repair";
  return `Authenticated playback${account}: ${suffix}`;
}

export async function getTwitchWebsiteAuthStatus(): Promise<TwitchWebsiteAuthStatus> {
  if (!isTauri()) {
    return {
      configured: false,
      login: null,
      userId: null,
      playbackReady: false,
    };
  }
  return invoke<TwitchWebsiteAuthStatus>("twitch_web_auth_status");
}

export async function saveTwitchWebsiteAuth(
  token: string,
): Promise<TwitchWebsiteAuthStatus> {
  return invoke<TwitchWebsiteAuthStatus>("twitch_web_auth_save", { token });
}

export async function clearTwitchWebsiteAuth(): Promise<TwitchWebsiteAuthStatus> {
  return invoke<TwitchWebsiteAuthStatus>("twitch_web_auth_clear");
}
