import type { OverlayRect } from "./raid";

export type { OverlayRect };

/** Host GQL fallback after Hermes pushes the live poll/prediction. */
export const POLL_FALLBACK_REFRESH_MS = 60_000;

export function pollOverlayShouldPollGql(overlayWindow: boolean): boolean {
  return !overlayWindow;
}

const POLL_OVERLAY_WIDTH = 360;
const POLL_OVERLAY_HEIGHT = 340;
const POLL_OVERLAY_INSET = 12;

export function overlayRectMoved(
  a: OverlayRect,
  b: OverlayRect,
  slop = 4,
): boolean {
  return (
    Math.abs(a.x - b.x) > slop ||
    Math.abs(a.y - b.y) > slop ||
    Math.abs(a.width - b.width) > slop ||
    Math.abs(a.height - b.height) > slop
  );
}

/** Sit over owned Chatterino or the in-app chat column — never over video. */
export function pollOverlayRect(
  chat: OverlayRect | null,
  main: OverlayRect | null,
): OverlayRect | null {
  const host = chat ?? main;
  if (!host) return null;
  const width = Math.max(
    200,
    Math.min(POLL_OVERLAY_WIDTH, host.width - POLL_OVERLAY_INSET * 2),
  );
  const height = Math.max(
    160,
    Math.min(POLL_OVERLAY_HEIGHT, host.height - POLL_OVERLAY_INSET * 2),
  );
  return {
    x: Math.round(host.x + POLL_OVERLAY_INSET),
    y: Math.round(host.y + host.height - height - POLL_OVERLAY_INSET),
    width: Math.round(width),
    height: Math.round(height),
  };
}
