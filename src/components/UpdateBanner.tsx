import { useEffect, useState } from "react";
import { isTauri } from "../lib/tauri";
import { UpdateDialog } from "./UpdateDialog";

export interface UpdateHandle {
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
 * One-shot update check shortly after app start. When an update is
 * available, shows a modal window with the release notes (changelog) and
 * Install now / Cancel. Cancel (or clicking outside the dialog) dismisses it
 * until the next app start; install downloads, opens the installer (NSIS
 * basicUi) and relaunches into the new version.
 */
export function UpdateBanner() {
  const [update, setUpdate] = useState<UpdateHandle | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    // Slight delay so the check does not compete with boot work (auth,
    // settings, first paint). Failures (offline, endpoint down) stay silent.
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const { check } = await import("@tauri-apps/plugin-updater");
          const update = (await check()) as UpdateHandle | null;
          if (update) {
            setUpdate(update);
          }
        } catch {
          // stay closed
        }
      })();
    }, 4000);
    return () => window.clearTimeout(timer);
  }, []);

  if (!update) return null;

  return (
    <UpdateDialog
      update={update}
      onCancel={() => setUpdate(null)}
    />
  );
}
