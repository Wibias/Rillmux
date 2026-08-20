import { describe, expect, it } from "vitest";
import {
  POLL_FALLBACK_REFRESH_MS,
  overlayRectMoved,
  pollOverlayRect,
  pollOverlayShouldPollGql,
} from "./pollOverlay";

describe("pollOverlayRect", () => {
  it("sits at the bottom of owned chat, not the video host", () => {
    expect(
      pollOverlayRect(
        { x: 810, y: 20, width: 300, height: 450 },
        { x: 0, y: 0, width: 1280, height: 800 },
      ),
    ).toEqual({ x: 822, y: 118, width: 276, height: 340 });
  });

  it("falls back to the main-window chat column when Chatterino is absent", () => {
    expect(
      pollOverlayRect(null, { x: 900, y: 80, width: 340, height: 700 }),
    ).toEqual({ x: 912, y: 428, width: 316, height: 340 });
  });

  it("does not invent a host when chat is not on screen", () => {
    expect(pollOverlayRect(null, null)).toBeNull();
  });

  it("ignores tiny overlay jitter so the window is not moved every tick", () => {
    const a = { x: 800, y: 100, width: 300, height: 240 };
    expect(overlayRectMoved(a, { ...a, x: 801, y: 102 })).toBe(false);
    expect(overlayRectMoved(a, { ...a, x: 820 })).toBe(true);
  });
});

describe("poll overlay GQL", () => {
  it("lets the host fall back slowly; the overlay window only consumes Hermes pushes", () => {
    expect(POLL_FALLBACK_REFRESH_MS).toBe(60_000);
    expect(pollOverlayShouldPollGql(false)).toBe(true);
    expect(pollOverlayShouldPollGql(true)).toBe(false);
  });
});
