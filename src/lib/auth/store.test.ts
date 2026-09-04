import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
  openUrl: vi.fn(() => Promise.resolve()),
}));

vi.mock("../tauri", () => ({
  invoke: mocks.invoke,
  isTauri: mocks.isTauri,
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: mocks.openUrl,
}));

import type { AuthSession, DeviceCodeResponse } from "./store";
import { useAuthStore } from "./store";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

const loggedInSession: AuthSession = {
  loggedIn: true,
  userId: "123",
  login: "rillmux-user",
  displayName: "Rillmux User",
  profileImageUrl: null,
  scopes: ["user:read:follows"],
};

const device: DeviceCodeResponse = {
  deviceCode: "device-code",
  expiresIn: 1800,
  interval: 30,
  userCode: "ABCD1234",
  verificationUri: "https://www.twitch.tv/activate",
};

describe("auth transition ownership", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauri.mockReturnValue(true);
    mocks.openUrl.mockClear();
    useAuthStore.setState({
      session: null,
      loading: true,
      device: null,
      error: null,
    });
  });

  it("does not let an older session restore overwrite logout", async () => {
    const restore = deferred<AuthSession>();
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "auth_get_session") return restore.promise;
      if (command === "auth_logout") return Promise.resolve();
      throw new Error(`unexpected command: ${command}`);
    });

    const refreshPromise = useAuthStore.getState().refreshSession();
    await useAuthStore.getState().logout();

    restore.resolve(loggedInSession);
    await refreshPromise;

    expect(useAuthStore.getState()).toMatchObject({
      session: { loggedIn: false, scopes: [] },
      loading: false,
      error: null,
    });
  });

  it("does not let an older session restore overwrite a new login", async () => {
    const restore = deferred<AuthSession>();
    const loginStart = deferred<DeviceCodeResponse>();
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "auth_get_session") return restore.promise;
      if (command === "auth_start_device_login") return loginStart.promise;
      throw new Error(`unexpected command: ${command}`);
    });

    const refreshPromise = useAuthStore.getState().refreshSession();
    const loginPromise = useAuthStore.getState().startLogin();

    restore.resolve(loggedInSession);
    await refreshPromise;

    expect(useAuthStore.getState().session).toBeNull();

    loginStart.resolve(device);
    await loginPromise;
    useAuthStore.getState().cancelLogin();
  });
});
