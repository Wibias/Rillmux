import {
  POINTS_HUD_CHIP_HEIGHT,
  POINTS_HUD_CHIP_MIN_WIDTH,
  chipRectForPlayer,
  hudKeepOnPlayerMiss,
  type ChannelPointsHudPlace,
  type HudOffset,
  type OverlayRect,
} from "./pointsHud";

export const MAX_HUD_WINDOWS = 8;

export type HudSyncWebsiteStatus = {
  configured: boolean;
};

export type HudSyncPassDeps = {
  isCurrent: () => boolean;
  hudEnabled: boolean;
  runningKey: string;
  wanted: string[];
  missingSince: Record<string, number>;
  now: () => number;
  getWebsiteStatus: () => Promise<HudSyncWebsiteStatus | undefined>;
  place: (channel: string) => Promise<ChannelPointsHudPlace | null>;
  ensureHud: (
    channel: string,
    rect: OverlayRect,
    showLogin: boolean,
    offset: HudOffset,
  ) => Promise<boolean>;
  closeHud: (channel: string) => Promise<void>;
  getOffset: () => HudOffset;
};

export type HudSyncPassResult = {
  wanted: string[];
  missingSince: Record<string, number>;
};

export async function runChannelPointsHudSyncPass(
  deps: HudSyncPassDeps,
): Promise<HudSyncPassResult> {
  const missingSince = { ...deps.missingSince };
  let wantedOpen = [...deps.wanted];

  const closeAll = async () => {
    const channels = wantedOpen;
    wantedOpen = [];
    await Promise.all(channels.map((channel) => deps.closeHud(channel)));
    return { wanted: [] as string[], missingSince: {} as Record<string, number> };
  };

  if (!deps.isCurrent()) {
    return { wanted: wantedOpen, missingSince };
  }
  if (!deps.hudEnabled) {
    return closeAll();
  }

  const website = await deps.getWebsiteStatus();
  if (!deps.isCurrent()) {
    return { wanted: wantedOpen, missingSince };
  }
  if (website === undefined) {
    return { wanted: wantedOpen, missingSince };
  }
  if (!website.configured) {
    return closeAll();
  }

  const wanted = [
    ...new Set(deps.runningKey.split("|").filter(Boolean)),
  ].slice(0, MAX_HUD_WINDOWS);
  const wantedSet = new Set(wanted);
  const openSet = new Set(wantedOpen);
  const showLogin = wanted.length > 1;
  await Promise.all(
    wantedOpen.flatMap((channel) => {
      if (wantedSet.has(channel)) return [];
      return [
        deps.closeHud(channel).then(() => {
          delete missingSince[channel];
        }),
      ];
    }),
  );
  if (!deps.isCurrent()) {
    return { wanted: wanted.filter((channel) => openSet.has(channel)), missingSince };
  }

  const kept: string[] = [];
  for (const channel of wanted) {
    const nextPlace = await deps.place(channel);
    if (!deps.isCurrent()) {
      return { wanted: kept, missingSince };
    }
    if (nextPlace?.hidden) {
      delete missingSince[channel];
      await deps.closeHud(channel);
      if (!deps.isCurrent()) {
        return { wanted: kept, missingSince };
      }
      continue;
    }
    if (!nextPlace?.player) {
      if (!openSet.has(channel)) continue;
      const now = deps.now();
      const missingAt = missingSince[channel] ?? now;
      missingSince[channel] = missingAt;
      if (hudKeepOnPlayerMiss("missing", now - missingAt)) {
        kept.push(channel);
        continue;
      }
      await deps.closeHud(channel);
      if (!deps.isCurrent()) {
        return { wanted: kept, missingSince };
      }
      delete missingSince[channel];
      continue;
    }
    delete missingSince[channel];
    const offset = deps.getOffset();
    const chip = chipRectForPlayer(
      nextPlace.player,
      offset,
      POINTS_HUD_CHIP_MIN_WIDTH,
      nextPlace.captionAvoid,
    );
    const hudRect = { ...chip, height: POINTS_HUD_CHIP_HEIGHT };
    const hudReady = await deps.ensureHud(channel, hudRect, showLogin, offset);
    if (!hudReady || !deps.isCurrent()) {
      return { wanted: kept, missingSince };
    }
    kept.push(channel);
  }
  return { wanted: kept, missingSince };
}
