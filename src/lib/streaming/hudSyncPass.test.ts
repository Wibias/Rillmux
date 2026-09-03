import { describe, expect, it } from "vitest";
import { runChannelPointsHudSyncPass } from "./hudSyncPass";
import type { ChannelPointsHudPlace } from "./pointsHud";

const place: ChannelPointsHudPlace = {
  player: { x: 10, y: 20, width: 800, height: 450 },
  captionAvoid: null,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

describe("runChannelPointsHudSyncPass", () => {
  it("does not create HUDs from a stale pass after configuration changes", async () => {
    const website = deferred<{ configured: boolean } | undefined>();
    const ensures: string[] = [];
    let current = true;
    const done = runChannelPointsHudSyncPass({
      isCurrent: () => current,
      hudEnabled: true,
      runningKey: "forsen",
      wanted: [],
      missingSince: {},
      now: () => 0,
      getWebsiteStatus: () => website.promise,
      place: async () => place,
      ensureHud: async (channel) => {
        ensures.push(channel);
        return true;
      },
      getOffset: () => null,
      closeHud: async () => undefined,
    });
    current = false;
    website.resolve({ configured: true });
    const result = await done;
    expect(ensures).toEqual([]);
    expect(result.wanted).toEqual([]);
  });

  it("closes owned HUDs on disable without waiting for website-auth status", async () => {
    const website = deferred<{ configured: boolean } | undefined>();
    const closes: string[] = [];
    const done = runChannelPointsHudSyncPass({
      isCurrent: () => true,
      hudEnabled: false,
      runningKey: "forsen",
      wanted: ["forsen"],
      missingSince: { forsen: 1 },
      now: () => 0,
      getWebsiteStatus: () => website.promise,
      place: async () => place,
      ensureHud: async () => true,
      closeHud: async (channel) => {
        closes.push(channel);
      },
      getOffset: () => null,
    });
    await Promise.resolve();
    expect(closes).toEqual(["forsen"]);
    const result = await done;
    expect(result.wanted).toEqual([]);
    expect(result.missingSince).toEqual({});
  });

  it("keeps an existing HUD during the player-miss grace window", async () => {
    const closes: string[] = [];
    const result = await runChannelPointsHudSyncPass({
      isCurrent: () => true,
      hudEnabled: true,
      runningKey: "forsen",
      wanted: ["forsen"],
      missingSince: {},
      now: () => 1_000,
      getWebsiteStatus: async () => ({ configured: true }),
      place: async () => null,
      ensureHud: async () => true,
      closeHud: async (channel) => {
        closes.push(channel);
      },
      getOffset: () => null,
    });
    expect(closes).toEqual([]);
    expect(result.wanted).toEqual(["forsen"]);
    expect(result.missingSince.forsen).toBe(1_000);
  });
});
