import type { OverlayRect } from "./raid";

export type { OverlayRect };

export const POINTS_HUD_CHIP_HEIGHT = 36;
export const POINTS_HUD_CHIP_MIN_WIDTH = 120;
export const POINTS_HUD_CHIP_MAX_WIDTH = 220;
export const POINTS_HUD_DEFAULT_INSET = 16;
export const POINTS_HUD_PAD = 8;
export const POINTS_HUD_CATALOG_MAX_WIDTH = 280;
export const POINTS_HUD_CATALOG_MAX_HEIGHT = 360;
export const POINTS_HUD_DRAG_THRESHOLD_PX = 6;
export const POINTS_HUD_MIN_PLAYER_WIDTH = 200;
export const POINTS_HUD_MIN_PLAYER_HEIGHT = 120;
/** Ignore HWND/DWM jitter so the overlay does not chase 1–8px player-rect noise. */
export const POINTS_HUD_MOVE_SLOP = 12;
export const POINTS_HUD_OFFSET_EVENT = "channel-points-hud-offset";

export type HudOffset = { x: number; y: number } | null;

export type CatalogSide = {
  openLeft: boolean;
  openDown: boolean;
};

export type RewardUnavailableReason =
  | "paused"
  | "disabled"
  | "outOfStock"
  | "cooldown"
  | "notEnough";

export function isPointsHudOverlay(search = window.location.search): boolean {
  return new URLSearchParams(search).get("overlay") === "points-hud";
}

export function pointsHudChannelFromSearch(
  search = window.location.search,
): string | null {
  const channel = new URLSearchParams(search)
    .get("channel")
    ?.trim()
    .toLowerCase();
  if (!channel || channel.length > 25) return null;
  if (!/^[a-z0-9_]+$/.test(channel)) return null;
  return channel;
}

export function pointsHudOverlayUrl(
  channel: string,
  offset: HudOffset = null,
): string {
  const login = channel.trim().toLowerCase();
  const params = new URLSearchParams({
    overlay: "points-hud",
    channel: login,
  });
  // Stamp the offset into the URL so a remounted overlay does not hydrate a
  // stale settings.json value (reset → go back to the stream).
  if (!offset) {
    params.set("hudOffset", "default");
  } else {
    params.set("ox", String(offset.x));
    params.set("oy", String(offset.y));
  }
  return `/?${params.toString()}`;
}

/** Offset stamped on the overlay URL. `found: false` means the URL is silent. */
export function hudOffsetFromSearch(search = window.location.search): {
  found: boolean;
  offset: HudOffset;
} {
  const params = new URLSearchParams(search);
  if (params.get("hudOffset") === "default") {
    return { found: true, offset: null };
  }
  const ox = params.get("ox");
  const oy = params.get("oy");
  if (ox == null || oy == null) return { found: false, offset: null };
  const x = Number(ox);
  const y = Number(oy);
  if (!Number.isFinite(x) || !Number.isFinite(y)) {
    return { found: false, offset: null };
  }
  return { found: true, offset: clampHudOffset({ x, y }) };
}

export function pointsHudLabel(channel: string): string {
  return `points-hud-${channel.trim().toLowerCase()}`;
}

export function playerTooSmallForHud(player: OverlayRect): boolean {
  return (
    player.width < POINTS_HUD_MIN_PLAYER_WIDTH ||
    player.height < POINTS_HUD_MIN_PLAYER_HEIGHT
  );
}

export function clampHudOffset(offset: HudOffset): HudOffset {
  if (!offset) return null;
  if (!Number.isFinite(offset.x) || !Number.isFinite(offset.y)) return null;
  return {
    x: Math.min(1, Math.max(0, offset.x)),
    y: Math.min(1, Math.max(0, offset.y)),
  };
}

export function hudOffsetFromUnknown(raw: unknown): HudOffset {
  if (raw == null || typeof raw !== "object") return null;
  const value = raw as { x?: unknown; y?: unknown };
  if (typeof value.x !== "number" || typeof value.y !== "number") return null;
  return clampHudOffset({ x: value.x, y: value.y });
}

