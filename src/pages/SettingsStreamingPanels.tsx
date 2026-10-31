import { useTranslation } from "react-i18next";
import { emit } from "@tauri-apps/api/event";
import { useSettingsStore } from "../lib/settings/store";
import { persistSettings } from "../lib/settings/persist";
import type { StreamOpenMode } from "../lib/settings/types";
import { invoke, isTauri } from "../lib/tauri";
import { syncViewerPresence, useWatchingStore } from "../lib/streaming/store";
import { POINTS_HUD_OFFSET_EVENT } from "../lib/streaming/pointsHud";

const QUALITY_PRESETS = [
  "best",
  "2160p60",
  "2160p",
  "1440p60",
  "1440p",
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

export function SettingsStreamingPlaybackPanel() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const qualityIsCustom = !isPresetQuality(settings.streaming.quality);
  return (
    <>
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
              aria-label={t("settings:quality")}
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
            <small className="muted">{t("settings:lowLatencyHint")}</small>
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
            <small className="muted">{t("settings:disableAdsHint")}</small>
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
            <small className="muted">{t("settings:channelPointsHint")}</small>
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
            <small className="muted">{t("settings:channelPointsPollsHint")}</small>
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
            <small className="muted">{t("settings:channelPointsHudHint")}</small>
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
    </>
  );
}
export function SettingsStreamingLayoutPanel() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const applyLayout = useWatchingStore((s) => s.applyLayout);
  return (
    <>
        <label className="settings__row">
          <span className="settings__label">
            <span>{t("settings:streamOpenMode")}</span>
            <small className="muted">{t("settings:streamOpenModeHint")}</small>
          </span>
          <select
            value={settings.streaming.streamOpenMode}
            onChange={(e) => {
              const mode = e.target.value as StreamOpenMode;
              setSettings({
                streaming: {
                  ...settings.streaming,
                  streamOpenMode: mode,
                },
              });
              if (isTauri()) {
                void invoke("dock_set_linked", {
                  enabled:
                    mode === "multistream" && settings.streaming.linkedDock,
                }).catch(() => undefined);
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
            <option value="independent">{t("settings:streamOpenIndependent")}</option>
            <option value="seamless">{t("settings:streamOpenSeamless")}</option>
            <option value="multistream">{t("settings:streamOpenMultistream")}</option>
          </select>
        </label>
        {settings.streaming.streamOpenMode === "multistream" ? (
          <label className="settings__row">
            <span className="settings__label">
              <span>{t("settings:multistreamLayout")}</span>
              <small className="muted">{t("settings:multistreamLayoutHint")}</small>
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
              <small className="muted">{t("settings:unevenMainSideHint")}</small>
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
                applyLayout();
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
                  void invoke("dock_set_linked", { enabled }).catch(() => undefined);
                }
                queueMicrotask(applyLayout);
              }}
            />
            <span className="settings__check-text">
              {t("settings:linkedDock")}
              <small className="muted">{t("settings:linkedDockHint")}</small>
            </span>
          </label>
        ) : null}
        <label className="settings__row settings__row--check">
          <input
            type="checkbox"
            checked={settings.streaming.followRaids}
            onChange={async (e) => {
              setSettings({
                streaming: {
                  ...settings.streaming,
                  followRaids: e.target.checked,
                },
              });
              const { syncEventSub } = await import("../lib/streaming/store");
              syncEventSub();
            }}
          />
          <span className="settings__check-text">
            {t("settings:followRaids")}
            <small className="muted">{t("settings:followRaidsHint")}</small>
          </span>
        </label>
        {settings.streaming.streamOpenMode === "multistream" &&
        settings.streaming.linkedDock ? (
          <label className="settings__row">
            <span className="settings__label">
              <span>{t("settings:chatWidthFraction")}</span>
              <small className="muted">{t("settings:chatWidthFractionHint")}</small>
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
            <small className="muted">{t("settings:webbrowserHint")}</small>
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
                aria-label={t("settings:retryStreams")}
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
                aria-label={t("settings:retryMax")}
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
    </>
  );
}
