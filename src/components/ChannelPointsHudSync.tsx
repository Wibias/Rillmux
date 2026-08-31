import { useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getTwitchWebsiteAuthStatus } from "../lib/auth/website";
import { useSettingsStore } from "../lib/settings/store";
import {
  POINTS_HUD_CHIP_HEIGHT,
  POINTS_HUD_CHIP_MIN_WIDTH,
  chipRectForPlayer,
  hudSyncRunningKey,
  pointsHudLabel,
  pointsHudOverlayUrl,
  type ChannelPointsHudPlace,
  type HudOffset,
  type OverlayRect,
} from "../lib/streaming/pointsHud";
import { useWatchingStore } from "../lib/streaming/store";
import { invoke, isTauri } from "../lib/tauri";

const MAX_HUD_WINDOWS = 8;
const PLAYER_MISS_GRACE_MS = 8_000;

async function closeHud(label: string) {
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const overlay = await WebviewWindow.getByLabel(label);
  await overlay?.close().catch(() => undefined);
}

async function placeHud(channel: string, rect: OverlayRect, force: boolean) {
  await invoke("points_hud_place_window", {
    channelLogin: channel,
    x: Math.round(rect.x),
    y: Math.round(rect.y),
    width: Math.max(1, Math.round(rect.width)),
    height: Math.max(1, Math.round(rect.height)),
    force,
  }).catch(() => undefined);
}

async function ensureHud(
  channel: string,
  rect: OverlayRect,
  showLogin: boolean,
  offset: HudOffset,
  isActive: () => boolean,
) {
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  if (!isActive()) return false;
  const label = pointsHudLabel(channel);
  const existing = await WebviewWindow.getByLabel(label);
  if (!isActive()) return false;
  if (existing) {
    return true;
  }
  const scale = await getCurrentWindow().scaleFactor().catch(() => 1);
  if (!isActive()) return false;
  new WebviewWindow(label, {
    url: `${pointsHudOverlayUrl(channel, offset)}${showLogin ? "&showLogin=1" : ""}`,
    title: "Channel Points",
    decorations: false,
    transparent: true,
    shadow: false,
    skipTaskbar: true,
    alwaysOnTop: false,
    resizable: false,
    focus: false,
    minWidth: 1,
    minHeight: 1,
    x: Math.round(rect.x / scale),
    y: Math.round(rect.y / scale),
    width: Math.round(rect.width / scale),
    height: Math.round(rect.height / scale),
  });
  await placeHud(channel, rect, true);
  if (!isActive()) {
    await closeHud(label);
    return false;
  }
  return true;
}

export function ChannelPointsHudSync() {
  const hudEnabled = useSettingsStore(
    (state) =>
      state.settings.streaming.channelPoints &&
      state.settings.streaming.channelPointsHud,
  );
  const runningKey = useWatchingStore((state) =>
    hudSyncRunningKey(state.sessions),
  );
  const wantedRef = useRef<string[]>([]);
  const missingSinceRef = useRef<Record<string, number>>({});

  // This cleanup is intentionally separate from the synchronization effect.
  // `runningKey` changes whenever a stream is added/removed; closing every HUD
  // in that effect's cleanup made the normal 1 -> 2 stream transition tear down
  // a healthy HUD before the next synchronization pass could preserve it.
  useEffect(() => {
    return () => {
      for (const channel of wantedRef.current) {
        void closeHud(pointsHudLabel(channel));
      }
    };
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let active = true;

    const closeWantedHuds = async () => {
      const channels = wantedRef.current;
      wantedRef.current = [];
      missingSinceRef.current = {};
      await Promise.all(
        channels.map((channel) => closeHud(pointsHudLabel(channel))),
      );
    };

    const sync = async () => {
      if (!active) return;
      if (!hudEnabled) {
        await closeWantedHuds();
        return;
      }
      const website = await getTwitchWebsiteAuthStatus().catch(() => undefined);
      if (!active) return;
      // A transient status lookup failure is not proof that auth disappeared.
      // Preserve already-running HUDs and retry on the next synchronization tick.
      if (website === undefined) return;
      if (!website.configured) {
        await closeWantedHuds();
        return;
      }
      const wanted = [
        ...new Set(runningKey.split("|").filter(Boolean)),
      ].slice(0, MAX_HUD_WINDOWS);
      const wantedSet = new Set(wanted);
      const openSet = new Set(wantedRef.current);
      const showLogin = wanted.length > 1;
      await Promise.all(
        wantedRef.current.flatMap((channel) => {
          if (wantedSet.has(channel)) return [];
          return [
            closeHud(pointsHudLabel(channel)).then(() => {
              delete missingSinceRef.current[channel];
            }),
          ];
        }),
      );
      if (!active) return;
      const kept: string[] = [];
      for (const channel of wanted) {
        const place = await invoke<ChannelPointsHudPlace | null>(
          "channel_points_hud_place",
          { channelLogin: channel },
        ).catch(() => null);
        if (!active) return;
        if (!place?.player) {
          const existingHud = openSet.has(channel);
          if (!existingHud) continue;

          const now = Date.now();
          const missingSince = missingSinceRef.current[channel] ?? now;
          missingSinceRef.current[channel] = missingSince;
          if (now - missingSince < PLAYER_MISS_GRACE_MS) {
            kept.push(channel);
            continue;
          }

          // A running session can briefly lose its discoverable HWND while the
          // native layout is rebuilding. Only retire an existing HUD after a
          // bounded grace period; normal stream stops are handled immediately by
          // `runningKey` removing the channel above.
          await closeHud(pointsHudLabel(channel));
          if (!active) return;
          delete missingSinceRef.current[channel];
          continue;
        }
        delete missingSinceRef.current[channel];
        const offset =
          useSettingsStore.getState().settings.streaming.channelPointsHudOffset;
        const chip = chipRectForPlayer(
          place.player,
          offset,
          POINTS_HUD_CHIP_MIN_WIDTH,
          place.captionAvoid,
        );
        const hudRect = { ...chip, height: POINTS_HUD_CHIP_HEIGHT };
        const hudReady = await ensureHud(
          channel,
          hudRect,
          showLogin,
          offset,
          () => active,
        );
        if (!hudReady || !active) return;
        kept.push(channel);
      }
      wantedRef.current = kept;
    };

    void sync();
    const timer = window.setInterval(() => void sync(), 1000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [hudEnabled, runningKey]);

  return null;
}
