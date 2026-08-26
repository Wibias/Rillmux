import { useEffect, useId, useRef, useState } from "react";
import { Link, useLocation, useNavigate } from "react-router";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
import { useSettingsStore } from "../lib/settings/store";
import {
  exportSettingsJson,
  importSettingsJson,
  loadPersistedSettings,
  persistSettings,
} from "../lib/settings/persist";
import type {
  ChatProvider,
  HotkeySettings,
  PlayerId,
  PlayerInput,
  StreamOpenMode,
  ThemeMode,
} from "../lib/settings/types";
import { defaultMpvPresets, describeMpvPresets } from "../lib/settings/mpv";
import { playerInstallGuide } from "../lib/settings/playerInstall";
import { eventToHotkey, normalizeHotkey } from "../lib/hotkeys";
import { toggleMutedFollowed } from "../lib/notifications/followedLive";
import { invoke, isTauri } from "../lib/tauri";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import { syncViewerPresence, useWatchingStore } from "../lib/streaming/store";
import {
  SETTINGS_TABS,
  settingsTabFromPath,
  settingsTabLabelKey,
} from "../lib/settings/tabs";
import {
  isOverlayWebview,
  shouldAttachDebugConsole,
} from "../lib/settings/debugConsole";
import {
  POINTS_HUD_OFFSET_EVENT,
  hudOffsetFromSearch,
  hudOffsetFromUnknown,
  hudOffsetsEqual,
  type HudOffset,
} from "../lib/streaming/pointsHud";
import "./SettingsPage.css";
import "../components/SetupHelp.css";

async function openExternal(url: string) {
  if (isTauri()) {
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

async function pickExecutablePath(): Promise<string | null> {
  if (!isTauri()) return null;
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Programs", extensions: ["exe"] }],
  });
  if (Array.isArray(selected)) return selected[0] ?? null;
  return selected;
}

const QUALITY_PRESETS = [
  "best",
  "1080p60",
  "1080p",
  "720p60",
  "720p",
  "480p",
  "360p",
  "160p",
  "audio_only",
  "worst",
] as const;

function isPresetQuality(quality: string): boolean {
  return (QUALITY_PRESETS as readonly string[]).includes(quality);
}

const PLAYER_INSTALL_PORTABLE_KEYS = {
  mpv: "settings:playerInstallPortable",
  vlc: "settings:playerInstallPortableVlc",
  mpc: "settings:playerInstallPortableMpc",
  potplayer: "settings:playerInstallPortablePot",
} as const satisfies Record<
  Exclude<PlayerId, "custom">,
  `settings:${string}`
>;

