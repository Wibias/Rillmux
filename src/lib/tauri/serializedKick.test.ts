import { describe, expect, it } from "vitest";
import { createSerializedKick } from "./serializedKick";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

describe("createSerializedKick", () => {
  it("runs one pass at a time and coalesces overlapping kicks into a single follow-up", async () => {
    const gate = deferred<void>();
    let started = 0;
    let finished = 0;
    const kick = createSerializedKick(async () => {
      started += 1;
      await gate.promise;
      finished += 1;
    });

    void kick.kick();
    void kick.kick();
    void kick.kick();
    await Promise.resolve();
    expect(started).toBe(1);
    expect(kick.inflight).toBe(true);

    gate.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(started).toBe(2);
    expect(finished).toBe(2);
    expect(kick.inflight).toBe(false);
  });

  it("ignores stale async work after the generation changes", async () => {
    const gate = deferred<void>();
    const mutations: string[] = [];
    let pass = 0;
    const kick = createSerializedKick(async (isCurrent) => {
      const label = pass === 0 ? "first" : "second";
      pass += 1;
      await gate.promise;
      if (!isCurrent()) return;
      mutations.push(label);
    });

    void kick.kick();
    await Promise.resolve();
    kick.invalidate();
    void kick.kick();
    gate.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(mutations).toEqual(["second"]);
  });

  it("does not apply a disposed pass", async () => {
    const gate = deferred<void>();
    let mutated = false;
    const kick = createSerializedKick(async (isCurrent) => {
      await gate.promise;
      if (!isCurrent()) return;
      mutated = true;
    });
    void kick.kick();
    await Promise.resolve();
    kick.dispose();
    gate.resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(mutated).toBe(false);
    expect(kick.inflight).toBe(false);
  });
});