export function hudOffsetsEqual(a: HudOffset, b: HudOffset): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return a.x === b.x && a.y === b.y;
}

function fitChipSize(player: OverlayRect, chipWidth: number): OverlayRect {
  const width = Math.max(
    1,
    Math.min(
      Math.max(POINTS_HUD_CHIP_MIN_WIDTH, chipWidth),
      POINTS_HUD_CHIP_MAX_WIDTH,
      player.width - POINTS_HUD_PAD * 2,
    ),
  );
  const height = Math.max(
    1,
    Math.min(POINTS_HUD_CHIP_HEIGHT, player.height - POINTS_HUD_PAD * 2),
  );
  return { x: 0, y: 0, width, height };
}

function clampRectInBox(
  box: OverlayRect,
  rect: OverlayRect,
  padding = POINTS_HUD_PAD,
): OverlayRect {
  const maxX = box.x + box.width - rect.width - padding;
  const maxY = box.y + box.height - rect.height - padding;
  const minX = box.x + padding;
  const minY = box.y + padding;
  return {
    x: Math.min(Math.max(rect.x, minX), Math.max(minX, maxX)),
    y: Math.min(Math.max(rect.y, minY), Math.max(minY, maxY)),
    width: rect.width,
    height: rect.height,
  };
}

/**
 * Offsets are fractions of the stream tile. Do not grow this into the Rillmux
 * title bar — that parks the overlay HWND on the app chrome instead of mpv.
 */
export function hudHostRect(
  player: OverlayRect,
  _captionAvoid: OverlayRect | null,
): OverlayRect {
  return { ...player };
}

export type ChannelPointsHudPlace = {
  player: OverlayRect;
  captionAvoid: OverlayRect | null;
};

export function overlayRectsOverlap(a: OverlayRect, b: OverlayRect): boolean {
  return (
    a.x < b.x + b.width &&
    a.x + a.width > b.x &&
    a.y < b.y + b.height &&
    a.y + a.height > b.y
  );
}

/**
 * Same row as the player window controls: stream top edge, left of min/max/close.
 */
function parkChipOnStreamLeftOfCaption(
  player: OverlayRect,
  chip: OverlayRect,
  avoid: OverlayRect,
): OverlayRect {
  const minX = player.x + POINTS_HUD_DEFAULT_INSET;
  const maxX = player.x + player.width - chip.width - POINTS_HUD_DEFAULT_INSET;
  const x = Math.max(
    minX,
    Math.min(maxX, avoid.x - chip.width - POINTS_HUD_DEFAULT_INSET),
  );
  const y = player.y + POINTS_HUD_DEFAULT_INSET;
  return { ...chip, x, y };
}

/** Caption buttons plus the strip directly underneath them. */
export function captionKeepoutRect(
  avoid: OverlayRect,
  chipHeight = POINTS_HUD_CHIP_HEIGHT,
): OverlayRect {
  return {
    ...avoid,
    height: avoid.height + POINTS_HUD_DEFAULT_INSET + chipHeight,
  };
}

export function chipInCaptionKeepout(
  chip: OverlayRect,
  avoid: OverlayRect | null,
): boolean {
  if (!avoid) return false;
  return overlayRectsOverlap(chip, captionKeepoutRect(avoid, chip.height));
}

/** Keep the chip out of the player's own min/max/close column. */
function nudgeChipOffCaption(
  player: OverlayRect,
  chip: OverlayRect,
  avoid: OverlayRect | null,
): OverlayRect {
  if (
    !avoid ||
    !overlayRectsOverlap(avoid, player) ||
    !chipInCaptionKeepout(chip, avoid)
  ) {
    return chip;
  }
  return parkChipOnStreamLeftOfCaption(player, chip, avoid);
}

