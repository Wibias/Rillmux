import { describe, expect, it } from "vitest";
import { ownAsyncSubscription } from "./ownAsyncSubscription";

function deferredUnlisten() {
  let resolveRegister: ((unlisten: () => void) => void) | undefined;
  let unlistened = 0;
  const registered = new Promise<() => void>((resolve) => {
    resolveRegister = resolve;
  });
  return {
    register: () => registered,
    unlistened: () => unlistened,
    complete() {
      resolveRegister?.(() => {
        unlistened += 1;
      });
    },
  };
}

export function tauriEarlyUnlistenError() {
  return new TypeError(
    "undefined is not an object (evaluating 'listeners[eventId].handlerId')",
  );
}

function createRetryQueue() {
  const queued: Array<() => void> = [];
  return {
    scheduleRetry(retry: () => void) {
      queued.push(retry);
      return () => {
        const index = queued.indexOf(retry);
        if (index >= 0) queued.splice(index, 1);
      };
    },
    flush() {
      const jobs = queued.splice(0);
      for (const job of jobs) job();
    },
    pending() {
      return queued.length;
    },
  };
}

function createEarlyUnlisten() {
  let ready = false;
  let backend = 0;
  let calls = 0;
  return {
    unlisten() {
      calls += 1;
      if (!ready) {
        throw tauriEarlyUnlistenError();
      }
      backend += 1;
    },
    becomeReady() {
      ready = true;
    },
    calls: () => calls,
    backend: () => backend,
  };
}

describe("ownAsyncSubscription", () => {
  it("unlistens after a normal mount and dispose", async () => {
    const sub = deferredUnlisten();
    const dispose = ownAsyncSubscription(sub.register);
    sub.complete();
    await Promise.resolve();
    dispose();
    expect(sub.unlistened()).toBe(1);
  });

  it("unlistens when dispose runs before registration resolves", async () => {
    const sub = deferredUnlisten();
    const dispose = ownAsyncSubscription(sub.register);
    dispose();
    expect(sub.unlistened()).toBe(0);
    sub.complete();
    await Promise.resolve();
    expect(sub.unlistened()).toBe(1);
  });

  it("does not unlisten twice after dispose-before-resolve plus a later dispose", async () => {
    const sub = deferredUnlisten();
    const dispose = ownAsyncSubscription(sub.register);
    dispose();
    sub.complete();
    await Promise.resolve();
    dispose();
    expect(sub.unlistened()).toBe(1);
  });

  it("retries dispose-before-resolve until the Tauri registry entry exists", async () => {
    const retries = createRetryQueue();
    const listener = createEarlyUnlisten();
    const dispose = ownAsyncSubscription(
      () => Promise.resolve(() => listener.unlisten()),
      { scheduleRetry: retries.scheduleRetry },
    );
    dispose();
    await Promise.resolve();
    await Promise.resolve();
    expect(listener.calls()).toBe(1);
    expect(listener.backend()).toBe(0);
    listener.becomeReady();
    retries.flush();
    await Promise.resolve();
    expect(listener.backend()).toBe(1);
    expect(retries.pending()).toBe(0);
  });

  it("cleans up on the first unlisten when the registry is already ready", async () => {
    const retries = createRetryQueue();
    const listener = createEarlyUnlisten();
    listener.becomeReady();
    const dispose = ownAsyncSubscription(
      () => Promise.resolve(() => listener.unlisten()),
      { scheduleRetry: retries.scheduleRetry },
    );
    await Promise.resolve();
    dispose();
    await Promise.resolve();
    expect(listener.backend()).toBe(1);
    expect(retries.pending()).toBe(0);
  });

  it("retries a throw-once unlisten until backend deregistration succeeds", async () => {
    const retries = createRetryQueue();
    const listener = createEarlyUnlisten();
    const dispose = ownAsyncSubscription(
      () => Promise.resolve(() => listener.unlisten()),
      { scheduleRetry: retries.scheduleRetry },
    );
    await Promise.resolve();
    dispose();
    await Promise.resolve();
    expect(listener.backend()).toBe(0);
    listener.becomeReady();
    retries.flush();
    await Promise.resolve();
    expect(listener.backend()).toBe(1);
  });

  it("repeated disposal stays idempotent while an early unlisten is retrying", async () => {
    const retries = createRetryQueue();
    const listener = createEarlyUnlisten();
    const dispose = ownAsyncSubscription(
      () => Promise.resolve(() => listener.unlisten()),
      { scheduleRetry: retries.scheduleRetry },
    );
    await Promise.resolve();
    dispose();
    dispose();
    await Promise.resolve();
    expect(listener.calls()).toBe(1);
    listener.becomeReady();
    retries.flush();
    await Promise.resolve();
    dispose();
    expect(listener.backend()).toBe(1);
    expect(listener.calls()).toBe(2);
  });

  it("does not let a stale subscription retry touch a replacement listener", async () => {
    const retries = createRetryQueue();
    const first = createEarlyUnlisten();
    const second = createEarlyUnlisten();
    second.becomeReady();
    const disposeFirst = ownAsyncSubscription(
      () => Promise.resolve(() => first.unlisten()),
      { scheduleRetry: retries.scheduleRetry },
    );
    await Promise.resolve();
    disposeFirst();
    await Promise.resolve();
    const disposeSecond = ownAsyncSubscription(
      () => Promise.resolve(() => second.unlisten()),
      { scheduleRetry: retries.scheduleRetry },
    );
    await Promise.resolve();
    first.becomeReady();
    retries.flush();
    await Promise.resolve();
    expect(first.backend()).toBe(1);
    expect(second.backend()).toBe(0);
    disposeSecond();
    await Promise.resolve();
    expect(second.backend()).toBe(1);
  });

  it("does not retry an unrelated unlisten failure", async () => {
    const retries = createRetryQueue();
    const dispose = ownAsyncSubscription(
      () =>
        Promise.resolve(() => {
          throw new Error("disk full");
        }),
      { scheduleRetry: retries.scheduleRetry },
    );
    await Promise.resolve();
    expect(() => dispose()).toThrow("disk full");
    expect(retries.pending()).toBe(0);
  });

  it("retries when unlisten rejects asynchronously with the registry TypeError", async () => {
    const retries = createRetryQueue();
    let ready = false;
    let backend = 0;
    const dispose = ownAsyncSubscription(
      () =>
        Promise.resolve(() => {
          if (!ready) {
            return Promise.reject(tauriEarlyUnlistenError());
          }
          backend += 1;
        }),
      { scheduleRetry: retries.scheduleRetry },
    );
    await Promise.resolve();
    dispose();
    await Promise.resolve();
    await Promise.resolve();
    expect(backend).toBe(0);
    ready = true;
    retries.flush();
    await Promise.resolve();
    await Promise.resolve();
    expect(backend).toBe(1);
  });
});
