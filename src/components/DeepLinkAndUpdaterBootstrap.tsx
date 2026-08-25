import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { isTauri } from "../lib/tauri";
import { useWatchingStore } from "../lib/streaming/store";
import { useSettingsStore } from "../lib/settings/store";
import {
  getChannelStreams,
  type HelixStream,
} from "../lib/twitch/helix";

/** Twitch logins are 1–25 chars: lowercase letters, digits, underscore. */
const TWITCH_LOGIN = /^[a-z0-9_]{1,25}$/;
const DEEP_LINK_ROUTES = new Set(["watch", "channel"]);

/** Parse only the documented `stg://watch/<login>` / `stg://channel/<login>` routes. */
export function parseDeepLinkChannel(url: string): string | null {
  try {
    const parsed = new URL(url);
    if (
      parsed.protocol !== "stg:" ||
      parsed.username ||
      parsed.password ||
      parsed.port
    ) {
      return null;
    }

    const host = parsed.hostname.toLowerCase();
    const segments = parsed.pathname.split("/").filter(Boolean);
    let route: string | undefined;
    let login: string | undefined;

    if (DEEP_LINK_ROUTES.has(host) && segments.length === 1) {
      route = host;
      login = segments[0];
    } else if (
      !host &&
      segments.length === 2 &&
      DEEP_LINK_ROUTES.has(segments[0]!.toLowerCase())
    ) {
      route = segments[0]!.toLowerCase();
      login = segments[1];
    }

    if (!route || !login) {
      return null;
    }
    const channel = login.toLowerCase();
    return TWITCH_LOGIN.test(channel) ? channel : null;
  } catch {
    return null;
  }
}

/** Handle `stg://watch/<login>` and `stg://channel/<login>` deep links. */
export function DeepLinkBootstrap({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate();
  const watchStream = useWatchingStore((s) => s.watchStream);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const { getCurrent, onOpenUrl } = await import(
        "@tauri-apps/plugin-deep-link"
      );

      const handleUrl = async (url: string) => {
        const channel = parseDeepLinkChannel(url);
        if (!channel || disposed) return;

        navigate(`/channel/${channel}`);

        // Auto-starting a stream spawns Streamlink/mpv/Chatterino — only do
        // that when the user explicitly opted in (Settings → GUI).
        if (!useSettingsStore.getState().settings.gui.deepLinkAutoWatch) {
          return;
        }
        try {
          const page = await getChannelStreams(channel);
          if (disposed) return;
          const live = page.data[0] as HelixStream | undefined;
          if (live) {
            await watchStream(live);
            if (!disposed) {
              navigate("/watching");
            }
          }
        } catch {
          // Channel page is enough if auth/network fails.
        }
      };

      const existing = await getCurrent().catch(() => null);
      if (!disposed && existing?.length) {
        for (const u of existing) {
          void handleUrl(u);
        }
      }

      const stopListening = await onOpenUrl((urls) => {
        for (const u of urls) void handleUrl(u);
      });
      if (disposed) {
        stopListening();
      } else {
        unlisten = stopListening;
      }
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [navigate, watchStream]);

  return children;
}

export function useUpdaterCheck() {
  const [status, setStatus] = useState<
    "idle" | "checking" | "available" | "none" | "error"
  >("idle");
  const [version, setVersion] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const check = async () => {
    if (!isTauri()) {
      setStatus("error");
      setError("Desktop app required");
      return;
    }
    setStatus("checking");
    setError(null);
    try {
      const { check: checkUpdate } = await import(
        "@tauri-apps/plugin-updater"
      );
      const update = await checkUpdate();
      if (update) {
        setVersion(update.version);
        setStatus("available");
      } else {
        setStatus("none");
      }
    } catch (err) {
      setStatus("error");
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const install = async () => {
    if (!isTauri()) return;
    setStatus("checking");
    try {
      const { check: checkUpdate } = await import(
        "@tauri-apps/plugin-updater"
      );
      const { relaunch } = await import("@tauri-apps/plugin-process");
      const update = await checkUpdate();
      if (!update) {
        setStatus("none");
        return;
      }
      await update.downloadAndInstall();
      await relaunch();
    } catch (err) {
      setStatus("error");
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return { status, version, error, check, install };
}
