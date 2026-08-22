export const AUTH_NETWORK_UNAVAILABLE = "network_unavailable";

const TRANSIENT_SNIPPETS = [
  AUTH_NETWORK_UNAVAILABLE,
  "error sending request",
  "connection reset",
  "connection refused",
  "dns error",
  "failed to lookup address",
  "no such host",
  "network unreachable",
  "timed out",
  "timeout",
  "will retry later",
];

export function authErrorText(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") {
      return message;
    }
  }
  return String(error);
}

export function isTransientAuthNetworkError(error: unknown): boolean {
  const message = authErrorText(error).trim();
  if (!message) {
    return false;
  }
  const lower = message.toLowerCase();
  if (lower.includes("401") || lower.includes("unauthorized")) {
    return false;
  }
  if (lower.includes("session expired")) {
    return false;
  }
  return TRANSIENT_SNIPPETS.some((snippet) => lower.includes(snippet));
}

export function nextAuthRetryDelayMs(attempt: number): number {
  const delays = [2000, 4000, 8000, 15_000];
  const index = Math.min(Math.max(attempt, 0), delays.length - 1);
  return delays[index]!;
}

export function planAuthSessionRetry(input: {
  loggedIn: boolean;
  error: string | null;
  attempt: number;
}): { delayMs: number } | null {
  if (input.loggedIn || !isTransientAuthNetworkError(input.error)) {
    return null;
  }
  return { delayMs: nextAuthRetryDelayMs(input.attempt) };
}

export function shouldOfferTwitchLogin(input: {
  loggedIn: boolean;
  deviceActive: boolean;
  error: string | null;
}): boolean {
  if (input.loggedIn || input.deviceActive) {
    return false;
  }
  return !isTransientAuthNetworkError(input.error);
}
