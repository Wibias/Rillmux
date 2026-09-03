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
});
