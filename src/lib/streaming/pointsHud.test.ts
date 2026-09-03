import { describe, expect, it } from "vitest";
import {
  POINTS_HUD_CHIP_HEIGHT,
  POINTS_HUD_DEFAULT_INSET,
  POINTS_HUD_PAD,
  catalogRectForChip,
  catalogSideForChip,
  chipRectForPlayer,
  overlayRectsOverlap,
  isPointsHudOverlay,
  movementIsDrag,
  offsetFromChipRect,
  overlayRectForHud,
  hudDragSurfaceRect,
  hudGeometryTransitionNeedsConceal,
  chipRectFromDrag,
  physicalDeltaFromScreen,
  cssPx,
  playerTooSmallForHud,
  pointsHudChannelFromSearch,
  pointsHudLabel,
  pointsHudOverlayUrl,
  hudOffsetFromSearch,
  hudOffsetFromUnknown,
  hudOffsetsEqual,
  hudKeepOnPlayerMiss,
  hudSyncRunningKey,
  PLAYER_LAYOUT_CHANGED_EVENT,
  PLAYER_MISS_GRACE_MS,
  playerLayoutChangedTargetsChannel,
  rewardUnavailableReason,
  sortCustomRewards,
} from "./pointsHud";

const player = { x: 100, y: 50, width: 800, height: 450 };

describe("points HUD URL", () => {
  it("parses overlay=points-hud and a channel login", () => {
    expect(isPointsHudOverlay("?overlay=points-hud&channel=Forsen")).toBe(true);
    expect(isPointsHudOverlay("?overlay=raid")).toBe(false);
    expect(
      pointsHudChannelFromSearch("?overlay=points-hud&channel=Forsen"),
    ).toBe("forsen");
    expect(pointsHudChannelFromSearch("?overlay=points-hud")).toBeNull();
    expect(pointsHudOverlayUrl("Forsen")).toBe(
      "/?overlay=points-hud&channel=forsen&hudOffset=default",
    );
    expect(pointsHudOverlayUrl("Forsen", { x: 0.25, y: 0.4 })).toBe(
      "/?overlay=points-hud&channel=forsen&ox=0.25&oy=0.4",
    );
    expect(pointsHudLabel("Forsen")).toBe("points-hud-forsen");
  });

  it("reads a stamped offset from the overlay URL, beating a stale settings.json", () => {
    expect(hudOffsetFromSearch("?overlay=points-hud&channel=forsen")).toEqual({
      found: false,
      offset: null,
    });
    expect(
      hudOffsetFromSearch(
        "?overlay=points-hud&channel=forsen&hudOffset=default",
      ),
    ).toEqual({ found: true, offset: null });
    expect(
      hudOffsetFromSearch("?overlay=points-hud&channel=forsen&ox=0.25&oy=0.4"),
    ).toEqual({ found: true, offset: { x: 0.25, y: 0.4 } });
  });
});

describe("hudKeepOnPlayerMiss", () => {
  it("hides immediately when the player is minimized, not after the retile grace", () => {
    expect(hudKeepOnPlayerMiss("hidden", 0)).toBe(false);
    expect(hudKeepOnPlayerMiss("hidden", PLAYER_MISS_GRACE_MS - 1)).toBe(false);
    expect(hudKeepOnPlayerMiss("missing", 0)).toBe(true);
    expect(hudKeepOnPlayerMiss("missing", PLAYER_MISS_GRACE_MS - 1)).toBe(true);
    expect(hudKeepOnPlayerMiss("missing", PLAYER_MISS_GRACE_MS)).toBe(false);
    expect(hudKeepOnPlayerMiss("visible", PLAYER_MISS_GRACE_MS)).toBe(true);
  });
});

