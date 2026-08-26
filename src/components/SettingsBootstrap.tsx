import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useSettingsStore } from "../lib/settings/store";
import {
  loadPersistedSettings,
  persistSettings,
} from "../lib/settings/persist";
import { invoke, isTauri } from "../lib/tauri";
import {
  isOverlayWebview,
  shouldAttachDebugConsole,
} from "../lib/settings/debugConsole";
import {
  POINTS_HUD_OFFSET_EVENT,
  hudOffsetFromSearch,
  hudOffsetFromUnknown,
  hudOffsetsEqual,
  type HudOffset,
} from "../lib/streaming/pointsHud";

let pendingHudOffset: { received: true; value: HudOffset } | { received: false } =
  { received: false };

function applyHudOffset(next: HudOffset) {
  const store = useSettingsStore.getState();
  const current = store.settings.streaming.channelPointsHudOffset;
  if (hudOffsetsEqual(current, next)) return;
  store.setSettings({
    streaming: {
      ...store.settings.streaming,
      channelPointsHudOffset: next,
    },
  });
}

/** Hydrates and persists settings without loading the settings page itself. */
export function SettingsBootstrap({ children }: { children: React.ReactNode }) {
  const hydrate = useSettingsStore((s) => s.hydrate);
  const hydrated = useSettingsStore((s) => s.hydrated);
  const settings = useSettingsStore((s) => s.settings);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    // Listen before hydrate so a reset event cannot lose to a stale settings.json.
    if (isTauri()) {
      void listen(POINTS_HUD_OFFSET_EVENT, (event) => {
        const next = hudOffsetFromUnknown(event.payload);
        pendingHudOffset = { received: true, value: next };
        if (cancelled) return;
        if (useSettingsStore.getState().hydrated) {
          applyHudOffset(next);
        }
      }).then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unlisten = fn;
      });
    }

    void loadPersistedSettings().then((loaded) => {
      if (cancelled) return;
      hydrate(loaded);
      const fromUrl = hudOffsetFromSearch();
      if (fromUrl.found) applyHudOffset(fromUrl.offset);
      if (pendingHudOffset.received) applyHudOffset(pendingHudOffset.value);
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [hydrate]);

  useEffect(() => {
    if (!hydrated || !isTauri()) return;
    if (!shouldAttachDebugConsole()) return;
    void invoke("diagnostics_set_debug", {
      enabled: settings.gui.debugMode,
    }).catch(() => undefined);
  }, [hydrated, settings.gui.debugMode]);

  useEffect(() => {
    if (!hydrated) return;
    if (isOverlayWebview()) return;
    const handle = window.setTimeout(() => {
      void persistSettings(settings);
    }, 400);
    return () => window.clearTimeout(handle);
  }, [settings, hydrated]);

  return children;
}
