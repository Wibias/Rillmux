import { describe, expect, it } from "vitest";
import {
  createHudGeometryPoller,
  HUD_GEOMETRY_FAST_MS,
  HUD_GEOMETRY_IDLE_MS,
  HUD_GEOMETRY_IDLE_AFTER_STABLE_TICKS,
  hudPlaceUnchanged,
} from "./hudGeometrySampler";
import type { ChannelPointsHudPlace, OverlayRect } from "./pointsHud";

const player: OverlayRect = { x: 100, y: 50, width: 800, height: 450 };
const visible: ChannelPointsHudPlace = { player, captionAvoid: null };

function flush() {
  return Promise.resolve().then(() => Promise.resolve());
}

describe("hudPlaceUnchanged", () => {
  it("treats identical player and caption geometry as unchanged", () => {
    expect(
      hudPlaceUnchanged(visible, {
        player: { ...player },
        captionAvoid: null,
      }),
    ).toBe(true);
    expect(
      hudPlaceUnchanged(visible, {
        ...visible,
        player: { ...player, x: 101 },
      }),
    ).toBe(false);
  });
});

describe("createHudGeometryPoller", () => {
  it("drops to a slow fallback poll after consecutive stationary samples", async () => {
    const delays: number[] = [];
    let placeCalls = 0;
    let scaleCalls = 0;
    const scheduled: Array<() => void> = [];
    const poller = createHudGeometryPoller({
      place: async () => {
        placeCalls += 1;
        return visible;
      },
      scale: async () => {
        scaleCalls += 1;
        return 1;
      },
      schedule(fn, delay) {
        delays.push(delay);
        scheduled.push(fn);
        return delays.length;
      },
      cancel() {},
      onCommit() {},
    });
    poller.start();
    await flush();
    expect(placeCalls).toBe(1);
    expect(scaleCalls).toBe(1);

    for (let i = 0; i < HUD_GEOMETRY_IDLE_AFTER_STABLE_TICKS; i += 1) {
      const tick = scheduled.shift();
      tick?.();
      await flush();
    }
    expect(delays[delays.length - 1]).toBe(HUD_GEOMETRY_IDLE_MS);
    expect(placeCalls).toBe(1 + HUD_GEOMETRY_IDLE_AFTER_STABLE_TICKS);
    expect(scaleCalls).toBe(1);
  });

  it("returns to the fast interval after movement and still converges", async () => {
    let x = 100;
    const scheduled: Array<() => void> = [];
    const delays: number[] = [];
    const poller = createHudGeometryPoller({
      place: async () => ({
        player: { ...player, x },
        captionAvoid: null,
      }),
      scale: async () => 1,
      schedule(fn, delay) {
        delays.push(delay);
        scheduled.push(fn);
        return delays.length;
      },
      cancel() {},
      onCommit() {},
    });
    poller.start();
    await flush();
    for (let i = 0; i < HUD_GEOMETRY_IDLE_AFTER_STABLE_TICKS; i += 1) {
      scheduled.shift()?.();
      await flush();
    }
    expect(delays[delays.length - 1]).toBe(HUD_GEOMETRY_IDLE_MS);
    x = 180;
    scheduled.shift()?.();
    await flush();
    expect(delays[delays.length - 1]).toBe(HUD_GEOMETRY_FAST_MS);
  });

  it("does not let a disposed sample overwrite later geometry", async () => {
    const commits: OverlayRect[] = [];
    const first = deferred<ChannelPointsHudPlace>();
    const scheduled: Array<() => void> = [];
    const poller = createHudGeometryPoller({
      place: () => first.promise,
      scale: async () => 1,
      schedule(fn) {
        scheduled.push(fn);
        return scheduled.length;
      },
      cancel() {},
      onCommit(next) {
        if (next.player) commits.push(next.player);
      },
    });
    poller.start();
    await flush();
    poller.dispose();
    first.resolve(visible);
    await flush();
    expect(commits).toEqual([]);
  });

  it("skips React commits when sampled geometry did not change", async () => {
    let commits = 0;
    const scheduled: Array<() => void> = [];
    const poller = createHudGeometryPoller({
      place: async () => visible,
      scale: async () => 1,
      schedule(fn) {
        scheduled.push(fn);
        return scheduled.length;
      },
      cancel() {},
      onCommit() {
        commits += 1;
      },
    });
    poller.start();
    await flush();
    expect(commits).toBe(1);
    scheduled.shift()?.();
    await flush();
    expect(commits).toBe(1);
  });

  it("wakes immediately from idle when a layout signal nudges it", async () => {
    const delays: number[] = [];
    let placeCalls = 0;
    const scheduled: Array<() => void> = [];
    const poller = createHudGeometryPoller({
      place: async () => {
        placeCalls += 1;
        return visible;
      },
      scale: async () => 1,
      schedule(fn, delay) {
        delays.push(delay);
        scheduled.push(fn);
        return delays.length;
      },
      cancel() {},
      onCommit() {},
    });
    poller.start();
    await flush();
    for (let i = 0; i < HUD_GEOMETRY_IDLE_AFTER_STABLE_TICKS; i += 1) {
      scheduled.shift()?.();
      await flush();
    }
    expect(delays[delays.length - 1]).toBe(HUD_GEOMETRY_IDLE_MS);
    const idlePlaceCalls = placeCalls;
    poller.nudge();
    await flush();
    expect(placeCalls).toBe(idlePlaceCalls + 1);
    expect(delays[delays.length - 1]).toBe(HUD_GEOMETRY_FAST_MS);
  });

  it("does not overlap placement samples when IPC is slow", async () => {
    let inflight = 0;
    let maxInflight = 0;
    let calls = 0;
    const first = deferred<ChannelPointsHudPlace>();
    const scheduled: Array<() => void> = [];
    const poller = createHudGeometryPoller({
      place: () => {
        inflight += 1;
        maxInflight = Math.max(maxInflight, inflight);
        calls += 1;
        const finish = () => {
          inflight -= 1;
        };
        if (calls === 1) return first.promise.finally(finish);
        finish();
        return Promise.resolve(visible);
      },
      scale: async () => 1,
      schedule(fn) {
        scheduled.push(fn);
        return scheduled.length;
      },
      cancel() {},
      onCommit() {},
    });
    poller.start();
    await flush();
    poller.nudge();
    await flush();
    expect(maxInflight).toBe(1);
    first.resolve(visible);
    await flush();
    expect(maxInflight).toBe(1);
  });

  it("keeps committing geometry when nudges arrive faster than slow placement IPC", async () => {
    const inflightSamples: number[] = [];
    let maxInflight = 0;
    let placeCalls = 0;
    const commits: OverlayRect[] = [];
    const pending: Array<(value: ChannelPointsHudPlace) => void> = [];
    const poller = createHudGeometryPoller({
      place: () => {
        placeCalls += 1;
        inflightSamples.push(1);
        maxInflight = Math.max(maxInflight, inflightSamples.length);
        return new Promise<ChannelPointsHudPlace>((resolve) => {
          pending.push((value) => {
            inflightSamples.pop();
            resolve(value);
          });
        });
      },
      scale: async () => 1,
      schedule() {
        return 1;
      },
      cancel() {},
      onCommit(next) {
        if (next.player) commits.push(next.player);
      },
    });
    poller.start();
    await flush();
    expect(placeCalls).toBe(1);

    poller.nudge();
    poller.nudge();
    poller.nudge();
    await flush();
    expect(placeCalls).toBe(1);
    expect(maxInflight).toBe(1);

    pending.shift()?.({
      player: { ...player, x: 120 },
      captionAvoid: null,
    });
    await flush();
    expect(commits.length).toBeGreaterThan(0);
    expect(placeCalls).toBe(2);

    poller.nudge();
    poller.nudge();
    await flush();
    expect(placeCalls).toBe(2);
    pending.shift()?.({
      player: { ...player, x: 180 },
      captionAvoid: null,
    });
    await flush();
    expect(commits.length).toBeGreaterThanOrEqual(2);
    expect(placeCalls).toBeLessThanOrEqual(3);
    expect(maxInflight).toBe(1);
    expect(pending.length).toBeLessThanOrEqual(1);
  });

  it("does not commit or queue more placement after dispose during coalesced nudges", async () => {
    let placeCalls = 0;
    const commits: OverlayRect[] = [];
    const pending: Array<(value: ChannelPointsHudPlace) => void> = [];
    const poller = createHudGeometryPoller({
      place: () => {
        placeCalls += 1;
        return new Promise<ChannelPointsHudPlace>((resolve) => {
          pending.push(resolve);
        });
      },
      scale: async () => 1,
      schedule() {
        return 1;
      },
      cancel() {},
      onCommit(next) {
        if (next.player) commits.push(next.player);
      },
    });
    poller.start();
    await flush();
    poller.nudge();
    poller.nudge();
    poller.dispose();
    pending.shift()?.({
      player: { ...player, x: 140 },
      captionAvoid: null,
    });
    await flush();
    expect(commits).toEqual([]);
    expect(placeCalls).toBe(1);
  });

  it("does not schedule more work after dispose during an in-flight sample", async () => {
    const first = deferred<ChannelPointsHudPlace>();
    const delays: number[] = [];
    const scheduled: Array<() => void> = [];
    const poller = createHudGeometryPoller({
      place: () => first.promise,
      scale: async () => 1,
      schedule(fn, delay) {
        delays.push(delay);
        scheduled.push(fn);
        return delays.length;
      },
      cancel() {},
      onCommit() {},
    });
    poller.start();
    await flush();
    const scheduledBeforeDispose = scheduled.length;
    poller.dispose();
    first.resolve(visible);
    await flush();
    expect(scheduled.length).toBe(scheduledBeforeDispose);
  });

  it("counts native work for one stationary HUD versus eight", async () => {
    function run(hudCount: number) {
      const stats = { place: 0, scale: 0 };
      const scheduled: Array<() => void> = [];
      const pollers = Array.from({ length: hudCount }, () =>
        createHudGeometryPoller({
          place: async () => {
            stats.place += 1;
            return visible;
          },
          scale: async () => {
            stats.scale += 1;
            return 1;
          },
          schedule(fn) {
            scheduled.push(fn);
            return scheduled.length;
          },
          cancel() {},
          onCommit() {},
        }),
      );
      return { stats, scheduled, pollers };
    }

    const one = run(1);
    one.pollers.forEach((poller) => poller.start());
    await flush();
    const eight = run(8);
    eight.pollers.forEach((poller) => poller.start());
    await flush();

    const beforeOnePlacePerSecond = 1000 / HUD_GEOMETRY_FAST_MS;
    const beforeEightPlacePerSecond = 8 * beforeOnePlacePerSecond;
    const afterIdleOne = 1000 / HUD_GEOMETRY_IDLE_MS;
    const afterIdleEight = 8 * afterIdleOne;

    expect(one.stats.place).toBe(1);
    expect(one.stats.scale).toBe(1);
    expect(eight.stats.place).toBe(8);
    expect(eight.stats.scale).toBe(8);
    expect(afterIdleOne).toBeLessThan(beforeOnePlacePerSecond);
    expect(afterIdleEight).toBeLessThan(beforeEightPlacePerSecond);
    expect(afterIdleEight).toBe(4);
    expect(beforeEightPlacePerSecond).toBe(32);

    const hostSyncPlaceHz = 1;
    const beforeHostOne = hostSyncPlaceHz;
    const beforeHostEight = 8 * hostSyncPlaceHz;
    const afterHostOne = hostSyncPlaceHz;
    const afterHostEight = 8 * hostSyncPlaceHz;
    expect(beforeOnePlacePerSecond + beforeHostOne).toBe(5);
    expect(beforeEightPlacePerSecond + beforeHostEight).toBe(40);
    expect(afterIdleOne + afterHostOne).toBe(1.5);
    expect(afterIdleEight + afterHostEight).toBe(12);
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