describe("player layout change events", () => {
  it("names the native movement signal the HUD listens for", () => {
    expect(PLAYER_LAYOUT_CHANGED_EVENT).toBe("player-layout-changed");
  });

  it("wakes only the HUD whose player moved", () => {
    expect(playerLayoutChangedTargetsChannel("Forsen", "forsen")).toBe(true);
    expect(playerLayoutChangedTargetsChannel(" xqc ", "xqc")).toBe(true);
    expect(playerLayoutChangedTargetsChannel("xqc", "forsen")).toBe(false);
    expect(playerLayoutChangedTargetsChannel(undefined, "forsen")).toBe(false);
    expect(playerLayoutChangedTargetsChannel("  ", "forsen")).toBe(false);
  });
});

describe("hudOffsetsEqual", () => {
  it("treats null as the default and parses event payloads", () => {
    expect(hudOffsetsEqual(null, null)).toBe(true);
    expect(hudOffsetsEqual(null, { x: 0.2, y: 0.3 })).toBe(false);
    expect(hudOffsetsEqual({ x: 0.2, y: 0.3 }, { x: 0.2, y: 0.3 })).toBe(true);
    expect(hudOffsetFromUnknown(null)).toBeNull();
    expect(hudOffsetFromUnknown({ x: 0.2, y: 0.3 })).toEqual({
      x: 0.2,
      y: 0.3,
    });
    expect(hudOffsetFromUnknown({ x: 1.5, y: -0.2 })).toEqual({ x: 1, y: 0 });
  });
});

