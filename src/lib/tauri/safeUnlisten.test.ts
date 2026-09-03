import { describe, expect, it } from "vitest";
import {
  createSafeUnlisten,
  isEarlyUnlistenError,
} from "./safeUnlisten";

function tauriEarlyUnlistenError() {
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

describe("createSafeUnlisten exhaustion", () => {
  it("emits no exhaustion signal when the first unlisten succeeds", async () => {
    const retries = createRetryQueue();
    const exhausted: string[] = [];
    let backend = 0;
    const stop = createSafeUnlisten(
      () => {
        backend += 1;
      },
      {
        maxAttempts: 2,
        scheduleRetry: retries.scheduleRetry,
        onExhausted: () => exhausted.push("exhausted"),
      },
    );
    stop();
    await Promise.resolve();
    expect(backend).toBe(1);
    expect(exhausted).toEqual([]);
    expect(retries.pending()).toBe(0);
  });

  it("emits no exhaustion signal when an early unlisten later succeeds", async () => {
    const retries = createRetryQueue();
    const exhausted: string[] = [];
    let ready = false;
    let backend = 0;
    const stop = createSafeUnlisten(
      () => {
        if (!ready) throw tauriEarlyUnlistenError();
        backend += 1;
      },
      {
        maxAttempts: 3,
        scheduleRetry: retries.scheduleRetry,
        onExhausted: () => exhausted.push("exhausted"),
      },
    );
    stop();
    await Promise.resolve();
    ready = true;
    retries.flush();
    await Promise.resolve();
    expect(backend).toBe(1);
    expect(exhausted).toEqual([]);
    expect(retries.pending()).toBe(0);
  });

  it("stops after the configured attempt count and emits exhaustion once", async () => {
    const retries = createRetryQueue();
    const exhausted: string[] = [];
    let calls = 0;
    const stop = createSafeUnlisten(
      () => {
        calls += 1;
        throw tauriEarlyUnlistenError();
      },
      {
        maxAttempts: 3,
        scheduleRetry: retries.scheduleRetry,
        onExhausted: () => exhausted.push("exhausted"),
      },
    );
    stop();
    await Promise.resolve();
    expect(calls).toBe(1);
    retries.flush();
    await Promise.resolve();
    expect(calls).toBe(2);
    retries.flush();
    await Promise.resolve();
    expect(calls).toBe(3);
    expect(exhausted).toEqual(["exhausted"]);
    expect(retries.pending()).toBe(0);
  });

  it("does not emit exhaustion twice when dispose is repeated during retries", async () => {
    const retries = createRetryQueue();
    const exhausted: string[] = [];
    let calls = 0;
    const stop = createSafeUnlisten(
      () => {
        calls += 1;
        throw tauriEarlyUnlistenError();
      },
      {
        maxAttempts: 2,
        scheduleRetry: retries.scheduleRetry,
        onExhausted: () => exhausted.push("exhausted"),
      },
    );
    stop();
    stop();
    stop();
    await Promise.resolve();
    retries.flush();
    await Promise.resolve();
    stop();
    expect(calls).toBe(2);
    expect(exhausted).toEqual(["exhausted"]);
    expect(retries.pending()).toBe(0);
  });

  it("does not treat an unrelated unlisten error as retry exhaustion", async () => {
    const retries = createRetryQueue();
    const exhausted: string[] = [];
    const stop = createSafeUnlisten(
      () => {
        throw new Error("disk full");
      },
      {
        maxAttempts: 2,
        scheduleRetry: retries.scheduleRetry,
        onExhausted: () => exhausted.push("exhausted"),
      },
    );
    expect(() => stop()).toThrow("disk full");
    expect(isEarlyUnlistenError(new Error("disk full"))).toBe(false);
    expect(exhausted).toEqual([]);
    expect(retries.pending()).toBe(0);
  });
});
