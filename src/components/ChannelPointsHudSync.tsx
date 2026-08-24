import { useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getTwitchWebsiteAuthStatus } from "../lib/auth/website";
import { useSettingsStore } from "../lib/settings/store";
import {
  POINTS_HUD_CHIP_HEIGHT,
  POINTS_HUD_CHIP_MIN_WIDTH,
  POINTS_HUD_MOVE_SLOP,
  chipRectForPlayer,
  hudSyncRunningKey,
  pointsHudLabel,
  pointsHudOverlayUrl,
  type ChannelPointsHudPlace,
  type HudOffset,
  type OverlayRect,
} from "../lib/streaming/pointsHud";
import { useWatchingStore } from "../lib/streaming/store";
import { overlayRectMoved } from "../lib/streaming/pollOverlay";
import { invoke, isTauri } from "../lib/tauri";

const MAX_HUD_WINDOWS = 8;
const PLAYER_MISS_GRACE = 3;

async function closeHud(label: string) {
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const overlay = await WebviewWindow.getByLabel(label);
  await overlay?.close().catch(() => undefined);
}

async function placeHud(channel: string, rect: OverlayRect, force: boolean) {
  await invoke("overlay_place_hud", {
    label: pointsHudLabel(channel),
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
  forcePlace: boolean,
) {
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const label = pointsHudLabel(channel);
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    if (forcePlace) {
      await placeHud(channel, rect, true);
    }
    return;
  }
  const scale = await getCurrentWindow().scaleFactor().catch(() => 1);
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
  const missesRef = useRef<Record<string, number>>({});
  const lastPlacedRef = useRef<Record<string, OverlayRect>>({});

  useEffect(() => {
    if (!isTauri()) return;
    let active = true;

    const sync = async () => {
      if (!active) return;
      const website = await getTwitchWebsiteAuthStatus().catch(() => null);
      const enabled = Boolean(hudEnabled && website?.configured);
      if (!enabled) {
        for (const channel of wantedRef.current) {
          await closeHud(pointsHudLabel(channel));
        }
        wantedRef.current = [];
        return;
      }
      const wanted = [
        ...new Set(runningKey.split("|").filter(Boolean)),
      ].slice(0, MAX_HUD_WINDOWS);
      const showLogin = wanted.length > 1;
      for (const channel of wantedRef.current) {
        if (!wanted.includes(channel)) {
          await closeHud(pointsHudLabel(channel));
          delete lastPlacedRef.current[channel];
        }
      }
      const kept: string[] = [];
      for (const channel of wanted) {
        const place = await invoke<ChannelPointsHudPlace | null>(
          "channel_points_hud_place",
          { channelLogin: channel },
        ).catch(() => null);
        if (!place?.player) {
          // A retile/SW_RESTORE can miss the HWND for a frame. Closing the
          // overlay here remounts it against a stale settings.json offset.
          const misses = (missesRef.current[channel] ?? 0) + 1;
          missesRef.current[channel] = misses;
          if (misses >= PLAYER_MISS_GRACE) {
            await closeHud(pointsHudLabel(channel));
            delete lastPlacedRef.current[channel];
          } else if (wantedRef.current.includes(channel)) {
            kept.push(channel);
          }
          continue;
        }
        missesRef.current[channel] = 0;
        const offset =
          useSettingsStore.getState().settings.streaming.channelPointsHudOffset;
        const chip = chipRectForPlayer(
          place.player,
          offset,
          POINTS_HUD_CHIP_MIN_WIDTH,
          place.captionAvoid,
        );
        const hudRect = { ...chip, height: POINTS_HUD_CHIP_HEIGHT };
        const prev = lastPlacedRef.current[channel];
        const moved =
          !prev || overlayRectMoved(prev, hudRect, POINTS_HUD_MOVE_SLOP);
        await ensureHud(channel, hudRect, showLogin, offset, moved);
        lastPlacedRef.current[channel] = hudRect;
        kept.push(channel);
      }
      wantedRef.current = kept;
    };

    void sync();
    const timer = window.setInterval(() => void sync(), 1000);
    return () => {
      active = false;
      window.clearInterval(timer);
      for (const channel of wantedRef.current) {
        void closeHud(pointsHudLabel(channel));
      }
    };
  }, [hudEnabled, runningKey]);

  return null;
}