function PlayerInstallHelp({
  guide,
  children,
}: {
  guide: ReturnType<typeof playerInstallGuide>;
  children: React.ReactNode;
}) {
  const { t } = useTranslation(["settings"]);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const menuId = `${useId()}-player-install`;
  const guideId = guide?.id ?? null;

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  useEffect(() => {
    if (!guideId) setOpen(false);
  }, [guideId]);

  return (
    <div className="settings__control settings__control--player" ref={rootRef}>
      {guide ? (
        <button
          type="button"
          className="settings__help"
          aria-expanded={open}
          aria-haspopup="dialog"
          aria-controls={menuId}
          aria-label={t("settings:playerInstallTitle", { player: guide.name })}
          title={t("settings:playerInstallTitle", { player: guide.name })}
          onClick={() => setOpen((value) => !value)}
        >
          <span aria-hidden="true">?</span>
        </button>
      ) : null}
      {children}
      {guide && open ? (
        <div
          id={menuId}
          className="settings__help-menu"
          role="dialog"
          aria-label={t("settings:playerInstallTitle", { player: guide.name })}
        >
          <p className="settings__help-menu-title">
            {t("settings:playerInstallTitle", { player: guide.name })}
          </p>
          <p className="setup-help__body muted">
            {t("settings:playerInstallOpenShell")}
          </p>
          <div className="setup-help__cmds">
            <div>
              <span className="muted">{t("settings:playerInstallWinget")}</span>
              <code>{guide.winget}</code>
            </div>
            <div>
              <span className="muted">{t("settings:playerInstallScoop")}</span>
              <code>{guide.scoop}</code>
            </div>
          </div>
          <div className="setup-help__footer">
            <p className="setup-help__body muted">
              {t(PLAYER_INSTALL_PORTABLE_KEYS[guide.id])}
            </p>
            <div className="setup-help__actions">
              <button
                type="button"
                className="button-secondary"
                onClick={() => void openExternal(guide.downloadUrl)}
              >
                {t("settings:playerInstallDownloadLink")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

export function SettingsPage() {
  const { t } = useTranslation(["routes", "settings", "common"]);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const replaceSettings = useSettingsStore((s) => s.replaceSettings);
  const setChannelOverride = useSettingsStore((s) => s.setChannelOverride);
  const applyLayout = useWatchingStore((s) => s.applyLayout);
  const location = useLocation();
  const navigate = useNavigate();
  const activeTab = settingsTabFromPath(location.pathname);
  const fileRef = useRef<HTMLInputElement>(null);
  const [newChannelLogin, setNewChannelLogin] = useState("");
  const [newChannelQuality, setNewChannelQuality] = useState("");
  const [newMutedLogin, setNewMutedLogin] = useState("");

  const qualityIsCustom = !isPresetQuality(settings.streaming.quality);
  const channelEntries = Object.entries(settings.channels);

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
    <section className="settings">
      <header className="page__header">
        <h1>{t("routes:settingsTitle")}</h1>
      </header>

      <div className="settings__layout">
        <div
          className="settings__nav"
          role="tablist"
          aria-label={t("routes:settingsTitle")}
        >
          {SETTINGS_TABS.map((tab) => (
            <button
              key={tab}
              type="button"
              role="tab"
              id={`settings-tab-${tab}`}
              aria-controls="settings-panel"
              aria-selected={activeTab === tab}
              className={activeTab === tab ? "settings__tab is-active" : "settings__tab"}
              onClick={() => navigate(`/settings/${tab}`)}
            >
              {t(`settings:${settingsTabLabelKey(tab)}`)}
            </button>
          ))}
        </div>

        <div className="settings__main">
          <h2 className="settings__heading" id="settings-panel-title">
            {t(`settings:${settingsTabLabelKey(activeTab)}`)}
          </h2>

          {activeTab === "interface" ? (
            <div
              className="settings__group"
              role="tabpanel"
              id="settings-panel"
              aria-labelledby="settings-panel-title"
            >
              <div className="settings__row">
                <div className="settings__label">
                  <span>{t("settings:theme")}</span>
                </div>
                <div className="settings__control">
                  <select
                    value={settings.theme}
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
                  <small className="muted">
                    {t("settings:showSetupAgainHint")}
                  </small>
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
            </div>
          ) : null}

          {activeTab === "streaming" ? (
            <div
              className="settings__group"
              role="tabpanel"
              id="settings-panel"
              aria-labelledby="settings-panel-title"
            >
              <div className="settings__row settings__row--quality">
                <div className="settings__label">
                  <span>{t("settings:quality")}</span>
                </div>
                <div className="settings__control settings__control--quality">
                  <input
                    className="input"
                    value={qualityIsCustom ? settings.streaming.quality : ""}
                    disabled={!qualityIsCustom}
                    tabIndex={qualityIsCustom ? 0 : -1}
                    onChange={(e) =>
                      setSettings({
                        streaming: {
                          ...settings.streaming,
                          quality: e.target.value,
                        },
                      })
                    }
                    placeholder="720p,720p60"
                    aria-label={t("settings:qualityCustom")}
                  />
                  <select
                    value={qualityIsCustom ? "custom" : settings.streaming.quality}
                    onChange={(e) => {
                      const value = e.target.value;
                      if (value === "custom") {
                        setSettings({
                          streaming: { ...settings.streaming, quality: "" },
                        });
                        return;
                      }
                      setSettings({
                        streaming: { ...settings.streaming, quality: value },
                      });
                    }}
                  >
                    {QUALITY_PRESETS.map((q) => (
                      <option key={q} value={q}>
                        {q}
                      </option>
                    ))}
                    <option value="custom">{t("settings:qualityCustom")}</option>
                  </select>
                </div>
              </div>

              <label className="settings__row settings__row--check">
                <input
                  type="checkbox"
                  checked={settings.streaming.lowLatency}
                  onChange={(e) =>
                    setSettings({
                      streaming: {
                        ...settings.streaming,
                        lowLatency: e.target.checked,
                      },
                    })
                  }
                />
                <span className="settings__check-text">
                  {t("settings:lowLatency")}
                  <small className="muted">
                    {t("settings:lowLatencyHint")}
                  </small>
                </span>
              </label>

              <label className="settings__row settings__row--check">
                <input
                  type="checkbox"
                  checked={settings.streaming.disableAds}
                  onChange={(e) =>
                    setSettings({
                      streaming: {
                        ...settings.streaming,
                        disableAds: e.target.checked,
                      },
                    })
                  }
                />
                <span className="settings__check-text">
                  {t("settings:disableAds")}
                  <small className="muted">
                    {t("settings:disableAdsHint")}
                  </small>
                </span>
              </label>

              <label className="settings__row settings__row--check">
                <input
                  type="checkbox"
                  checked={settings.streaming.channelPoints}
                  onChange={(e) => {
                    setSettings({
                      streaming: {
                        ...settings.streaming,
                        channelPoints: e.target.checked,
                      },
                    });
                    queueMicrotask(syncViewerPresence);
                  }}
                />
                <span className="settings__check-text">
                  {t("settings:channelPoints")}
                  <small className="muted">
                    {t("settings:channelPointsHint")}
                  </small>
                </span>
              </label>

              <label className="settings__row settings__row--check">
                <input
                  type="checkbox"
                  checked={settings.streaming.channelPointsPolls}
                  disabled={!settings.streaming.channelPoints}
                  onChange={(e) =>
                    setSettings({
                      streaming: {
                        ...settings.streaming,
                        channelPointsPolls: e.target.checked,
                      },
                    })
                  }
                />
                <span className="settings__check-text">
                  {t("settings:channelPointsPolls")}
                  <small className="muted">
                    {t("settings:channelPointsPollsHint")}
                  </small>
                </span>
              </label>

              <label className="settings__row settings__row--check">
                <input
                  type="checkbox"
                  checked={settings.streaming.channelPointsHud}
                  disabled={!settings.streaming.channelPoints}
                  onChange={(e) =>
                    setSettings({
                      streaming: {
                        ...settings.streaming,
                        channelPointsHud: e.target.checked,
                      },
                    })
                  }
                />
                <span className="settings__check-text">
                  {t("settings:channelPointsHud")}
                  <small className="muted">
                    {t("settings:channelPointsHudHint")}
                  </small>
                </span>
              </label>

              <div className="settings__row settings__row--actions">
                <div className="settings__label">
                  <span>{t("settings:channelPointsHudReset")}</span>
                  <small className="muted">
                    {t("settings:channelPointsHudResetHint")}
                  </small>
                </div>
                <div className="settings__actions">
                  <button
                    type="button"
                    className="button-secondary"
                    disabled={
                      !settings.streaming.channelPoints ||
                      !settings.streaming.channelPointsHud
                    }
                    onClick={() => {
                      setSettings({
                        streaming: {
                          ...settings.streaming,
                          channelPointsHudOffset: null,
                        },
                      });
                      void persistSettings(useSettingsStore.getState().settings);
                      if (isTauri()) {
                        void emit(POINTS_HUD_OFFSET_EVENT, null);
                      }
                    }}
                  >
                    {t("settings:channelPointsHudResetBtn")}
                  </button>
                </div>
              </div>

              <label className="settings__row">
                <span className="settings__label">
                  <span>{t("settings:streamOpenMode")}</span>
                  <small className="muted">
                    {t("settings:streamOpenModeHint")}
                  </small>
                </span>
                <select
                  value={settings.streaming.streamOpenMode}
                  onChange={(e) => {
                    const mode = e.target.value as StreamOpenMode;
                    const linkedDock =
                      mode === "multistream"
                        ? settings.streaming.linkedDock
                        : false;
                    setSettings({
                      streaming: {
                        ...settings.streaming,
                        streamOpenMode: mode,
                        linkedDock,
                      },
                    });
                    if (isTauri()) {
                      void invoke("dock_set_linked", { enabled: linkedDock }).catch(
                        () => undefined,
                      );
                    }
                    queueMicrotask(() => {
                      void useWatchingStore
                        .getState()
                        .refresh()
                        .then(() => useWatchingStore.getState().applyLayout())
                        .catch(() => undefined);
                    });
                  }}
                >
                  <option value="independent">
                    {t("settings:streamOpenIndependent")}
                  </option>
                  <option value="seamless">
                    {t("settings:streamOpenSeamless")}
                  </option>
                  <option value="multistream">
                    {t("settings:streamOpenMultistream")}
                  </option>
                </select>
              </label>

              {settings.streaming.streamOpenMode === "multistream" ? (
                <label className="settings__row">
                  <span className="settings__label">
                    <span>{t("settings:multistreamLayout")}</span>
                    <small className="muted">
                      {t("settings:multistreamLayoutHint")}
                    </small>
                  </span>
                  <select
                    value={settings.streaming.multistreamLayout}
                    onChange={(e) => {
                      setSettings({
                        streaming: {
                          ...settings.streaming,
                          multistreamLayout: e.target
                            .value as typeof settings.streaming.multistreamLayout,
                        },
                      });
                      queueMicrotask(applyLayout);
                    }}
                  >
                    <option value="1">{t("settings:layout1")}</option>
                    <option value="2">{t("settings:layout2")}</option>
                    <option value="1x2">{t("settings:layout1x2")}</option>
                    <option value="1x3">{t("settings:layout1x3")}</option>
                    <option value="1x4">{t("settings:layout1x4")}</option>
                    <option value="2plus1">{t("settings:layout2plus1")}</option>
                    <option value="2x2">{t("settings:layout2x2")}</option>
                    <option value="3plus1">{t("settings:layout3plus1")}</option>
                    <option value="3x2">{t("settings:layout3x2")}</option>
                    <option value="4x2">{t("settings:layout4x2")}</option>
                    <option value="8x1">{t("settings:layout8x1")}</option>
                  </select>
                </label>
              ) : null}

              {settings.streaming.streamOpenMode === "multistream" &&
              (settings.streaming.multistreamLayout === "2plus1" ||
                settings.streaming.multistreamLayout === "3plus1") ? (
                <label className="settings__row">
                  <span className="settings__label">
                    <span>{t("settings:unevenMainSide")}</span>
                    <small className="muted">
                      {t("settings:unevenMainSideHint")}
                    </small>
                  </span>
                  <select
                    value={settings.streaming.unevenMainSide}
                    onChange={(e) => {
                      setSettings({
                        streaming: {
                          ...settings.streaming,
                          unevenMainSide: e.target
                            .value as typeof settings.streaming.unevenMainSide,
                        },
                      });
                      queueMicrotask(applyLayout);
                    }}
                  >
                    <option value="left">{t("settings:mainSideLeft")}</option>
                    <option value="right">{t("settings:mainSideRight")}</option>
                    <option value="top">{t("settings:mainSideTop")}</option>
                    <option value="bottom">{t("settings:mainSideBottom")}</option>
                  </select>
                </label>
              ) : null}

              {settings.streaming.streamOpenMode === "multistream" ? (
                <label className="settings__row settings__row--check">
                  <input
                    type="checkbox"
                    checked={settings.streaming.linkedDock}
                    onChange={(e) => {
                      const enabled = e.target.checked;
                      setSettings({
                        streaming: {
                          ...settings.streaming,
                          linkedDock: enabled,
                        },
                      });
                      if (isTauri()) {
                        void invoke("dock_set_linked", { enabled }).catch(
                          () => undefined,
                        );
                      }
                      queueMicrotask(applyLayout);
                    }}
                  />
                  <span className="settings__check-text">
                    {t("settings:linkedDock")}
                    <small className="muted">
                      {t("settings:linkedDockHint")}
                    </small>
                  </span>
                </label>
              ) : null}

              <label className="settings__row settings__row--check">
                <input
                  type="checkbox"
                  checked={settings.streaming.followRaids}
                  onChange={(e) => {
                    setSettings({
                      streaming: {
                        ...settings.streaming,
                        followRaids: e.target.checked,
                      },
                    });
                    void import("../lib/streaming/store").then(({ syncEventSub }) => {
                      syncEventSub();
                    });
                  }}
                />
                <span className="settings__check-text">
                  {t("settings:followRaids")}
                  <small className="muted">
                    {t("settings:followRaidsHint")}
                  </small>
                </span>
              </label>

              {settings.streaming.streamOpenMode === "multistream" &&
              settings.streaming.linkedDock ? (
                <label className="settings__row">
                  <span className="settings__label">
                    <span>{t("settings:chatWidthFraction")}</span>
                    <small className="muted">
                      {t("settings:chatWidthFractionHint")}
                    </small>
                  </span>
                  <input
                    type="range"
                    min={12}
                    max={45}
                    step={1}
                    value={Math.round(settings.streaming.chatWidthFraction * 100)}
                    onChange={(e) => {
                      const fraction = Number(e.target.value) / 100;
                      setSettings({
                        streaming: {
                          ...settings.streaming,
                          chatWidthFraction: fraction,
                        },
                      });
                      if (isTauri()) {
                        void invoke("dock_set_chat_fraction", { fraction }).catch(
                          () => undefined,
                        );
                      }
                    }}
                  />
                  <span className="muted">
                    {Math.round(settings.streaming.chatWidthFraction * 100)}%
                  </span>
                </label>
              ) : null}

              <label className="settings__row settings__row--check">
                <input
                  type="checkbox"
                  checked={settings.streaming.webbrowser}
                  onChange={(e) =>
                    setSettings({
                      streaming: {
                        ...settings.streaming,
                        webbrowser: e.target.checked,
                      },
                    })
                  }
                />
                <span className="settings__check-text">
                  {t("settings:webbrowser")}
                  <small className="muted">
                    {t("settings:webbrowserHint")}
                  </small>
                </span>
              </label>

              <div className="settings__row settings__row--pair">
                <label className="settings__pair settings__pair--check">
                  <input
                    type="checkbox"
                    checked={settings.streaming.webbrowserHeadless}
                    onChange={(e) =>
                      setSettings({
                        streaming: {
                          ...settings.streaming,
                          webbrowserHeadless: e.target.checked,
                        },
                      })
                    }
                  />
                  <span className="settings__check-text">
                    {t("settings:webbrowserHeadless")}
                  </span>
                </label>
                <div className="settings__pair">
                  <div className="settings__label">
                    <span>{t("settings:retryStreams")}</span>
                  </div>
                  <div className="settings__control">
                    <input
                      className="input"
                      type="number"
                      min={0}
                      value={settings.streaming.retryStreams}
                      onChange={(e) =>
                        setSettings({
                          streaming: {
                            ...settings.streaming,
                            retryStreams: Number(e.target.value) || 0,
                          },
                        })
                      }
                    />
                  </div>
                </div>
                <div className="settings__pair">
                  <div className="settings__label">
                    <span>{t("settings:retryMax")}</span>
                  </div>
                  <div className="settings__control">
                    <input
                      className="input"
                      type="number"
                      min={0}
                      value={settings.streaming.retryMax}
                      onChange={(e) =>
                        setSettings({
                          streaming: {
                            ...settings.streaming,
                            retryMax: Number(e.target.value) || 0,
                          },
                        })
                      }
                    />
                  </div>
                </div>
              </div>
            </div>
          ) : null}

          {activeTab === "player" ? (
            <div
              className="settings__group"
              role="tabpanel"
              id="settings-panel"
              aria-labelledby="settings-panel-title"
            >
              <div className="settings__row">
                <div className="settings__label">
                  <span>{t("settings:playerId")}</span>
                </div>
                <PlayerInstallHelp guide={playerInstallGuide(settings.player.id)}>
                  <select
                    value={settings.player.id}
                    onChange={(e) =>
                      setSettings({
                        player: {
                          ...settings.player,
                          id: e.target.value as PlayerId,
                        },
                      })
                    }
                  >
                    <option value="mpv">mpv</option>
                    <option value="vlc">VLC</option>
                    <option value="mpc">MPC-HC</option>
                    <option value="potplayer">PotPlayer</option>
                    <option value="custom">{t("settings:chatCustom")}</option>
                  </select>
                </PlayerInstallHelp>
              </div>

              <div className="settings__row">
                <div className="settings__label">
                  <span>{t("settings:playerCustomPath")}</span>
                  <small className="muted">
                    {t("settings:playerCustomPathHint")}
                  </small>
                </div>
                <div className="settings__control settings__control--row settings__control--path">
                  <input
                    className="input"
                    value={settings.player.customPath}
                    disabled={settings.player.id !== "custom"}
                    tabIndex={settings.player.id === "custom" ? 0 : -1}
                    onChange={(e) =>
                      setSettings({
                        player: {
                          ...settings.player,
                          customPath: e.target.value,
                        },
                      })
                    }
                    aria-label={t("settings:playerCustomPath")}
                  />
                  <button
                    type="button"
                    className="button-secondary"
                    disabled={settings.player.id !== "custom"}
                    onClick={() => {
                      void pickExecutablePath().then((path) => {
                        if (!path) return;
                        setSettings({
                          player: { ...settings.player, customPath: path },
                        });
                      });
                    }}
                  >
                    {t("settings:browseFile")}
                  </button>
                </div>
              </div>

              {settings.player.id === "mpv" ? (
                <div className="settings__row settings__row--stack">
                  <div className="settings__label">
                    <span>{t("settings:playerMpvPresets")}</span>
                    <small className="muted">
                      {t("settings:playerMpvPresetsLede")}
                    </small>
                    <small className="muted">
                      {describeMpvPresets(settings.player.mpv).length
                        ? t("settings:playerMpvIncluded", {
                            list: describeMpvPresets(settings.player.mpv).join(", "),
                          })
                        : t("settings:playerMpvIncludedNone")}
                    </small>
                  </div>
                  <div className="settings__control settings__mpv-presets">
                    {(
                      [
                        ["noBorder", "playerMpvNoBorder"],
                        ["noKeepaspectWindow", "playerMpvNoKeepaspect"],
                        ["windowMaximized", "playerMpvMaximized"],
                        ["loopReload", "playerMpvLoopReload"],
                        ["cacheRewind", "playerMpvCacheRewind"],
                      ] as const
                    ).map(([key, labelKey]) => (
                      <label key={key} className="settings__row settings__row--check">
                        <input
                          type="checkbox"
                          checked={settings.player.mpv[key]}
                          onChange={(e) =>
                            setSettings({
                              player: {
                                ...settings.player,
                                mpv: {
                                  ...settings.player.mpv,
                                  [key]: e.target.checked,
                                },
                              },
                            })
                          }
                        />
                        <span className="settings__check-text">
                          {t(`settings:${labelKey}`)}
                        </span>
                      </label>
                    ))}
                    <button
                      type="button"
                      className="button-secondary"
                      onClick={() =>
                        setSettings({
                          player: {
                            ...settings.player,
                            mpv: defaultMpvPresets(),
                            customArgs: "",
                          },
                        })
                      }
                    >
                      {t("settings:playerMpvReset")}
                    </button>
                  </div>
                </div>
              ) : null}

              <div className="settings__row">
                <div className="settings__label">
                  <span>{t("settings:playerCustomArgs")}</span>
                  <small className="muted">
                    {t("settings:playerCustomArgsHint")}
                  </small>
                </div>
                <div className="settings__control">
                  <input
                    className="input"
                    value={settings.player.customArgs}
                    onChange={(e) =>
                      setSettings({
                        player: {
                          ...settings.player,
                          customArgs: e.target.value,
                        },
                      })
                    }
                  />
                </div>
              </div>

              <div className="settings__row">
                <div className="settings__label">
                  <span>{t("settings:playerInput")}</span>
                  <small className="muted">
                    {t("settings:playerInputHint")}
                  </small>
                </div>
                <div className="settings__control">
                  <select
                    value={settings.player.input}
                    onChange={(e) =>
                      setSettings({
                        player: {
                          ...settings.player,
                          input: e.target.value as PlayerInput,
                        },
                      })
                    }
                  >
                    <option value="default">
                      {t("settings:playerInputDefault")}
                    </option>
                    <option value="fifo">{t("settings:playerInputFifo")}</option>
                    <option value="http">{t("settings:playerInputHttp")}</option>
                  </select>
                </div>
              </div>

              <label className="settings__row settings__row--check">
                <input
                  type="checkbox"
                  checked={settings.streaming.playerNoClose}
                  onChange={(e) =>
                    setSettings({
                      streaming: {
                        ...settings.streaming,
                        playerNoClose: e.target.checked,
                      },
                    })
                  }
                />
                <span className="settings__check-text">
                  {t("settings:playerNoClose")}
                </span>
              </label>
            </div>
          ) : null}

          {activeTab === "chat" ? (
            <div
              className="settings__group"
              role="tabpanel"
              id="settings-panel"
              aria-labelledby="settings-panel-title"
            >
              <div className="settings__row">
                <div className="settings__label">
                  <span>{t("settings:chatProvider")}</span>
                  <small className="muted">
                    {t("settings:chatProviderHint")}
                  </small>
                </div>
                <div className="settings__control">
                  <select
                    value={settings.chat.provider}
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
                    <option value="chatterino">
                      {t("settings:chatChatterino")}
                    </option>
                    <option value="browser">{t("settings:chatBrowser")}</option>
                    <option value="chrome">{t("settings:chatChrome")}</option>
                    <option value="custom">{t("settings:chatCustom")}</option>
                  </select>
                </div>
              </div>
            </div>
          ) : null}

          {activeTab === "notifications" ? (
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
                    <small className="muted">
                      {t("settings:followedOnlineHint")}
                    </small>
                  </span>
                </label>
                <p className="muted settings__hint">
                  {t("settings:mutedFollowedHint")}
                </p>
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
                <p className="muted settings__empty">
                  {t("settings:mutedFollowedEmpty")}
                </p>
              ) : (
                <ul className="settings__channel-list">
                  {settings.notifications.mutedFollowed.map((login) => (
                    <li key={login} className="settings__channel-item">
                      <Link
                        className="settings__channel-login"
                        to={`/channel/${login}`}
                      >
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
          ) : null}

          {activeTab === "hotkeys" ? (
            <div
              className="settings__group"
              role="tabpanel"
              id="settings-panel"
              aria-labelledby="settings-panel-title"
            >
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
            </div>
          ) : null}

          {activeTab === "channels" ? (
            <div
              className="settings__section"
              role="tabpanel"
              id="settings-panel"
              aria-labelledby="settings-panel-title"
            >
              <div className="settings__group">
                <p className="muted settings__hint">
                  {t("settings:channelsHint")}
                </p>
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
                <p className="muted settings__empty">
                  {t("settings:channelEmpty")}
                </p>
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
          ) : null}

          {activeTab === "general" ? (
            <div
              className="settings__group"
              role="tabpanel"
              id="settings-panel"
              aria-labelledby="settings-panel-title"
            >
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
                    onClick={() =>
                      void invoke("diagnostics_open_logs").catch(() => undefined)
                    }
                  >
                    {t("settings:openLogs")}
                  </button>
                  <button
                    type="button"
                    className="button-secondary"
                    onClick={() =>
                      void invoke("diagnostics_open_crashes").catch(
                        () => undefined,
                      )
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
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}

let pendingHudOffset: { received: true; value: HudOffset } | { received: false } =
  { received: false };

function applyHudOffset(next: HudOffset) {
  const store = useSettingsStore.getState();
  const current = store.settings.streaming.channelPointsHudOffset;
  if (hudOffsetsEqual(current, next)) return;
  store.setSettings({
    streaming: {
      ...store.settings.streaming,
      channelPointsHudOffset: next,
    },
  });
}

export function SettingsBootstrap({ children }: { children: React.ReactNode }) {
  const hydrate = useSettingsStore((s) => s.hydrate);
  const hydrated = useSettingsStore((s) => s.hydrated);
  const settings = useSettingsStore((s) => s.settings);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    // Listen before hydrate so a reset event cannot lose to a stale settings.json.
    if (isTauri()) {
      void listen(POINTS_HUD_OFFSET_EVENT, (event) => {
        const next = hudOffsetFromUnknown(event.payload);
        pendingHudOffset = { received: true, value: next };
        if (cancelled) return;
        if (useSettingsStore.getState().hydrated) {
          applyHudOffset(next);
        }
      }).then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unlisten = fn;
      });
    }

    void loadPersistedSettings().then((loaded) => {
      if (cancelled) return;
      hydrate(loaded);
      const fromUrl = hudOffsetFromSearch();
      if (fromUrl.found) applyHudOffset(fromUrl.offset);
      if (pendingHudOffset.received) applyHudOffset(pendingHudOffset.value);
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [hydrate]);

  useEffect(() => {
    if (!hydrated || !isTauri()) return;
    if (!shouldAttachDebugConsole()) return;
    void invoke("diagnostics_set_debug", {
      enabled: settings.gui.debugMode,
    }).catch(() => undefined);
  }, [hydrated, settings.gui.debugMode]);

  useEffect(() => {
    if (!hydrated) return;
    // Overlay webviews must not write settings.json — a remount can persist a
    // stale Channel Points offset and shove the chip back under the caption.
    if (isOverlayWebview()) return;
    const handle = window.setTimeout(() => {
      void persistSettings(settings);
    }, 400);
    return () => window.clearTimeout(handle);
  }, [settings, hydrated]);

  return children;
}
