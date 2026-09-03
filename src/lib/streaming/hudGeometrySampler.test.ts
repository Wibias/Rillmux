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

  it("does not let a stale sample overwrite newer geometry", async () => {
    const commits: OverlayRect[] = [];
    const first = deferred<ChannelPointsHudPlace>();
    const second: ChannelPointsHudPlace = {
      player: { ...player, x: 400 },
      captionAvoid: null,
    };
    let calls = 0;
    const scheduled: Array<() => void> = [];
    const poller = createHudGeometryPoller({
      place: () => {
        calls += 1;
        if (calls === 1) return first.promise;
        return Promise.resolve(second);
      },
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
    poller.nudge();
    await flush();
    first.resolve(visible);
    await flush();
    expect(commits).toEqual([second.player]);
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
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