describe("chipRectForPlayer", () => {
  it("defaults to the top-right inset", () => {
    expect(chipRectForPlayer(player, null, 120)).toEqual({
      x: 100 + 800 - 120 - POINTS_HUD_DEFAULT_INSET,
      y: 50 + POINTS_HUD_DEFAULT_INSET,
      width: 120,
      height: POINTS_HUD_CHIP_HEIGHT,
    });
  });

  it("places from a fraction offset and clamps overflow", () => {
    const placed = chipRectForPlayer(player, { x: 0.5, y: 0.25 }, 120);
    expect(placed.x).toBe(100 + 400);
    expect(placed.y).toBe(50 + 112.5);
    const overflow = chipRectForPlayer(player, { x: 1, y: 1 }, 120);
    expect(overflow.x).toBe(100 + 800 - 120 - 8);
    expect(overflow.y).toBe(50 + 450 - POINTS_HUD_CHIP_HEIGHT - 8);
  });

  it("round-trips a dragged chip to a fraction offset", () => {
    const chip = chipRectForPlayer(player, { x: 0.2, y: 0.3 }, 120);
    expect(offsetFromChipRect(player, chip)).toEqual({
      x: (chip.x - 100) / 800,
      y: (chip.y - 50) / 450,
    });
  });

  it("parks the default chip on the stream top edge, left of min/max/close", () => {
    const full = { x: 0, y: 0, width: 1280, height: 800 };
    const caption = { x: 1280 - 138, y: 0, width: 138, height: 38 };
    const chip = chipRectForPlayer(full, null, 120, caption);
    expect(chip.x).toBe(caption.x - 120 - POINTS_HUD_DEFAULT_INSET);
    expect(chip.y).toBe(full.y + POINTS_HUD_DEFAULT_INSET);
    expect(chip.x + chip.width).toBeLessThanOrEqual(caption.x);
    expect(overlayRectsOverlap(chip, caption)).toBe(false);
  });

  it("leaves a saved offset in place even when it sits in the caption keepout", () => {
    const full = { x: 0, y: 0, width: 1280, height: 800 };
    const caption = { x: 1280 - 138, y: 0, width: 138, height: 38 };
    const under = chipRectForPlayer(full, { x: 0.9, y: 0.08 }, 120, caption);
    expect(under.x).toBe(full.x + 0.9 * full.width);
    expect(under.y).toBe(full.y + 0.08 * full.height);
  });

  it("parks on the stream even when chat sits under the caption buttons", () => {
    const video = { x: 0, y: 38, width: 1000, height: 800 };
    const caption = { x: 1280 - 138, y: 0, width: 138, height: 38 };
    expect(overlayRectsOverlap(video, caption)).toBe(false);
    const chip = chipRectForPlayer(video, null, 120, caption);
    expect(chip.x + chip.width).toBeLessThanOrEqual(
      video.x + video.width - POINTS_HUD_DEFAULT_INSET,
    );
    expect(chip.x).toBeGreaterThanOrEqual(video.x);
    expect(chip.y).toBe(video.y + POINTS_HUD_DEFAULT_INSET);
    expect(overlayRectsOverlap(chip, caption)).toBe(false);
  });

  it("uses the player top-right when caption buttons do not overlap the stream", () => {
    const tile = { x: 640, y: 38, width: 640, height: 400 };
    const caption = { x: 1280 - 138, y: 0, width: 138, height: 38 };
    expect(overlayRectsOverlap(tile, caption)).toBe(false);
    const chip = chipRectForPlayer(tile, null, 120, caption);
    expect(chip.x).toBe(tile.x + tile.width - 120 - POINTS_HUD_DEFAULT_INSET);
    expect(chip.y).toBe(tile.y + POINTS_HUD_DEFAULT_INSET);
  });

  it("round-trips a chip at the top edge, left of the window controls, even when the stream sits below the title bar", () => {
    const tile = { x: 0, y: 38, width: 1540, height: 994 };
    const caption = { x: 1540 - 138, y: 0, width: 138, height: 38 };
    const placed = {
      x: 1261,
      y: 54,
      width: 120,
      height: POINTS_HUD_CHIP_HEIGHT,
    };
    const offset = offsetFromChipRect(tile, placed, caption);
    const restored = chipRectForPlayer(tile, offset, 120, caption);
    expect(restored.x).toBe(placed.x);
    expect(restored.y).toBe(placed.y);
    expect(restored.x + restored.width).toBeLessThanOrEqual(caption.x);
    expect(overlayRectsOverlap(restored, caption)).toBe(false);
  });

  it("still parks a bottom-right tile on that stream, not on the title bar", () => {
    const tile = { x: 640, y: 438, width: 640, height: 400 };
    const caption = { x: 1280 - 138, y: 0, width: 138, height: 38 };
    expect(overlayRectsOverlap(tile, caption)).toBe(false);
    const chip = chipRectForPlayer(tile, null, 120, caption);
    expect(chip.x).toBe(tile.x + tile.width - 120 - POINTS_HUD_DEFAULT_INSET);
    expect(chip.y).toBe(tile.y + POINTS_HUD_DEFAULT_INSET);
    expect(chip.y).toBeGreaterThan(caption.y + caption.height);
  });

  it("parks on the player top edge, left of that player's window controls", () => {
    const stream = { x: 0, y: 38, width: 1000, height: 800 };
    const caption = { x: 1000 - 138, y: 38, width: 138, height: 38 };
    expect(overlayRectsOverlap(stream, caption)).toBe(true);
    const chip = chipRectForPlayer(stream, null, 120, caption);
    expect(chip.x).toBe(caption.x - 120 - POINTS_HUD_DEFAULT_INSET);
    expect(chip.y).toBe(stream.y + POINTS_HUD_DEFAULT_INSET);
    expect(chip.x + chip.width).toBeLessThanOrEqual(caption.x);
  });

  it("keeps a saved offset instead of snapping back to the caption park", () => {
    const full = { x: 0, y: 0, width: 1280, height: 800 };
    const caption = { x: 1280 - 138, y: 0, width: 138, height: 38 };
    const chip = chipRectForPlayer(full, { x: 0.9, y: 0.08 }, 120, caption);
    expect(chip.x).toBe(full.x + 0.9 * full.width);
    expect(chip.y).toBe(full.y + 0.08 * full.height);
  });
});

