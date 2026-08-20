/** Pure helpers for outgoing-raid prompt queueing. */

export interface RaidOutgoingEvent {
  fromChannel: string;
  toChannel: string;
  toUserId: string;
  viewers?: number;
}

export function raidDedupeKey(e: RaidOutgoingEvent): string {
  return `${e.fromChannel.toLowerCase()}->${e.toChannel.toLowerCase()}`;
}

/** Normalize logins; drop duplicate from→to already in the queue. */
export function enqueueRaid(
  queue: RaidOutgoingEvent[],
  next: RaidOutgoingEvent,
): RaidOutgoingEvent[] {
  const normalized: RaidOutgoingEvent = {
    fromChannel: next.fromChannel.toLowerCase(),
    toChannel: next.toChannel.toLowerCase(),
    toUserId: next.toUserId,
    viewers: next.viewers,
  };
  const key = raidDedupeKey(normalized);
  if (queue.some((e) => raidDedupeKey(e) === key)) {
    return queue;
  }
  return [...queue, normalized];
}

export interface OverlayRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

const OVERLAY_WIDTH = 420;
const OVERLAY_HEIGHT = 92;
const OVERLAY_INSET = 16;

/** Prefer the raiding player, then owned chat, then the main window. */
export function raidOverlayRect(
  player: OverlayRect | null,
  chat: OverlayRect | null,
  main: OverlayRect | null,
): OverlayRect | null {
  const host = player ?? chat ?? main;
  if (!host) return null;
  const width = Math.max(240, Math.min(OVERLAY_WIDTH, host.width - OVERLAY_INSET * 2));
  return {
    x: Math.round(host.x + OVERLAY_INSET),
    y: Math.round(host.y + OVERLAY_INSET),
    width: Math.round(width),
    height: OVERLAY_HEIGHT,
  };
}
