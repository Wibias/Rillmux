import { describe, expect, it } from "vitest";
import {
  AUTH_NETWORK_UNAVAILABLE,
  isTransientAuthNetworkError,
  nextAuthRetryDelayMs,
  planAuthSessionRetry,
  shouldOfferTwitchLogin,
} from "./sessionRestore";

describe("isTransientAuthNetworkError", () => {
  it("treats a failed oauth validate send as retryable", () => {
    expect(
      isTransientAuthNetworkError(
        "error sending request for url (https://id.twitch.tv/oauth2/validate)",
      ),
    ).toBe(true);
  });

  it("treats the stable network_unavailable code as retryable", () => {
    expect(isTransientAuthNetworkError(AUTH_NETWORK_UNAVAILABLE)).toBe(true);
  });

  it("treats a later refresh as retryable", () => {
    expect(
      isTransientAuthNetworkError("token refresh failed (503); will retry later"),
    ).toBe(true);
  });

  it("does not treat an expired session as retryable", () => {
    expect(
      isTransientAuthNetworkError("session expired; please log in again"),
    ).toBe(false);
  });

  it("does not treat a 401 validate as retryable", () => {
    expect(
      isTransientAuthNetworkError(
        "HTTP status client error (401 Unauthorized) for url (https://id.twitch.tv/oauth2/validate)",
      ),
    ).toBe(false);
  });
});

describe("planAuthSessionRetry", () => {
  it("retries a stored session after the app starts offline", () => {
    expect(
      planAuthSessionRetry({
        loggedIn: false,
        error:
          "error sending request for url (https://id.twitch.tv/oauth2/validate)",
        attempt: 0,
      }),
    ).toEqual({ delayMs: 2000 });
  });

  it("backs off and then stops offering login while waiting", () => {
    expect(
      planAuthSessionRetry({
        loggedIn: false,
        error: AUTH_NETWORK_UNAVAILABLE,
        attempt: 3,
      }),
    ).toEqual({ delayMs: 15_000 });
    expect(
      shouldOfferTwitchLogin({
        loggedIn: false,
        deviceActive: false,
        error: AUTH_NETWORK_UNAVAILABLE,
      }),
    ).toBe(false);
  });

  it("does not retry a successful session or a real logout", () => {
    expect(
      planAuthSessionRetry({
        loggedIn: true,
        error: null,
        attempt: 0,
      }),
    ).toBeNull();
    expect(
      planAuthSessionRetry({
        loggedIn: false,
        error: null,
        attempt: 0,
      }),
    ).toBeNull();
    expect(
      shouldOfferTwitchLogin({
        loggedIn: false,
        deviceActive: false,
        error: null,
      }),
    ).toBe(true);
  });
});

describe("nextAuthRetryDelayMs", () => {
  it("caps backoff at 15 seconds", () => {
    expect(nextAuthRetryDelayMs(0)).toBe(2000);
    expect(nextAuthRetryDelayMs(1)).toBe(4000);
    expect(nextAuthRetryDelayMs(2)).toBe(8000);
    expect(nextAuthRetryDelayMs(9)).toBe(15_000);
  });
});
