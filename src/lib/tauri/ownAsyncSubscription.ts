import { listen, type EventCallback } from "@tauri-apps/api/event";

/** Owns an async-registered cleanup so dispose-before-resolve still unlistens. */
export function ownAsyncSubscription(
  register: () => Promise<() => void>,
): () => void {
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  let unlistened = false;

  void register().then(
    (fn) => {
      if (cancelled) {
        fn();
        unlistened = true;
        return;
      }
      unlisten = fn;
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
