import { useRef, useState } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { useSettingsStore } from "../lib/settings/store";
import {
  exportSettingsJson,
  importSettingsJson,
} from "../lib/settings/persist";
import type {
  ChatProvider,
  HotkeySettings,
  ThemeMode,
} from "../lib/settings/types";
import { eventToHotkey, normalizeHotkey } from "../lib/hotkeys";
import { toggleMutedFollowed } from "../lib/notifications/followedLive";
import { invoke } from "../lib/tauri";

export function SettingsTabPanel({ children }: { children: React.ReactNode }) {
  return (
    <div
      className="settings__group"
      role="tabpanel"
      id="settings-panel"
      aria-labelledby="settings-panel-title"
    >
      {children}
    </div>
  );
}


export function SettingsInterfacePanel() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  return (
    <SettingsTabPanel>
        <div className="settings__row">
          <div className="settings__label">
            <span>{t("settings:theme")}</span>
          </div>
          <div className="settings__control">
            <select
              value={settings.theme}
              aria-label={t("settings:theme")}
              onChange={(e) =>
                setSettings({ theme: e.target.value as ThemeMode })
              }
            >
              <option value="system">{t("settings:themeSystem")}</option>
              <option value="dark">{t("settings:themeDark")}</option>
              <option value="light">{t("settings:themeLight")}</option>
            </select>
          </div>
        </div>

        <label className="settings__row settings__row--check">
          <input
            type="checkbox"
            checked={settings.gui.closeToTray}
            onChange={(e) =>
              setSettings({
                gui: { ...settings.gui, closeToTray: e.target.checked },
              })
            }
          />
          <span className="settings__check-text">
            {t("settings:closeToTray")}
          </span>
        </label>

        <label className="settings__row settings__row--check">
          <input
            type="checkbox"
            checked={settings.gui.minimizeOnWatch}
            onChange={(e) =>
              setSettings({
                gui: { ...settings.gui, minimizeOnWatch: e.target.checked },
              })
            }
          />
          <span className="settings__check-text">
            {t("settings:minimizeOnWatch")}
          </span>
        </label>

        <label className="settings__row settings__row--check">
          <input
            type="checkbox"
            checked={settings.gui.deepLinkAutoWatch}
            onChange={(e) =>
              setSettings({
                gui: { ...settings.gui, deepLinkAutoWatch: e.target.checked },
              })
            }
          />
          <span className="settings__check-text">
            {t("settings:deepLinkAutoWatch")}
          </span>
        </label>

        <div className="settings__row">
          <div className="settings__label">
            <span>{t("settings:showSetupAgain")}</span>
            <small className="muted">{t("settings:showSetupAgainHint")}</small>
          </div>
          <div className="settings__control">
            <button
              type="button"
              className="button-secondary"
              onClick={() =>
                setSettings({
                  gui: { ...settings.gui, onboardingDone: false },
                })
              }
            >
              {t("settings:showSetupAgain")}
            </button>
          </div>
        </div>
    </SettingsTabPanel>
  );
}

export function SettingsChatPanel() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  return (
    <SettingsTabPanel>
        <div className="settings__row">
          <div className="settings__label">
            <span>{t("settings:chatProvider")}</span>
            <small className="muted">{t("settings:chatProviderHint")}</small>
          </div>
          <div className="settings__control">
            <select
              value={settings.chat.provider}
              aria-label={t("settings:chatProvider")}
              onChange={(e) =>
                setSettings({
                  chat: {
                    ...settings.chat,
                    provider: e.target.value as ChatProvider,
                  },
                })
              }
            >
              <option value="embedded">{t("settings:chatEmbedded")}</option>
              <option value="chatterino">{t("settings:chatChatterino")}</option>
              <option value="browser">{t("settings:chatBrowser")}</option>
              <option value="chrome">{t("settings:chatChrome")}</option>
              <option value="custom">{t("settings:chatCustom")}</option>
            </select>
          </div>
        </div>
    </SettingsTabPanel>
  );
}

