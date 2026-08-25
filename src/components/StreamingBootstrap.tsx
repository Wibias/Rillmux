import { useEffect, type ReactNode } from "react";
import { useSettingsStore } from "../lib/settings/store";
import {
  bindStreamingListeners,
  syncEventSub,
  syncViewerPresence,
} from "../lib/streaming/store";

/** Bind Streamlink status / session events for the app lifetime. */
export function StreamingBootstrap({ children }: { children: ReactNode }) {
  const settingsHydrated = useSettingsStore((state) => state.hydrated);
  const channelPointsEnabled = useSettingsStore(
    (state) => state.settings.streaming.channelPoints,
  );
  const followRaids = useSettingsStore(
    (state) => state.settings.streaming.followRaids,
  );

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void bindStreamingListeners().then((unlisten) => {
      cleanup = unlisten;
    });
    return () => cleanup?.();
  }, []);

  useEffect(() => {
    if (!settingsHydrated) return;
    // Persisted settings load after the first render. Reconcile again once
    // hydration completes so Rust cannot stay on the default `false` value.
    syncViewerPresence();
  }, [settingsHydrated, channelPointsEnabled]);

  useEffect(() => {
    if (!settingsHydrated) return;
    // The initial listener bind can run before persisted settings hydrate.
    // Reconcile outgoing-raid EventSub after hydration and on runtime toggles.
    syncEventSub();
  }, [settingsHydrated, followRaids]);

  return children;
}
