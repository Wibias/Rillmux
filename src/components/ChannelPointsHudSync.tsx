import { useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getTwitchWebsiteAuthStatus, TWITCH_WEB_AUTH_CHANGED_EVENT } from "../lib/auth/website";
import { useSettingsStore } from "../lib/settings/store";
import {
  hudSyncRunningKey,
  pointsHudLabel,
  pointsHudOverlayUrl,
  type ChannelPointsHudPlace,
  type HudOffset,
  type OverlayRect,
} from "../lib/streaming/pointsHud";
import { runChannelPointsHudSyncPass } from "../lib/streaming/hudSyncPass";
import { useWatchingStore } from "../lib/streaming/store";
import { invoke, isTauri } from "../lib/tauri";
import { listenWhileMounted } from "../lib/tauri/ownAsyncSubscription";
import { createSerializedKick } from "../lib/tauri/serializedKick";

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
    const kick = createSerializedKick(async (isCurrent) => {
      const result = await runChannelPointsHudSyncPass({
        isCurrent,
        hudEnabled,
        runningKey,
        wanted: wantedRef.current,
        missingSince: missingSinceRef.current,
        now: () => Date.now(),
        getWebsiteStatus: () =>
          getTwitchWebsiteAuthStatus().catch(() => undefined),
        place: (channel) =>
          invoke<ChannelPointsHudPlace | null>("channel_points_hud_place", {
            channelLogin: channel,
          }).catch(() => null),
        ensureHud: (channel, rect, showLogin, offset) =>
          ensureHud(channel, rect, showLogin, offset, isCurrent),
        closeHud: (channel) => closeHud(pointsHudLabel(channel)),
        getOffset: () =>
          useSettingsStore.getState().settings.streaming.channelPointsHudOffset,
      });
      if (!isCurrent()) return;
      wantedRef.current = result.wanted;
      missingSinceRef.current = result.missingSince;
    });
    kick.kick();
    const timer = window.setInterval(() => kick.kick(), 1000);
    const stopAuth = listenWhileMounted(TWITCH_WEB_AUTH_CHANGED_EVENT, () => {
      kick.invalidate();
      kick.kick();
    });
    return () => {
      kick.dispose();
      window.clearInterval(timer);
      stopAuth();
    };
  }, [hudEnabled, runningKey]);

  return null;
}