export function SettingsNotificationsPanel() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const [newMutedLogin, setNewMutedLogin] = useState("");
  return (
      <div
        className="settings__section"
        role="tabpanel"
        id="settings-panel"
        aria-labelledby="settings-panel-title"
      >
      <div className="settings__group">
        <label className="settings__row settings__row--check">
          <input
            type="checkbox"
            checked={settings.notifications.followedOnline}
            onChange={(e) =>
              setSettings({
                notifications: {
                  ...settings.notifications,
                  followedOnline: e.target.checked,
                },
              })
            }
          />
          <span className="settings__check-text">
            {t("settings:followedOnline")}
            <small className="muted">{t("settings:followedOnlineHint")}</small>
          </span>
        </label>
        <p className="muted settings__hint">{t("settings:mutedFollowedHint")}</p>
        <div className="settings__row settings__row--add">
          <input
            className="input"
            value={newMutedLogin}
            placeholder={t("settings:channelLogin")}
            onChange={(e) => setNewMutedLogin(e.target.value)}
            aria-label={t("settings:channelLogin")}
          />
          <button
            type="button"
            className="button-secondary"
            onClick={() => {
              const login = newMutedLogin.trim().toLowerCase();
              if (!login) return;
              setSettings({
                notifications: {
                  ...settings.notifications,
                  mutedFollowed: toggleMutedFollowed(
                    settings.notifications.mutedFollowed,
                    login,
                    false,
                  ),
                },
              });
              setNewMutedLogin("");
            }}
          >
            {t("settings:mutedFollowedAdd")}
          </button>
        </div>
      </div>
        {settings.notifications.mutedFollowed.length === 0 ? (
          <p className="muted settings__empty">{t("settings:mutedFollowedEmpty")}</p>
        ) : (
          <ul className="settings__channel-list">
            {settings.notifications.mutedFollowed.map((login) => (
              <li key={login} className="settings__channel-item">
                <Link className="settings__channel-login" to={`/channel/${login}`}>
                  {login}
                </Link>
                <button
                  type="button"
                  className="button-secondary"
                  onClick={() =>
                    setSettings({
                      notifications: {
                        ...settings.notifications,
                        mutedFollowed: toggleMutedFollowed(
                          settings.notifications.mutedFollowed,
                          login,
                          true,
                        ),
                      },
                    })
                  }
                >
                  {t("settings:mutedFollowedUnmute")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
  );
}

export function SettingsHotkeysPanel() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const captureHotkey =
    (key: keyof HotkeySettings) => (e: React.KeyboardEvent<HTMLInputElement>) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setSettings({
          hotkeys: { ...settings.hotkeys, [key]: "" },
        });
        return;
      }
      if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return;
      setSettings({
        hotkeys: {
          ...settings.hotkeys,
          [key]: normalizeHotkey(eventToHotkey(e.nativeEvent)),
        },
      });
    };
  return (
    <SettingsTabPanel>
        <p className="muted settings__hint">{t("settings:hotkeysHint")}</p>
        {(
          [
            ["refresh", "hotkeyRefresh"],
            ["focusSearch", "hotkeyFocusSearch"],
            ["stopAll", "hotkeyStopAll"],
            ["cycleDockMonitor", "hotkeyCycleDockMonitor"],
            ["openSettings", "hotkeyOpenSettings"],
            ["quit", "hotkeyQuit"],
          ] as const
        ).map(([key, labelKey]) => (
          <div className="settings__row" key={key}>
            <div className="settings__label">
              <span>{t(`settings:${labelKey}`)}</span>
            </div>
            <div className="settings__control">
              <input
                className="input"
                readOnly
                value={settings.hotkeys[key]}
                placeholder="-"
                onKeyDown={captureHotkey(key)}
                aria-label={t(`settings:${labelKey}`)}
              />
            </div>
          </div>
        ))}
    </SettingsTabPanel>
  );
}

export function SettingsChannelsPanel() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((s) => s.settings);
  const setChannelOverride = useSettingsStore((s) => s.setChannelOverride);
  const [newChannelLogin, setNewChannelLogin] = useState("");
  const [newChannelQuality, setNewChannelQuality] = useState("");
  const channelEntries = Object.entries(settings.channels);
  return (
      <div
        className="settings__section"
        role="tabpanel"
        id="settings-panel"
        aria-labelledby="settings-panel-title"
      >
        <div className="settings__group">
          <p className="muted settings__hint">{t("settings:channelsHint")}</p>
          <div className="settings__row settings__row--add">
            <input
              className="input"
              value={newChannelLogin}
              placeholder={t("settings:channelLogin")}
              onChange={(e) => setNewChannelLogin(e.target.value)}
              aria-label={t("settings:channelLogin")}
            />
            <input
              className="input"
              value={newChannelQuality}
              placeholder={t("settings:channelQuality")}
              onChange={(e) => setNewChannelQuality(e.target.value)}
              aria-label={t("settings:channelQuality")}
            />
            <button
              type="button"
              className="button-secondary"
              onClick={() => {
                const login = newChannelLogin.trim().toLowerCase();
                if (!login) return;
                setChannelOverride(login, {
                  quality: newChannelQuality.trim() || undefined,
                });
                setNewChannelLogin("");
                setNewChannelQuality("");
              }}
            >
              {t("settings:channelAdd")}
            </button>
          </div>
        </div>
        {channelEntries.length === 0 ? (
          <p className="muted settings__empty">{t("settings:channelEmpty")}</p>
        ) : (
          <ul className="settings__channel-list">
            {channelEntries.map(([login, override]) => (
              <li className="settings__channel-item" key={login}>
                <span className="settings__channel-login">{login}</span>
                <input
                  className="input"
                  value={override.quality ?? ""}
                  placeholder={t("settings:useGlobal")}
                  onChange={(e) =>
                    setChannelOverride(login, {
                      quality: e.target.value || undefined,
                    })
                  }
                  aria-label={`${login} ${t("settings:channelQuality")}`}
                />
                <button
                  type="button"
                  className="button-secondary"
                  onClick={() => setChannelOverride(login, null)}
                >
                  {t("settings:channelRemove")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
  );
}

export function SettingsGeneralPanel() {
  const { t } = useTranslation(["settings", "common"]);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const replaceSettings = useSettingsStore((s) => s.replaceSettings);
  const fileRef = useRef<HTMLInputElement>(null);
  return (
    <SettingsTabPanel>
        <label className="settings__row settings__row--check">
          <input
            type="checkbox"
            checked={settings.sentryEnabled}
            onChange={(e) => setSettings({ sentryEnabled: e.target.checked })}
          />
          <span className="settings__check-text">
            {t("settings:sentryEnabled")}
            <small className="muted">{t("settings:sentryHint")}</small>
          </span>
        </label>

        <label className="settings__row settings__row--check">
          <input
            type="checkbox"
            checked={settings.gui.debugMode}
            onChange={(e) =>
              setSettings({
                gui: { ...settings.gui, debugMode: e.target.checked },
              })
            }
          />
          <span className="settings__check-text">
            {t("settings:debugMode")}
            <small className="muted">{t("settings:debugModeHint")}</small>
          </span>
        </label>

        <div className="settings__row settings__row--actions">
          <div className="settings__label" />
          <div className="settings__actions">
            <button
              type="button"
              className="button-secondary"
              onClick={() => void invoke("diagnostics_open_logs").catch(() => undefined)}
            >
              {t("settings:openLogs")}
            </button>
            <button
              type="button"
              className="button-secondary"
              onClick={() =>
                void invoke("diagnostics_open_crashes").catch(() => undefined)
              }
            >
              {t("settings:openCrashes")}
            </button>
          </div>
        </div>

        <div className="settings__row settings__row--actions">
          <div className="settings__label" />
          <div className="settings__actions">
            <button
              type="button"
              className="button-secondary"
              onClick={() => {
                const blob = new Blob([exportSettingsJson(settings)], {
                  type: "application/json",
                });
                const url = URL.createObjectURL(blob);
                const a = document.createElement("a");
                a.href = url;
                a.download = "rillmux-settings.json";
                a.click();
                URL.revokeObjectURL(url);
              }}
            >
              {t("settings:exportSettings")}
            </button>
            <button
              type="button"
              className="button-secondary"
              onClick={() => fileRef.current?.click()}
            >
              {t("settings:importSettings")}
            </button>
            <input
              ref={fileRef}
              type="file"
              accept="application/json,.json"
              hidden
              onChange={async (e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                const text = await file.text();
                replaceSettings(importSettingsJson(text));
                e.target.value = "";
              }}
            />
          </div>
        </div>
    </SettingsTabPanel>
  );
}
