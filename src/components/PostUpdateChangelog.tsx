import { useEffect, useState } from "react";
import { useSettingsStore } from "../lib/settings/store";
import { shouldShowChangelogAfterUpdate } from "../lib/updater/changelogPrompt";
import { normalizeAppVersion } from "../lib/updater/prompt";
import { isTauri } from "../lib/tauri";
import { ChangelogDialog } from "./ChangelogDialog";

/** Show the released version's notes once after the app updates. */
export function PostUpdateChangelog() {
  const hydrated = useSettingsStore((state) => state.hydrated);
  const onboardingDone = useSettingsStore(
    (state) => state.settings.gui.onboardingDone,
  );
  const lastSeenVersion = useSettingsStore(
    (state) => state.settings.gui.lastSeenChangelogVersion,
  );
  const setSettings = useSettingsStore((state) => state.setSettings);
  const settings = useSettingsStore((state) => state.settings);
  const [currentVersion, setCurrentVersion] = useState<string | null>(null);
  const [show, setShow] = useState(false);

  useEffect(() => {
    if (!hydrated || !isTauri()) return;
    let active = true;
    void (async () => {
      try {
        const { getVersion } = await import("@tauri-apps/api/app");
        const version = await getVersion();
        if (!active) return;
        const current = normalizeAppVersion(version);
        setCurrentVersion(current || null);
        setShow(
          shouldShowChangelogAfterUpdate({
            currentVersion: current,
            lastSeenVersion,
            onboardingDone,
          }),
        );
      } catch {
        // Version lookup must never disrupt the app shell.
      }
    })();
    return () => {
      active = false;
    };
  }, [hydrated, lastSeenVersion, onboardingDone]);

  if (!show || !currentVersion) return null;
  return (
    <ChangelogDialog
      limit={1}
      onClose={() => {
        setSettings({
          gui: {
            ...settings.gui,
            lastSeenChangelogVersion: currentVersion,
          },
        });
        setShow(false);
      }}
    />
  );
}
