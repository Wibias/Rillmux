import { describe, expect, it } from "vitest";
import { ownAsyncSubscription } from "./ownAsyncSubscription";
import { createOwnedListenerSet } from "./ownedListenerSet";

function controllableRegister() {
  let resolveRegister: ((unlisten: () => void) => void) | undefined;
  let rejectRegister: ((error: Error) => void) | undefined;
  let unlistened = 0;
  const registered = new Promise<() => void>((resolve, reject) => {
    resolveRegister = resolve;
    rejectRegister = reject;
  });
  return {
    register: () => registered,
    unlistened: () => unlistened,
    complete() {
      resolveRegister?.(() => {
        unlistened += 1;
      });
    },
    fail(error: Error) {
      rejectRegister?.(error);
    },
  };
}

describe("createOwnedListenerSet", () => {
  it("owns every listener after a complete successful registration", async () => {
    const first = controllableRegister();
    const second = controllableRegister();
    const set = createOwnedListenerSet();
    const bound = set.bind([first.register, second.register]);
    first.complete();
    second.complete();
    const dispose = await bound;
    expect(set.bound).toBe(true);
    dispose();
    expect(first.unlistened()).toBe(1);
    expect(second.unlistened()).toBe(1);
    expect(set.bound).toBe(false);
  });

  it("removes already-registered listeners when a later registration fails", async () => {
    const first = controllableRegister();
    const second = controllableRegister();
    const set = createOwnedListenerSet();
    const bound = set.bind([first.register, second.register]);
    first.complete();
    second.fail(new Error("listen failed"));
    await expect(bound).rejects.toThrow("listen failed");
    expect(first.unlistened()).toBe(1);
    expect(set.bound).toBe(false);
  });

  it("can bind normally after a failed attempt", async () => {
    const failing = controllableRegister();
    const set = createOwnedListenerSet();
    const firstAttempt = set.bind([failing.register]);
    failing.fail(new Error("listen failed"));
    await expect(firstAttempt).rejects.toThrow("listen failed");

    const retry = controllableRegister();
    const secondAttempt = set.bind([retry.register]);
    retry.complete();
    const dispose = await secondAttempt;
    expect(set.bound).toBe(true);
    dispose();
    expect(retry.unlistened()).toBe(1);
  });

  it("releases listeners when dispose runs before registration resolves", async () => {
    const first = controllableRegister();
    const set = createOwnedListenerSet();
    let bindPromise!: Promise<() => void>;
    const dispose = ownAsyncSubscription(() => {
      bindPromise = set.bind([first.register]);
      return bindPromise;
    });
    dispose();
    first.complete();
    await bindPromise;
    await Promise.resolve();
    expect(first.unlistened()).toBe(1);
    expect(set.bound).toBe(false);
  });

  it("does not create a second listener set for concurrent bind attempts", async () => {
    let registerCalls = 0;
    const first = controllableRegister();
    const set = createOwnedListenerSet();
    const register = () => {
      registerCalls += 1;
      return first.register();
    };
    const a = set.bind([register]);
    const b = set.bind([register]);
    first.complete();
    const disposeA = await a;
    const disposeB = await b;
    expect(registerCalls).toBe(1);
    expect(set.bound).toBe(true);
    disposeA();
    expect(first.unlistened()).toBe(0);
    expect(set.bound).toBe(true);
    disposeB();
    expect(first.unlistened()).toBe(1);
    expect(set.bound).toBe(false);
  });

  it("makes owner cleanup idempotent", async () => {
    const first = controllableRegister();
    const set = createOwnedListenerSet();
    const bound = set.bind([first.register]);
    first.complete();
    const dispose = await bound;
    dispose();
    dispose();
    expect(first.unlistened()).toBe(1);
    expect(set.bound).toBe(false);
  });

  it("rolls back immediately when a sibling hangs after another registration fails", async () => {
    const first = controllableRegister();
    const second = controllableRegister();
    const hang = { register: () => new Promise<() => void>(() => {}) };
    const set = createOwnedListenerSet();
    const bound = set.bind([first.register, second.register, hang.register]);
    first.complete();
    second.fail(new Error("listen failed"));
    const outcome = await Promise.race([
      bound.then(
        () => "resolved" as const,
        (error: unknown) => error,
      ),
      new Promise<"timeout">((resolve) => {
        setTimeout(() => resolve("timeout"), 50);
      }),
    ]);
    expect(outcome).toBeInstanceOf(Error);
    expect((outcome as Error).message).toBe("listen failed");
    expect(first.unlistened()).toBe(1);
    expect(set.bound).toBe(false);
  });

  it("removes the later listener when the first registration rejects", async () => {
    const first = controllableRegister();
    const second = controllableRegister();
    const set = createOwnedListenerSet();
    const bound = set.bind([first.register, second.register]);
    second.complete();
    first.fail(new Error("listen failed"));
    await expect(bound).rejects.toThrow("listen failed");
    expect(second.unlistened()).toBe(1);
    expect(set.bound).toBe(false);
  });

  it("unlistens a late success that arrives after the attempt already failed", async () => {
    const late = controllableRegister();
    const failing = controllableRegister();
    const set = createOwnedListenerSet();
    const bound = set.bind([late.register, failing.register]);
    failing.fail(new Error("listen failed"));
    await expect(bound).rejects.toThrow("listen failed");
    expect(late.unlistened()).toBe(0);
    late.complete();
    await Promise.resolve();
    await Promise.resolve();
    expect(late.unlistened()).toBe(1);
    expect(set.bound).toBe(false);
  });
});
