import { useEffect, useRef, useState } from "react";
import { isTauri } from "../lib/tauri";
import {
  readSkippedUpdateVersion,
  shouldPromptAppUpdate,
  writeSkippedUpdateVersion,
} from "../lib/updater/prompt";
import { UpdateDialog } from "./UpdateDialog";

export interface UpdateHandle {
  currentVersion?: string;
  version: string;
  body?: string;
  downloadAndInstall: (
    cb?: (event: {
      event: "Started" | "Progress" | "Finished";
      data: { contentLength?: number; chunkLength: number };
    }) => void,
  ) => Promise<void>;
}

/**
 * Check shortly after app start and once per hour while the app stays open.
 * Dismissing a version keeps it quiet across restarts; a newer release can
 * still appear.
 */
export function UpdateBanner() {
  const [update, setUpdate] = useState<UpdateHandle | null>(null);
  const dismissedVersionRef = useRef<string | null>(null);
  const checkingRef = useRef(false);

  useEffect(() => {
    if (!isTauri()) return;
    if (!shouldPromptAppUpdate({ viteDev: import.meta.env.DEV })) return;

    const skippedStorage = () =>
      typeof localStorage === "undefined" ? null : localStorage;

    const runCheck = async () => {
      if (checkingRef.current) return;
      checkingRef.current = true;
      try {
        const { check } = await import("@tauri-apps/plugin-updater");
        const next = (await check()) as UpdateHandle | null;
        const skipped =
          dismissedVersionRef.current ??
          readSkippedUpdateVersion(skippedStorage());
        if (
          next &&
          shouldPromptAppUpdate({
            viteDev: import.meta.env.DEV,
            currentVersion: next.currentVersion,
            availableVersion: next.version,
            skippedVersion: skipped,
          })
        ) {
          setUpdate(next);
        }
      } catch {
        // Offline / endpoint failures stay silent and the next interval retries.
      } finally {
        checkingRef.current = false;
      }
    };

    // Slight delay so the first check does not compete with auth/settings boot.
    const startupTimer = window.setTimeout(() => void runCheck(), 4000);
    const refreshTimer = window.setInterval(
      () => void runCheck(),
      60 * 60 * 1000,
    );
    return () => {
      window.clearTimeout(startupTimer);
      window.clearInterval(refreshTimer);
    };
  }, []);

  if (!update) return null;

  return (
    <UpdateDialog
      update={update}
      onCancel={() => {
        dismissedVersionRef.current = update.version;
        writeSkippedUpdateVersion(
          typeof localStorage === "undefined" ? null : localStorage,
          update.version,
        );
        setUpdate(null);
      }}
    />
  );
}