describe("chipRectFromDrag", () => {
  it("follows a physical pointer delta and clamps to the player", () => {
    const origin = chipRectForPlayer(player, { x: 0.2, y: 0.3 }, 120);
    const dragged = chipRectFromDrag(player, origin, 40, -10);
    expect(dragged.x).toBe(origin.x + 40);
    expect(dragged.y).toBe(origin.y - 10);
    expect(dragged.width).toBe(origin.width);
    const clamped = chipRectFromDrag(player, origin, 10_000, 10_000);
    expect(clamped.x + clamped.width).toBeLessThanOrEqual(
      player.x + player.width,
    );
    expect(clamped.y + clamped.height).toBeLessThanOrEqual(
      player.y + player.height,
    );
  });

  it("keeps a drag on the stream instead of entering the title-bar row", () => {
    const tile = { x: 0, y: 38, width: 1540, height: 994 };
    const caption = { x: 1540 - 138, y: 0, width: 138, height: 38 };
    const origin = { x: 1261, y: 70, width: 120, height: 36 };
    const dragged = chipRectFromDrag(tile, origin, 0, -80, caption);
    expect(dragged.y).toBeGreaterThanOrEqual(tile.y);
    expect(dragged.y).toBe(tile.y + POINTS_HUD_PAD);
    expect(overlayRectsOverlap(dragged, caption)).toBe(false);
  });

  it("does not snap toward the caption park while dragging", () => {
    const tile = { x: 640, y: 38, width: 640, height: 400 };
    const origin = { x: 1000, y: 70, width: 120, height: 36 };
    const dragged = chipRectFromDrag(tile, origin, 80, 0);
    expect(dragged.x).toBeGreaterThan(origin.x);
    expect(dragged.y).toBe(origin.y);
  });

  it("converts screen CSS pixels to physical HWND pixels", () => {
    expect(physicalDeltaFromScreen(100, 50, 110, 60, 1.25)).toEqual({
      dx: 12.5,
      dy: 12.5,
    });
    expect(physicalDeltaFromScreen(10, 10, 10, 16, 1)).toEqual({
      dx: 0,
      dy: 6,
    });
  });
});

describe("playerTooSmallForHud", () => {
  it("hides when the player is under 200×120", () => {
    expect(playerTooSmallForHud({ x: 0, y: 0, width: 199, height: 120 })).toBe(
      true,
    );
    expect(playerTooSmallForHud({ x: 0, y: 0, width: 200, height: 119 })).toBe(
      true,
    );
    expect(playerTooSmallForHud({ x: 0, y: 0, width: 200, height: 120 })).toBe(
      false,
    );
  });
});

describe("movementIsDrag", () => {
  it("treats movement of 6px or less as a click", () => {
    expect(movementIsDrag(6, 0)).toBe(false);
    expect(movementIsDrag(0, 6)).toBe(false);
    expect(movementIsDrag(4, 5)).toBe(true);
    expect(movementIsDrag(6.1, 0)).toBe(true);
  });
});

describe("catalog placement", () => {
  it("prefers left and down, then flips to stay on the player", () => {
    const chip = chipRectForPlayer(player, null, 120);
    const side = catalogSideForChip(player, chip, 280, 360);
    expect(side.openLeft).toBe(true);
    expect(side.openDown).toBe(true);
    const panel = catalogRectForChip(player, chip, 280, 360);
    expect(panel.x + panel.width).toBe(chip.x + chip.width);
    expect(panel.y).toBe(chip.y + chip.height);

    const lowChip = {
      x: 108,
      y: player.y + player.height - 40,
      width: 120,
      height: 36,
    };
    const flipped = catalogSideForChip(player, lowChip, 280, 360);
    expect(flipped.openDown).toBe(false);
    const up = catalogRectForChip(player, lowChip, 280, 360);
    expect(up.y + up.height).toBeLessThanOrEqual(lowChip.y + 0.5);
  });

  it("converts physical overlay pixels to CSS pixels", () => {
    expect(cssPx(150, 1.25)).toBe(120);
    expect(cssPx(36, 1)).toBe(36);
    expect(cssPx(100, 0)).toBe(100);
  });

  it("unions chip and panel into the overlay window", () => {
    const chip = { x: 700, y: 60, width: 120, height: 36 };
    const panel = { x: 540, y: 96, width: 280, height: 200 };
    expect(overlayRectForHud(chip, panel)).toEqual({
      x: 540,
      y: 60,
      width: 280,
      height: 236,
    });
  });

  it("keeps a stable drag surface on the player tile only", () => {
    const tile = { x: 640, y: 38, width: 640, height: 400 };
    const caption = { x: 1142, y: 0, width: 138, height: 38 };
    const size = { width: 120, height: 36 };
    const surface = hudDragSurfaceRect(tile, caption, size);
    expect(hudDragSurfaceRect(tile, caption, size)).toEqual(surface);
    expect(surface).toEqual(tile);
  });
});

