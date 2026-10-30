import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettingsStore } from "../lib/settings/store";
import type { PlayerId, PlayerInput } from "../lib/settings/types";
import {
  clampMpvVolume,
  defaultMpvPresets,
  describeMpvPresets,
  MPV_VOLUME_BOOSTS,
  normalizeMpvVolumeBoost,
  normalizeMpvVolumeMode,
} from "../lib/settings/mpv";
import { playerInstallGuide } from "../lib/settings/playerInstall";
import { isTauri } from "../lib/tauri";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import { SettingsTabPanel } from "./SettingsTabs";

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
        <dialog
          id={menuId}
          className="settings__help-menu"
          open
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
        </dialog>
      ) : null}
    </div>
  );
}
function MpvVolumeSettings() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((state) => state.settings);
  const setSettings = useSettingsStore((state) => state.setSettings);
  const mpv = settings.player.mpv;
  return (
    <>
      <div className="settings__row">
        <div className="settings__label">
          <span>{t("settings:playerMpvVolumeMode")}</span>
          <small className="muted">
            {t("settings:playerMpvVolumeModeHint")}
          </small>
        </div>
        <div className="settings__control">
          <select
            value={mpv.volumeMode}
            aria-label={t("settings:playerMpvVolumeMode")}
            onChange={(event) =>
              setSettings({
                player: {
                  ...settings.player,
                  mpv: {
                    ...mpv,
                    volumeMode: normalizeMpvVolumeMode(event.target.value),
                  },
                },
              })
            }
          >
            <option value="last">
              {t("settings:playerMpvVolumeModeLast")}
            </option>
            <option value="max">
              {t("settings:playerMpvVolumeModeMax")}
            </option>
            <option value="custom">
              {t("settings:playerMpvVolumeModeCustom")}
            </option>
          </select>
        </div>
      </div>
      <div className="settings__row">
        <div className="settings__label">
          <span>{t("settings:playerMpvVolumeBoost")}</span>
          <small className="muted">
            {t("settings:playerMpvVolumeBoostHint")}
          </small>
        </div>
        <div className="settings__control">
          <select
            value={String(mpv.volumeBoost)}
            aria-label={t("settings:playerMpvVolumeBoost")}
            onChange={(event) =>
              setSettings({
                player: {
                  ...settings.player,
                  mpv: {
                    ...mpv,
                    volumeBoost: normalizeMpvVolumeBoost(
                      Number(event.target.value),
                    ),
                  },
                },
              })
            }
          >
            {MPV_VOLUME_BOOSTS.map((level) => (
              <option key={level} value={level}>
                {level === 100
                  ? t("settings:playerMpvVolumeBoostOff")
                  : t("settings:playerMpvVolumeBoostLevel", { level })}
              </option>
            ))}
          </select>
        </div>
      </div>
      {mpv.volumeMode === "custom" ? (
        <div className="settings__row">
          <div className="settings__label">
            <span>{t("settings:playerMpvVolumeCustom")}</span>
            <small className="muted">
              {t("settings:playerMpvVolumeCustomHint", {
                maximum: mpv.volumeBoost,
              })}
            </small>
          </div>
          <div className="settings__control">
            <input
              className="input"
              type="number"
              min={1}
              max={mpv.volumeBoost}
              value={mpv.volumeCustom}
              aria-label={t("settings:playerMpvVolumeCustom")}
              onChange={(event) =>
                setSettings({
                  player: {
                    ...settings.player,
                    mpv: {
                      ...mpv,
                      volumeCustom: clampMpvVolume(
                        event.target.value,
                        mpv.volumeBoost,
                      ),
                    },
                  },
                })
              }
            />
          </div>
        </div>
      ) : null}
    </>
  );
}
export function SettingsPlayerPanel() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  return (
    <SettingsTabPanel>
        <div className="settings__row">
          <div className="settings__label">
            <span>{t("settings:playerId")}</span>
          </div>
          <PlayerInstallHelp
            key={settings.player.id}
            guide={playerInstallGuide(settings.player.id)}
          >
            <select
              value={settings.player.id}
              aria-label={t("settings:playerId")}
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
            <small className="muted">{t("settings:playerCustomPathHint")}</small>
          </div>
          <div className="settings__control settings__control--row settings__control--path">
            <input
              className="input"
              value={settings.player.customPath}
              disabled={settings.player.id !== "custom"}
              tabIndex={settings.player.id === "custom" ? 0 : -1}
              onChange={(e) =>
                setSettings({
                  player: { ...settings.player, customPath: e.target.value },
                })
              }
              aria-label={t("settings:playerCustomPath")}
            />
            <button
              type="button"
              className="button-secondary"
              disabled={settings.player.id !== "custom"}
              onClick={async () => {
                const path = await pickExecutablePath();
                if (!path) return;
                setSettings({
                  player: { ...settings.player, customPath: path },
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
              <small className="muted">{t("settings:playerMpvPresetsLede")}</small>
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
                  ["autoCatchUp", "playerMpvAutoCatchUp"],
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
        {settings.player.id === "mpv" ? <MpvVolumeSettings /> : null}
        <div className="settings__row">
          <div className="settings__label">
            <span>{t("settings:playerCustomArgs")}</span>
            <small className="muted">{t("settings:playerCustomArgsHint")}</small>
          </div>
          <div className="settings__control">
            <input
              className="input"
              value={settings.player.customArgs}
              aria-label={t("settings:playerCustomArgs")}
              onChange={(e) =>
                setSettings({
                  player: { ...settings.player, customArgs: e.target.value },
                })
              }
            />
          </div>
        </div>
        <div className="settings__row">
          <div className="settings__label">
            <span>{t("settings:playerInput")}</span>
            <small className="muted">{t("settings:playerInputHint")}</small>
          </div>
          <div className="settings__control">
            <select
              value={settings.player.input}
              aria-label={t("settings:playerInput")}
              onChange={(e) =>
                setSettings({
                  player: {
                    ...settings.player,
                    input: e.target.value as PlayerInput,
                  },
                })
              }
            >
              <option value="default">{t("settings:playerInputDefault")}</option>
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
    </SettingsTabPanel>
  );
}
