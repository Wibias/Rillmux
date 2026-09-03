import { describe, expect, it } from "vitest";
import { subscribePollOverlayHostListeners } from "./pollOverlayHostListeners";

function deferredListen() {
  const byEvent = new Map<
    string,
    {
      handler: ((event: { payload: unknown }) => void) | undefined;
      resolve: ((unlisten: () => void) => void) | undefined;
      reject: ((error: Error) => void) | undefined;
      unlistened: number;
      registered: number;
      queued: "complete" | Error | undefined;
    }
  >();

  function slot(event: string) {
    const existing = byEvent.get(event);
    if (existing) return existing;
    const next = {
      handler: undefined as ((event: { payload: unknown }) => void) | undefined,
      resolve: undefined as ((unlisten: () => void) => void) | undefined,
      reject: undefined as ((error: Error) => void) | undefined,
      unlistened: 0,
      registered: 0,
      queued: undefined as "complete" | Error | undefined,
    };
    byEvent.set(event, next);
    return next;
  }

  function unlistenFor(current: ReturnType<typeof slot>) {
    return () => {
      current.unlistened += 1;
    };
  }

  return {
    listen(event: string, handler: (event: { payload: unknown }) => void) {
      const current = slot(event);
      current.handler = handler;
      current.registered += 1;
      if (current.queued instanceof Error) {
        const error = current.queued;
        current.queued = undefined;
        return Promise.reject(error);
      }
      if (current.queued === "complete") {
        current.queued = undefined;
        return Promise.resolve(unlistenFor(current));
      }
      return new Promise<() => void>((resolve, reject) => {
        current.resolve = resolve;
        current.reject = reject;
      });
    },
    complete(event: string) {
      const current = slot(event);
      if (current.resolve) {
        current.resolve(unlistenFor(current));
        current.resolve = undefined;
        return;
      }
      current.queued = "complete";
    },
    fail(event: string, error: Error) {
      const current = slot(event);
      if (current.reject) {
        current.reject(error);
        current.reject = undefined;
        return;
      }
      current.queued = error;
    },
    emit(event: string, payload: unknown) {
      slot(event).handler?.({ payload });
    },
    unlistened(event: string) {
      return slot(event).unlistened;
    },
    registered(event: string) {
      return slot(event).registered;
    },
  };
}

async function flush() {
  for (let i = 0; i < 10; i += 1) {
    await Promise.resolve();
  }
}

async function waitRegistered(
  events: ReturnType<typeof deferredListen>,
  count: number,
) {
  for (let i = 0; i < 40; i += 1) {
    if (
      events.registered("poll-overlay-ready") >= count &&
      events.registered("poll-overlay-vote-confirmed") >= count
    ) {
      await flush();
      return;
    }
    await Promise.resolve();
  }
  throw new Error("listeners never registered");
}

