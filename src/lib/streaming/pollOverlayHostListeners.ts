import { ownAsyncSubscription } from "../tauri/ownAsyncSubscription";
import { createOwnedListenerSet } from "../tauri/ownedListenerSet";

export const POLL_OVERLAY_READY_EVENT = "poll-overlay-ready";
export const POLL_OVERLAY_VOTE_CONFIRMED_EVENT = "poll-overlay-vote-confirmed";

type EventHandler<T> = (event: { payload: T }) => void;

export function subscribePollOverlayHostListeners<TReady, TVote>(deps: {
  listen: (event: string, handler: EventHandler<unknown>) => Promise<() => void>;
  onReady: (payload: TReady) => void;
  onConfirmedVote: (payload: TVote) => void;
}): () => void {
  const set = createOwnedListenerSet();
  let handlersLive = false;
  return ownAsyncSubscription(async () => {
    const dispose = await set.bind([
      () =>
        deps.listen(POLL_OVERLAY_READY_EVENT, (event) => {
          if (!handlersLive) return;
          deps.onReady(event.payload as TReady);
        }),
      () =>
        deps.listen(POLL_OVERLAY_VOTE_CONFIRMED_EVENT, (event) => {
          if (!handlersLive) return;
          deps.onConfirmedVote(event.payload as TVote);
        }),
    ]);
    handlersLive = true;
    return () => {
      handlersLive = false;
      dispose();
    };
  });
}
