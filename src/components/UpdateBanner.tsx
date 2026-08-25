import { useEffect, useRef, useState } from "react";
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
 * Check shortly after app start and once per hour while the app stays open.
 * Dismissing one version keeps that version quiet until the next app start;
 * a newer release discovered by a later hourly check can still be shown.
 */
export function UpdateBanner() {
  const [update, setUpdate] = useState<UpdateHandle | null>(null);
  const dismissedVersionRef = useRef<string | null>(null);
  const checkingRef = useRef(false);

  useEffect(() => {
    if (!isTauri()) return;

    const runCheck = async () => {
      if (checkingRef.current) return;
      checkingRef.current = true;
      try {
        const { check } = await import("@tauri-apps/plugin-updater");
        const next = (await check()) as UpdateHandle | null;
        if (next && next.version !== dismissedVersionRef.current) {
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
        setUpdate(null);
      }}
    />
  );
}
