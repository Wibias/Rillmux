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
import { MAIN_TRAY_ID, shouldCreateDesktopTray } from "../lib/desktop/tray";
import { createDesktopTraySession } from "../lib/desktop/trayBootstrap";

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
  useEffect(() => {
    closeToTrayRef.current = closeToTray;
  }, [closeToTray]);

  useEffect(() => {
    if (!isTauri() || !hydrated) return;
    const useTray = shouldCreateDesktopTray(import.meta.env.DEV);
    const session = createDesktopTraySession({
      shouldCreateTray: useTray,
      closeToTray: () => closeToTrayRef.current,
      hideWindow: async () => {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().hide();
      },
      async loadApis() {
        const [
          { getCurrentWindow },
          { TrayIcon },
          { Menu },
          { defaultWindowIcon },
        ] = await Promise.all([
          import("@tauri-apps/api/window"),
          import("@tauri-apps/api/tray"),
          import("@tauri-apps/api/menu"),
          import("@tauri-apps/api/app"),
        ]);
        const win = getCurrentWindow();
        const showWindow = async () => {
          await win.show();
          await win.unminimize();
          await win.setFocus();
        };
        let menu: Awaited<ReturnType<typeof Menu.new>> | undefined;
        return {
          async closeLeftover() {
            const leftover = await TrayIcon.getById(MAIN_TRAY_ID).catch(
              () => null,
            );
            await leftover?.close().catch(() => undefined);
          },
          async createMenu() {
            menu = await Menu.new({
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
          },
          async createTray() {
            const icon = await defaultWindowIcon();
            await TrayIcon.new({
              id: MAIN_TRAY_ID,
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
            }).catch(() => undefined);
          },
          async closeTray() {
            const tray = await TrayIcon.getById(MAIN_TRAY_ID).catch(() => null);
            await tray?.close().catch(() => undefined);
          },
          onCloseRequested: (handler) => win.onCloseRequested(handler),
        };
      },
    });
    return session.start();
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