export function chipRectForPlayer(
  player: OverlayRect,
  offset: HudOffset,
  chipWidth: number,
  captionAvoid: OverlayRect | null = null,
): OverlayRect {
  const size = fitChipSize(player, chipWidth);
  // Only the player's own min/max/close. A Rillmux caption on another monitor
  // must not yank the chip to the left edge of the stream.
  const parked =
    !offset && captionAvoid && overlayRectsOverlap(captionAvoid, player)
      ? parkChipOnStreamLeftOfCaption(
          player,
          { x: 0, y: 0, width: size.width, height: size.height },
          captionAvoid,
        )
      : null;
  if (parked) return parked;
  const host = hudHostRect(player, captionAvoid);
  const clampedOffset = clampHudOffset(offset);
  const unclamped = clampedOffset
    ? {
        x: host.x + clampedOffset.x * host.width,
        y: host.y + clampedOffset.y * host.height,
        width: size.width,
        height: size.height,
      }
    : {
        x: player.x + player.width - size.width - POINTS_HUD_DEFAULT_INSET,
        y: player.y + POINTS_HUD_DEFAULT_INSET,
        width: size.width,
        height: size.height,
      };
  const rect = clampRectInBox(clampedOffset ? host : player, unclamped);
  if (!clampedOffset) {
    return nudgeChipOffCaption(player, rect, captionAvoid);
  }
  return rect;
}

export function offsetFromChipRect(
  player: OverlayRect,
  chip: OverlayRect,
  captionAvoid: OverlayRect | null = null,
): { x: number; y: number } {
  const host = hudHostRect(player, captionAvoid);
  const width = Math.max(1, host.width);
  const height = Math.max(1, host.height);
  return {
    x: Math.min(1, Math.max(0, (chip.x - host.x) / width)),
    y: Math.min(1, Math.max(0, (chip.y - host.y) / height)),
  };
}

/** Follow the pointer in HWND pixels. Do not park beside caption buttons. */
export function chipRectFromDrag(
  player: OverlayRect,
  origin: OverlayRect,
  dxPhysical: number,
  dyPhysical: number,
  captionAvoid: OverlayRect | null = null,
): OverlayRect {
  return clampRectInBox(hudHostRect(player, captionAvoid), {
    ...origin,
    x: origin.x + dxPhysical,
    y: origin.y + dyPhysical,
  });
}

/** `screenX`/`screenY` are CSS pixels; overlay geometry is physical. */
export function physicalDeltaFromScreen(
  startX: number,
  startY: number,
  x: number,
  y: number,
  scale: number,
): { dx: number; dy: number } {
  const factor = Number.isFinite(scale) && scale > 0 ? scale : 1;
  return {
    dx: (x - startX) * factor,
    dy: (y - startY) * factor,
  };
}

export function movementIsDrag(dx: number, dy: number): boolean {
  return Math.hypot(dx, dy) > POINTS_HUD_DRAG_THRESHOLD_PX;
}

export function catalogPanelSize(
  player: OverlayRect,
  panelWidth = POINTS_HUD_CATALOG_MAX_WIDTH,
  panelHeight = POINTS_HUD_CATALOG_MAX_HEIGHT,
): { width: number; height: number } {
  return {
    width: Math.max(1, Math.min(panelWidth, player.width - POINTS_HUD_PAD * 2)),
    height: Math.max(
      1,
      Math.min(panelHeight, player.height - POINTS_HUD_PAD * 2),
    ),
  };
}

export function catalogSideForChip(
  player: OverlayRect,
  chip: OverlayRect,
  panelWidth: number,
  panelHeight: number,
): CatalogSide {
  const pad = POINTS_HUD_PAD;
  const leftX = chip.x + chip.width - panelWidth;
  const openLeft = leftX >= player.x + pad;
  const downY = chip.y + chip.height;
  const openDown = downY + panelHeight <= player.y + player.height - pad;
  return { openLeft, openDown };
}

