/**
 * Tauri 2.11 listen()/unlisten() race (https://github.com/tauri-apps/tauri/issues/15799).
 *
 * `listen()` resolves when Rust answers `plugin:event|listen`, but the webview
 * registry entry is written by a later eval. Calling the returned unlisten in
 * that window throws:
 *
 *   TypeError: ... listeners[eventId].handlerId
 *
 * on the generated unlisten script *before* `@tauri-apps/api` sends
 * `plugin:event|unlisten`, so the backend listener leaks.
 *
 * Locked Rillmux versions (`tauri` 2.11.5, `@tauri-apps/api` 2.11.1) do not
 * include a released fix: PR https://github.com/tauri-apps/tauri/pull/15800 is
 * unmerged, PR https://github.com/tauri-apps/tauri/pull/15851 was closed.
 *
 * Retry the same unlisten closure until the registry entry exists (or the
 * attempt budget is exhausted). That closure closes over the original event id,
 * so a remount's replacement listener is never unlistened by a stale retry.
 * Unrelated unlisten errors are not retried.
 */
import { debugRuntimeEvent } from "../diagnostics/runtimeDebug";

export type ScheduleRetry = (retry: () => void) => () => void;

export type SafeUnlistenOptions = {
  scheduleRetry?: ScheduleRetry;
  maxAttempts?: number;
  onExhausted?: () => void;
};

export const SAFE_UNLISTEN_MAX_ATTEMPTS = 8;

export function reportSafeUnlistenExhausted(): void {
  debugRuntimeEvent("windows", "tauri-unlisten.exhausted", {});
}

export function withUnlistenDiagnostics(
  options?: SafeUnlistenOptions,
): SafeUnlistenOptions {
  return {
    ...options,
    onExhausted: options?.onExhausted ?? reportSafeUnlistenExhausted,
  };
}

export function isEarlyUnlistenError(error: unknown): boolean {
  const message =
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : "";
  return message.includes("handlerId");
}

function defaultScheduleRetry(retry: () => void): () => void {
  const channel = new MessageChannel();
  let cancelled = false;
  channel.port1.onmessage = () => {
    if (!cancelled) retry();
  };
  channel.port2.postMessage(null);
  return () => {
    cancelled = true;
    channel.port1.onmessage = null;
  };
}

export function createSafeUnlisten(
  unlisten: () => void | Promise<void>,
  options?: SafeUnlistenOptions,
): () => void {
  const scheduleRetry = options?.scheduleRetry ?? defaultScheduleRetry;
  const maxAttempts = options?.maxAttempts ?? SAFE_UNLISTEN_MAX_ATTEMPTS;
  const onExhausted = options?.onExhausted;
  let started = false;
  let done = false;
  let attempts = 0;
  let cancelRetry: (() => void) | undefined;

  function finish() {
    done = true;
    cancelRetry?.();
    cancelRetry = undefined;
  }

  function handleError(error: unknown) {
    if (done) return;
    if (isEarlyUnlistenError(error) && attempts < maxAttempts) {
      cancelRetry = scheduleRetry(() => {
        attempt();
      });
      return;
    }
    finish();
    if (!isEarlyUnlistenError(error)) {
      throw error;
    }
    onExhausted?.();
  }

  function attempt() {
    if (done) return;
    attempts += 1;
    try {
      const result = unlisten();
      void Promise.resolve(result).then(
        () => {
          finish();
        },
        (error: unknown) => {
          handleError(error);
        },
      );
    } catch (error) {
      handleError(error);
    }
  }

  return () => {
    if (started) return;
    started = true;
    attempt();
  };
}
