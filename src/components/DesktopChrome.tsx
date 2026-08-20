import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { invoke, isTauri } from "../lib/tauri";
import { useFollowedLiveStreams } from "../lib/browse/useFollowedLive";
import { useAuthStore } from "../lib/auth/store";
import { useSettingsStore } from "../lib/settings/store";
import {
  liveFollowedLogins,
  newlyLiveFollowedLogins,
  shouldNotifyFollowedLive,
} from "../lib/notifications/followedLive";

/**
 * Desktop-only chrome: tray icon, close-to-tray, followed-live notifications.
 */
export function DesktopChrome() {
  const { t } = useTranslation(["common", "routes"]);
  const closeToTray = useSettingsStore((s) => s.settings.gui.closeToTray);
  const notifyFollowed = useSettingsStore(
    (s) => s.settings.notifications.followedOnline,
  );
  const mutedFollowed = useSettingsStore(
    (s) => s.settings.notifications.mutedFollowed,
  );
  const hydrated = useSettingsStore((s) => s.hydrated);
  const userId = useAuthStore((s) => s.session?.userId);
  const { streams, loggedIn } = useFollowedLiveStreams();
  const knownLive = useRef<Set<string>>(new Set());
  const primed = useRef(false);
  const closeToTrayRef = useRef(closeToTray);
  closeToTrayRef.current = closeToTray;

  useEffect(() => {
    if (!isTauri() || !hydrated) return;
    let unlistenClose: (() => void) | undefined;
    let disposed = false;

    void (async () => {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const { TrayIcon } = await import("@tauri-apps/api/tray");
      const { Menu } = await import("@tauri-apps/api/menu");
      const { defaultWindowIcon } = await import("@tauri-apps/api/app");

      if (disposed) return;

      const win = getCurrentWindow();
      const showWindow = async () => {
        await win.show();
        await win.unminimize();
        await win.setFocus();
      };

      const menu = await Menu.new({
        items: [
          {
            id: "show",
            text: t("common:appNameShort"),
            action: () => {
              void showWindow();
            },
          },
          {
            id: "quit",
            text: t("common:quit"),
            action: () => {
              void invoke("app_quit");
            },
          },
        ],
      });

      const icon = await defaultWindowIcon();
      try {
        await TrayIcon.new({
          id: "main-tray",
          icon: icon ?? undefined,
          tooltip: t("common:appName"),
          menu,
          menuOnLeftClick: false,
          action: (event) => {
            if (
              event.type === "Click" &&
              event.button === "Left" &&
              event.buttonState === "Up"
            ) {
              void showWindow();
            }
          },
        });
      } catch {
        // Tray may already exist after HMR; ignore.
      }

      unlistenClose = await win.onCloseRequested(async (event) => {
        if (closeToTrayRef.current) {
          event.preventDefault();
          await win.hide();
        }
      });
    })();

    return () => {
      disposed = true;
      unlistenClose?.();
    };
  }, [hydrated, t]);

  useEffect(() => {
    primed.current = false;
    knownLive.current = new Set();
  }, [userId]);

  useEffect(() => {
    if (!notifyFollowed || !loggedIn) return;
    const next = liveFollowedLogins(streams);

    if (!primed.current) {
      knownLive.current = next;
      primed.current = true;
      return;
    }

    const newlyLive = newlyLiveFollowedLogins(knownLive.current, next).filter(
      (login) =>
        shouldNotifyFollowedLive(login, {
          followedOnline: notifyFollowed,
          mutedFollowed,
        }),
    );
    knownLive.current = next;

    if (!newlyLive.length || !isTauri()) return;

    void (async () => {
      const {
        isPermissionGranted,
        requestPermission,
        sendNotification,
      } = await import("@tauri-apps/plugin-notification");

      let granted = await isPermissionGranted();
      if (!granted) {
        granted = (await requestPermission()) === "granted";
      }
      if (!granted) return;

      for (const login of newlyLive.slice(0, 3)) {
        const stream = streams.find(
          (s) => s.user_login.toLowerCase() === login,
        );
        sendNotification({
          title: t("routes:notifyLiveTitle", {
            channel: stream?.user_name ?? login,
          }),
          body: stream?.title ?? t("routes:notifyLiveBody"),
        });
      }
    })();
  }, [loggedIn, mutedFollowed, notifyFollowed, streams, t]);

  return null;
}
