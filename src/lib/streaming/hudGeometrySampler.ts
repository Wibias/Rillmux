import type { ChannelPointsHudPlace, OverlayRect } from "./pointsHud";

export const HUD_GEOMETRY_FAST_MS = 250;
export const HUD_GEOMETRY_IDLE_MS = 2000;
export const HUD_GEOMETRY_IDLE_AFTER_STABLE_TICKS = 4;

export function overlayRectsEqual(
  a: OverlayRect | null | undefined,
  b: OverlayRect | null | undefined,
): boolean {
  if (a == null && b == null) return true;
  if (a == null || b == null) return false;
  return (
    a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height
  );
}

export function hudPlaceUnchanged(
  previous: ChannelPointsHudPlace | null,
  next: ChannelPointsHudPlace | null,
): boolean {
  if (previous == null && next == null) return true;
  if (previous == null || next == null) return false;
  return (
    Boolean(previous.hidden) === Boolean(next.hidden) &&
    overlayRectsEqual(previous.player, next.player) &&
    overlayRectsEqual(previous.captionAvoid, next.captionAvoid)
  );
}

export type HudGeometryCommit = {
  hidden: boolean;
  player: OverlayRect | null;
  captionAvoid: OverlayRect | null;
  scale: number;
};

export type HudGeometryPollerDeps = {
  place: () => Promise<ChannelPointsHudPlace | null>;
  scale: () => Promise<number>;
  schedule: (fn: () => void, delay: number) => number;
  cancel: (id: number) => void;
  onCommit: (next: HudGeometryCommit) => void;
  shouldSample?: () => boolean;
};

function commitUnchanged(
  previous: HudGeometryCommit | null,
  next: HudGeometryCommit,
): boolean {
  if (!previous) return false;
  return (
    previous.hidden === next.hidden &&
    previous.scale === next.scale &&
    overlayRectsEqual(previous.player, next.player) &&
    overlayRectsEqual(previous.captionAvoid, next.captionAvoid)
  );
}

export function createHudGeometryPoller(deps: HudGeometryPollerDeps) {
  let active = true;
  let timer = 0;
  let inflight = false;
  let dirty = false;
  let epoch = 0;
  let stableTicks = 0;
  let interval = HUD_GEOMETRY_FAST_MS;
  let lastPlace: ChannelPointsHudPlace | null = null;
  let lastCommit: HudGeometryCommit | null = null;
  let lastScale = 1;
  let scaleKnown = false;

  function clearTimer() {
    if (!timer) return;
    deps.cancel(timer);
    timer = 0;
  }

  function scheduleNext() {
    if (!active) return;
    clearTimer();
    timer = deps.schedule(() => {
      void sample();
    }, interval);
  }

  function commit(next: HudGeometryCommit) {
    if (commitUnchanged(lastCommit, next)) return;
    lastCommit = next;
    deps.onCommit(next);
  }

  async function sample() {
    if (!active) return;
    if (inflight) {
      dirty = true;
      return;
    }
    inflight = true;
    const captured = epoch;
    try {
      do {
        dirty = false;
        if (deps.shouldSample && !deps.shouldSample()) {
          interval = HUD_GEOMETRY_FAST_MS;
          break;
        }
        const place = await deps.place();
        if (!active || captured !== epoch) break;

        if (!scaleKnown) {
          lastScale = await deps.scale();
          if (!active || captured !== epoch) break;
          scaleKnown = true;
        }

        if (!place) {
          stableTicks = 0;
          interval = HUD_GEOMETRY_FAST_MS;
          continue;
        }

        const hidden = Boolean(place.hidden);
        const next: HudGeometryCommit = {
          hidden,
          player: hidden ? lastCommit?.player ?? place.player : place.player,
          captionAvoid: hidden
            ? lastCommit?.captionAvoid ?? place.captionAvoid
            : place.captionAvoid,
          scale: lastScale,
        };
        const moved = !hudPlaceUnchanged(lastPlace, place);
        lastPlace = place;
        commit(next);
        if (moved) {
          stableTicks = 0;
          interval = HUD_GEOMETRY_FAST_MS;
        } else {
          stableTicks += 1;
          if (stableTicks >= HUD_GEOMETRY_IDLE_AFTER_STABLE_TICKS) {
            interval = HUD_GEOMETRY_IDLE_MS;
          }
        }
      } while (dirty && active && captured === epoch);
    } finally {
      inflight = false;
      if (!active) {
        clearTimer();
        return;
      }
      if (dirty || captured !== epoch) {
        void sample();
        return;
      }
      scheduleNext();
    }
  }

  return {
    start() {
      void sample();
    },
    nudge() {
      epoch += 1;
      stableTicks = 0;
      interval = HUD_GEOMETRY_FAST_MS;
      dirty = true;
      if (!inflight) void sample();
    },
    invalidateScale() {
      scaleKnown = false;
      dirty = true;
      interval = HUD_GEOMETRY_FAST_MS;
      if (!inflight) void sample();
    },
    setScale(next: number) {
      if (!active) return;
      scaleKnown = true;
      if (lastScale === next) return;
      lastScale = next;
      if (!lastCommit) return;
      commit({ ...lastCommit, scale: next });
    },
    dispose() {
      active = false;
      epoch += 1;
      dirty = false;
      clearTimer();
    },
  };
}
