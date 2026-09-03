import { listen, type EventCallback } from "@tauri-apps/api/event";
import {
  createSafeUnlisten,
  type SafeUnlistenOptions,
} from "./safeUnlisten";

export type OwnedUnlisten = () => void | Promise<void>;

/** Owns an async-registered cleanup so dispose-before-resolve still unlistens. */
export function ownAsyncSubscription(
  register: () => Promise<OwnedUnlisten>,
  options?: SafeUnlistenOptions,
): () => void {
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  let unlistened = false;

  void register().then(
    (fn) => {
      const stop = createSafeUnlisten(fn, options);
      if (cancelled) {
        stop();
        unlistened = true;
        return;
      }
      unlisten = stop;
    },
    () => undefined,
  );

  return () => {
    cancelled = true;
    if (unlistened) return;
    unlisten?.();
    unlistened = true;
    unlisten = undefined;
  };
}

export function listenWhileMounted<T>(
  event: string,
  handler: EventCallback<T>,
): () => void {
  return ownAsyncSubscription(() => listen(event, handler));
}