describe("rewards", () => {
  it("sorts redeemable first, then by cost", () => {
    const sorted = sortCustomRewards([
      { cost: 100, redeemable: false },
      { cost: 500, redeemable: true },
      { cost: 50, redeemable: true },
    ]);
    expect(sorted.map((r) => r.cost)).toEqual([50, 500, 100]);
  });

  it("uses a string key so Zustand snapshots stay referentially stable", () => {
    const sessions = [
      { running: true, channel: "Forsen" },
      { running: false, channel: "lirik" },
      { running: true, channel: "xQc" },
    ];
    expect(hudSyncRunningKey(sessions)).toBe("forsen|xqc");
    expect(hudSyncRunningKey(sessions)).toBe(hudSyncRunningKey(sessions));
    expect(hudSyncRunningKey([])).toBe("");
  });

  it("explains why a reward cannot be redeemed", () => {
    const base = {
      paused: false,
      enabled: true,
      inStock: true,
      cooldownSeconds: 0,
      cost: 100,
      balance: 200,
    };
    expect(rewardUnavailableReason(base)).toBeNull();
    expect(rewardUnavailableReason({ ...base, paused: true })).toBe("paused");
    expect(rewardUnavailableReason({ ...base, enabled: false })).toBe(
      "disabled",
    );
    expect(rewardUnavailableReason({ ...base, inStock: false })).toBe(
      "outOfStock",
    );
    expect(rewardUnavailableReason({ ...base, cooldownSeconds: 12 })).toBe(
      "cooldown",
    );
    expect(rewardUnavailableReason({ ...base, balance: 50 })).toBe("notEnough");
  });
});

describe("HUD geometry transition concealment", () => {
  it("conceals a catalog opening that shifts the native HUD window left", () => {
    const chip = chipRectForPlayer(player, null, 120);
    const closed = overlayRectForHud(chip, null);
    const panel = catalogRectForChip(player, chip, 280, 360);
    const open = overlayRectForHud(chip, panel);
    expect(open.x).toBeLessThan(closed.x);
    expect(hudGeometryTransitionNeedsConceal(closed, open)).toBe(true);
    expect(hudGeometryTransitionNeedsConceal(open, closed)).toBe(true);
  });

  it("does not conceal a resize whose native origin stays fixed", () => {
    const chip = { x: 108, y: 66, width: 120, height: 36 };
    const panel = catalogRectForChip(player, chip, 280, 300);
    const closed = overlayRectForHud(chip, null);
    const open = overlayRectForHud(chip, panel);
    expect(open.x).toBe(closed.x);
    expect(open.y).toBe(closed.y);
    expect(hudGeometryTransitionNeedsConceal(closed, open)).toBe(false);
  });

  it("keeps same-size position changes visible for dragging", () => {
    const current = { x: 700, y: 60, width: 120, height: 36 };
    const next = { ...current, x: 640 };
    expect(hudGeometryTransitionNeedsConceal(current, next)).toBe(false);
  });
});
