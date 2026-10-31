import { listen } from "@tauri-apps/api/event";
import { invoke, isTauri } from "../../tauri";
import { createOwnedListenerSet } from "../../tauri/ownedListenerSet";
import type { HelixStream } from "../../twitch/helix";
import { useSettingsStore } from "../../settings/store";
import { debugRuntimeEvent } from "../../diagnostics/runtimeDebug";
import { useWatchingStore } from "../store";
import type { StreamStatusEvent } from "./types";
import { syncViewerPresence } from "./presenceSync";
import { runningChannels } from "./layoutChannels";
import { afterSessionsChanged } from "./layoutSchedule";

export function syncEventSub() {
  if (!isTauri()) return;
  const settings = useSettingsStore.getState().settings;
  const channels = runningChannels();
  debugRuntimeEvent("raids", "eventsub.sync.frontend", {
    enabled: settings.streaming.followRaids,
    channels: channels.join(","),
  });
  void invoke("eventsub_sync", {
    enabled: settings.streaming.followRaids,
    channels,
  }).catch((reason) =>
    debugRuntimeEvent("raids", "eventsub.sync.frontend.result", {
      ok: false,
      reason: reason instanceof Error ? reason.message : String(reason),
    }),
  );
}

export function stubHelixStream(opts: {
  login: string;
  userId: string;
  displayName?: string;
  title?: string;
  gameName?: string;
}): HelixStream {
  return {
    id: "",
    user_id: opts.userId,
    user_login: opts.login,
    user_name: opts.displayName ?? opts.login,
    game_id: "",
    game_name: opts.gameName ?? "",
    type: "live",
    title: opts.title ?? "",
    viewer_count: 0,
    started_at: new Date().toISOString(),
    language: "",
    thumbnail_url: "",
    is_mature: false,
  };
}

export const streamingListeners = createOwnedListenerSet();

export async function bindStreamingListeners(): Promise<() => void> {
  if (!isTauri()) {
    return () => undefined;
  }
  const unlisten = await streamingListeners.bind([
    () =>
      listen<StreamStatusEvent>("stream-status", (event) => {
        useWatchingStore.getState().applyStatus(event.payload);
      }),
    () =>
      listen("stream-sessions-changed", () => {
        void useWatchingStore.getState().refresh().then(() => {
          afterSessionsChanged();
        });
      }),
    () =>
      listen<number>("dock-chat-fraction", (event) => {
        const fraction = event.payload;
        if (typeof fraction !== "number" || Number.isNaN(fraction)) return;
        const settings = useSettingsStore.getState().settings;
        const clamped = Math.min(0.45, Math.max(0.12, fraction));
        if (Math.abs(settings.streaming.chatWidthFraction - clamped) < 0.001) {
          return;
        }
        useSettingsStore.getState().setSettings({
          streaming: {
            ...settings.streaming,
            chatWidthFraction: clamped,
          },
        });
      }),
  ]);
  syncEventSub();
  syncViewerPresence();
  return unlisten;
}

