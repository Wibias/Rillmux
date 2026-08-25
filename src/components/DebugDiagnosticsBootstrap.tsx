import { useEffect } from "react";
import { shouldAttachDebugConsole } from "../lib/settings/debugConsole";
import { useSettingsStore } from "../lib/settings/store";
import { invoke, isTauri } from "../lib/tauri";

/** Keep the native diagnostics category filter aligned with persisted GUI settings. */
export function DebugDiagnosticsBootstrap({
  children,
}: {
  children: React.ReactNode;
}) {
  const hydrated = useSettingsStore((state) => state.hydrated);
  const categories = useSettingsStore(
    (state) => state.settings.gui.debugCategories,
  );

  useEffect(() => {
    if (!hydrated || !isTauri() || !shouldAttachDebugConsole()) return;
    void invoke("diagnostics_set_debug_categories", { categories }).catch(
      () => undefined,
    );
  }, [categories, hydrated]);

  return children;
}