describe("subscribePollOverlayHostListeners", () => {
  it("owns both listeners after they both succeed", async () => {
    const events = deferredListen();
    const dispose = subscribePollOverlayHostListeners({
      listen: events.listen,
      onReady() {},
      onConfirmedVote() {},
    });
    events.complete("poll-overlay-ready");
    events.complete("poll-overlay-vote-confirmed");
    await waitRegistered(events, 1);
    dispose();
    await flush();
    expect(events.unlistened("poll-overlay-ready")).toBe(1);
    expect(events.unlistened("poll-overlay-vote-confirmed")).toBe(1);
  });

  it("removes the ready listener when confirmed-vote registration rejects", async () => {
    const events = deferredListen();
    subscribePollOverlayHostListeners({
      listen: events.listen,
      onReady() {},
      onConfirmedVote() {},
    });
    events.complete("poll-overlay-ready");
    events.fail("poll-overlay-vote-confirmed", new Error("listen failed"));
    await flush();
    expect(events.unlistened("poll-overlay-ready")).toBe(1);
    expect(events.unlistened("poll-overlay-vote-confirmed")).toBe(0);
  });

  it("removes the confirmed-vote listener when ready registration rejects", async () => {
    const events = deferredListen();
    subscribePollOverlayHostListeners({
      listen: events.listen,
      onReady() {},
      onConfirmedVote() {},
    });
    events.complete("poll-overlay-vote-confirmed");
    events.fail("poll-overlay-ready", new Error("listen failed"));
    await flush();
    expect(events.unlistened("poll-overlay-vote-confirmed")).toBe(1);
    expect(events.unlistened("poll-overlay-ready")).toBe(0);
  });

  it("unlistens listeners that resolve after the owner already disposed", async () => {
    const events = deferredListen();
    const dispose = subscribePollOverlayHostListeners({
      listen: events.listen,
      onReady() {},
      onConfirmedVote() {},
    });
    dispose();
    events.complete("poll-overlay-ready");
    events.complete("poll-overlay-vote-confirmed");
    await waitRegistered(events, 1);
    await flush();
    expect(events.unlistened("poll-overlay-ready")).toBe(1);
    expect(events.unlistened("poll-overlay-vote-confirmed")).toBe(1);
  });

  it("unlistens a listener that resolves after its sibling already failed", async () => {
    const events = deferredListen();
    subscribePollOverlayHostListeners({
      listen: events.listen,
      onReady() {},
      onConfirmedVote() {},
    });
    events.fail("poll-overlay-vote-confirmed", new Error("listen failed"));
    await flush();
    expect(events.unlistened("poll-overlay-ready")).toBe(0);
    events.complete("poll-overlay-ready");
    await flush();
    expect(events.unlistened("poll-overlay-ready")).toBe(1);
  });

  it("cleanup after a successful pair is idempotent", async () => {
    const events = deferredListen();
    const dispose = subscribePollOverlayHostListeners({
      listen: events.listen,
      onReady() {},
      onConfirmedVote() {},
    });
    events.complete("poll-overlay-ready");
    events.complete("poll-overlay-vote-confirmed");
    await waitRegistered(events, 1);
    dispose();
    dispose();
    await flush();
    expect(events.unlistened("poll-overlay-ready")).toBe(1);
    expect(events.unlistened("poll-overlay-vote-confirmed")).toBe(1);
  });

  it("does not accumulate registrations across remounts", async () => {
    const events = deferredListen();
    const first = subscribePollOverlayHostListeners({
      listen: events.listen,
      onReady() {},
      onConfirmedVote() {},
    });
    events.complete("poll-overlay-ready");
    events.complete("poll-overlay-vote-confirmed");
    await waitRegistered(events, 1);
    first();
    await flush();
    const second = subscribePollOverlayHostListeners({
      listen: events.listen,
      onReady() {},
      onConfirmedVote() {},
    });
    events.complete("poll-overlay-ready");
    events.complete("poll-overlay-vote-confirmed");
    await waitRegistered(events, 2);
    second();
    await flush();
    expect(events.registered("poll-overlay-ready")).toBe(2);
    expect(events.registered("poll-overlay-vote-confirmed")).toBe(2);
    expect(events.unlistened("poll-overlay-ready")).toBe(2);
    expect(events.unlistened("poll-overlay-vote-confirmed")).toBe(2);
  });

  it("does not deliver events until both listeners are owned", async () => {
    const events = deferredListen();
    let ready = 0;
    subscribePollOverlayHostListeners({
      listen: events.listen,
      onReady() {
        ready += 1;
      },
      onConfirmedVote() {},
    });
    events.complete("poll-overlay-ready");
    await flush();
    events.emit("poll-overlay-ready", { channel: "forsen" });
    expect(ready).toBe(0);
    events.complete("poll-overlay-vote-confirmed");
    await flush();
    events.emit("poll-overlay-ready", { channel: "forsen" });
    expect(ready).toBe(1);
  });
});
