import { describe, expect, it } from "vitest";
import {
  POINTS_REFRESH_INTERVAL_MS,
  PRESENCE_STATUS_FALLBACK_MS,
  buildPresenceTargets,
  describeViewerPresenceStatus,
  presenceSourceFromStream,
  prunePresenceMetadata,
  shouldRefreshChannelPoints,
  type PresenceMetadata,
  type PresenceSession,
} from "./presence";

const metadata: PresenceMetadata = {
  one: {
    channelLogin: "one",
    channelId: "101",
    broadcastId: "broadcast-one",
  },
  two: {
    channelLogin: "two",
    channelId: "202",
    broadcastId: "broadcast-two",
  },
  three: {
    channelLogin: "three",
    channelId: "303",
    broadcastId: "broadcast-three",
  },
  stale: {
    channelLogin: "stale",
    channelId: "404",
    broadcastId: "broadcast-stale",
  },
};

const sessions: PresenceSession[] = [
  { id: "one", running: true, ready: true },
  { id: "two", running: true, ready: true },
  { id: "three", running: true, ready: true },
  { id: "starting", running: true, ready: false },
  { id: "ended", running: false, ready: true },
];

describe("viewer presence lifecycle", () => {
  it("prunes metadata for sessions the backend no longer owns", () => {
    const pruned = prunePresenceMetadata(metadata, sessions);

    expect(Object.keys(pruned)).toEqual(["one", "two", "three"]);
    expect(pruned.stale).toBeUndefined();
  });

  it("selects every ready running session up to the layout cap", () => {
    const targets = buildPresenceTargets(sessions, metadata);

    expect(targets).toEqual([
      {
        sessionId: "one",
        channelLogin: "one",
        channelId: "101",
        broadcastId: "broadcast-one",
      },
      {
        sessionId: "two",
        channelLogin: "two",
        channelId: "202",
        broadcastId: "broadcast-two",
      },
      {
        sessionId: "three",
        channelLogin: "three",
        channelId: "303",
        broadcastId: "broadcast-three",
      },
    ]);
  });

  it("uses the stable multistream slot order instead of backend map order", () => {
    const targets = buildPresenceTargets(sessions, metadata, [
      "three",
      "one",
      "two",
    ]);

    expect(targets.map((target) => target.sessionId)).toEqual([
      "three",
      "one",
      "two",
    ]);
  });

  it("caps presence workers at eight ready streams", () => {
    const manySessions: PresenceSession[] = Array.from({ length: 9 }, (_, index) => ({
      id: `s${index + 1}`,
      running: true,
      ready: true,
    }));
    const manyMetadata: PresenceMetadata = Object.fromEntries(
      manySessions.map((session, index) => [
        session.id,
        {
          channelLogin: session.id,
          channelId: String(100 + index),
          broadcastId: `broadcast-${session.id}`,
        },
      ]),
    );

    const targets = buildPresenceTargets(manySessions, manyMetadata);
    expect(targets).toHaveLength(8);
    expect(targets.map((target) => target.sessionId)).toEqual(
      manySessions.slice(0, 8).map((session) => session.id),
    );
  });

  it("ignores incomplete Twitch identifiers", () => {
    const targets = buildPresenceTargets(
      [{ id: "broken", running: true, ready: true }],
      {
        broken: {
          channelLogin: "broken",
          channelId: "",
          broadcastId: "",
        },
      },
    );

    expect(targets).toEqual([]);
  });

  it("builds presence metadata from a complete Helix stream", () => {
    expect(
      presenceSourceFromStream({
        id: "broadcast-9",
        user_id: "999",
        user_login: "Alice",
      }),
    ).toEqual({
      channelLogin: "alice",
      channelId: "999",
      broadcastId: "broadcast-9",
    });
  });

  it("does not farm from a stub stream with empty Helix ids", () => {
    expect(
      presenceSourceFromStream({
        id: "",
        user_id: "",
        user_login: "alice",
      }),
    ).toBeNull();
  });

  it("describes the exact failing backend protocol stage", () => {
    expect(
      describeViewerPresenceStatus({
        enabled: true,
        activeSessionIds: ["one"],
        limited: false,
        workers: [
          {
            sessionId: "one",
            channelLogin: "one",
            lastStage: "playback-token",
            lastHttpStatus: 401,
            lastError: "Twitch rejected the playback-token request",
            lastSuccessUnixMs: null,
          },
        ],
      }),
    ).toBe(
      "one: playback-token HTTP 401 — Twitch rejected the playback-token request",
    );
  });
});

describe("channel points status timing", () => {
  it("does not poll presence every few seconds; balance stays on a slower cadence", () => {
    expect(PRESENCE_STATUS_FALLBACK_MS).toBeGreaterThanOrEqual(30_000);
    expect(POINTS_REFRESH_INTERVAL_MS).toBeGreaterThanOrEqual(15_000);
    expect(shouldRefreshChannelPoints(0, 14_999, false)).toBe(false);
    expect(shouldRefreshChannelPoints(0, 15_000, false)).toBe(true);
    expect(shouldRefreshChannelPoints(10_000, 20_000, true)).toBe(false);
  });
});