export function catalogRectForChip(
  player: OverlayRect,
  chip: OverlayRect,
  panelWidth = POINTS_HUD_CATALOG_MAX_WIDTH,
  panelHeight = POINTS_HUD_CATALOG_MAX_HEIGHT,
): OverlayRect {
  const size = catalogPanelSize(player, panelWidth, panelHeight);
  const side = catalogSideForChip(player, chip, size.width, size.height);
  const x = side.openLeft ? chip.x + chip.width - size.width : chip.x;
  const y = side.openDown ? chip.y + chip.height : chip.y - size.height;
  return clampRectInBox(player, {
    x,
    y,
    width: size.width,
    height: size.height,
  });
}

/**
 * Moving a transparent HUD window's origin while resizing can expose a stale
 * WebView2 frame for one compositor tick. Conceal only transitions where the
 * native origin and size both change; ordinary drag/follow movement stays live.
 */
export function hudGeometryTransitionNeedsConceal(
  current: OverlayRect,
  next: OverlayRect,
): boolean {
  const originMoved =
    Math.round(current.x) !== Math.round(next.x) ||
    Math.round(current.y) !== Math.round(next.y);
  const sizeChanged =
    Math.round(current.width) !== Math.round(next.width) ||
    Math.round(current.height) !== Math.round(next.height);
  return originMoved && sizeChanged;
}

/** Convert a physical overlay length (HWND pixels) into CSS pixels. */
export function cssPx(physical: number, scale: number): number {
  const factor = Number.isFinite(scale) && scale > 0 ? scale : 1;
  return Math.round(physical / factor);
}

export function overlayRectForHud(
  chip: OverlayRect,
  panel: OverlayRect | null,
): OverlayRect {
  if (!panel) return { ...chip };
  const left = Math.min(chip.x, panel.x);
  const top = Math.min(chip.y, panel.y);
  const right = Math.max(chip.x + chip.width, panel.x + panel.width);
  const bottom = Math.max(chip.y + chip.height, panel.y + panel.height);
  return {
    x: left,
    y: top,
    width: right - left,
    height: bottom - top,
  };
}

/**
 * Drag surface is the stream tile only. Expanding into the title bar puts the
 * overlay HWND on the Rillmux window instead of mpv.
 */
export function hudDragSurfaceRect(
  player: OverlayRect,
  captionAvoid: OverlayRect | null,
  _chipSize: { width: number; height: number },
): OverlayRect {
  return hudHostRect(player, captionAvoid);
}

/** Map chip-local coords into the union overlay window. */
export function chipOriginInOverlay(
  overlay: OverlayRect,
  chip: OverlayRect,
): { x: number; y: number } {
  return { x: chip.x - overlay.x, y: chip.y - overlay.y };
}

export function sortCustomRewards<
  T extends { cost: number; redeemable: boolean },
>(rewards: T[]): T[] {
  return [...rewards].sort((a, b) => {
    if (a.redeemable !== b.redeemable) return a.redeemable ? -1 : 1;
    if (a.cost !== b.cost) return a.cost - b.cost;
    return 0;
  });
}

/**
 * Stable Zustand snapshot for HUD sync. Returning a new array from a
 * selector makes React 19 loop (`Maximum update depth exceeded`) and
 * leaves a blank grey window after the splash dismisses.
 */
export function hudSyncRunningKey(
  sessions: readonly { running: boolean; channel: string }[],
): string {
  return sessions
    .flatMap((session) =>
      session.running ? [session.channel.toLowerCase()] : [],
    )
    .join("|");
}

export function rewardUnavailableReason(opts: {
  paused: boolean;
  enabled: boolean;
  inStock: boolean;
  cooldownSeconds: number;
  cost: number;
  balance: number;
}): RewardUnavailableReason | null {
  if (!opts.enabled) return "disabled";
  if (opts.paused) return "paused";
  if (!opts.inStock) return "outOfStock";
  if (opts.cooldownSeconds > 0) return "cooldown";
  if (opts.cost > opts.balance) return "notEnough";
  return null;
}
