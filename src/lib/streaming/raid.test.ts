import { describe, expect, it } from "vitest";
import {
  enqueueRaid,
  raidCountdownSeconds,
  raidDedupeKey,
  raidOverlayRect,
  type RaidOutgoingEvent,
} from "./raid";

const base = (over: Partial<RaidOutgoingEvent> = {}): RaidOutgoingEvent => ({
  fromChannel: "alice",
  toChannel: "bob",
  toUserId: "123",
  viewers: 10,
  ...over,
});

describe("raid helpers", () => {
  it("builds a stable dedupe key", () => {
    expect(raidDedupeKey(base())).toBe("alice->bob");
    expect(raidDedupeKey(base({ fromChannel: "Alice", toChannel: "BOB" }))).toBe(
      "alice->bob",
    );
  });

  it("enqueues and lowercases logins", () => {
    const q = enqueueRaid([], base({ fromChannel: "Alice", toChannel: "Bob" }));
    expect(q).toEqual([
      { fromChannel: "alice", toChannel: "bob", toUserId: "123", viewers: 10 },
    ]);
  });

  it("ignores duplicate from→to", () => {
    const q1 = enqueueRaid([], base());
    const q2 = enqueueRaid(q1, base({ viewers: 99 }));
    expect(q2).toHaveLength(1);
    expect(q2[0].viewers).toBe(10);
  });

  it("queues a different from channel", () => {
    const q = enqueueRaid(
      enqueueRaid([], base()),
      base({ fromChannel: "carol", toChannel: "dave", toUserId: "9" }),
    );
    expect(q).toHaveLength(2);
    expect(q.map((e) => e.fromChannel)).toEqual(["alice", "carol"]);
  });
  it("uses the raid-start window when Twitch has not sent a remaining count", () => {
    expect(raidCountdownSeconds(base({ kind: "start" }))).toBe(90);
    expect(raidCountdownSeconds(base({ kind: "start", remainingSeconds: 75 }))).toBe(
      75,
    );
    expect(raidCountdownSeconds(base({ kind: "go" }))).toBe(15);
    expect(raidCountdownSeconds(base())).toBe(15);
  });

  it("keeps a queued raid when the source session ends", () => {
    const queue = enqueueRaid([], base());
    expect(queue).toHaveLength(1);
    expect(queue[0].fromChannel).toBe("alice");
  });

  it("places the overlay on the player, then chat, then the main window", () => {
    expect(
      raidOverlayRect(
        { x: 10, y: 20, width: 800, height: 450 },
        { x: 810, y: 20, width: 300, height: 450 },
        { x: 0, y: 0, width: 1280, height: 800 },
      ),
    ).toEqual({ x: 26, y: 36, width: 420, height: 92 });
    expect(
      raidOverlayRect(
        null,
        { x: 810, y: 20, width: 300, height: 450 },
        { x: 0, y: 0, width: 1280, height: 800 },
      ),
    ).toEqual({ x: 826, y: 36, width: 268, height: 92 });
    expect(
      raidOverlayRect(null, null, { x: 40, y: 80, width: 1280, height: 800 }),
    ).toEqual({ x: 56, y: 96, width: 420, height: 92 });
  });
});