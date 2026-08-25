import { useTranslation } from "react-i18next";
import { useSettingsStore } from "../lib/settings/store";
import type { DebugCategories } from "../lib/settings/types";

const DEBUG_CATEGORY_ROWS = [
  ["windows", "debugWindows"],
  ["pointsCredit", "debugPointsCredit"],
  ["pointsClaim", "debugPointsClaim"],
  ["rewards", "debugRewards"],
  ["polls", "debugPolls"],
  ["raids", "debugRaids"],
] as const satisfies readonly [keyof DebugCategories, string][];

export function DebugOutputSettings() {
  const { t } = useTranslation(["settings"]);
  const settings = useSettingsStore((state) => state.settings);
  const setSettings = useSettingsStore((state) => state.setSettings);

  if (!settings.gui.debugMode) return null;

  const setCategory = (key: keyof DebugCategories, enabled: boolean) => {
    setSettings({
      gui: {
        ...settings.gui,
        debugCategories: {
          ...settings.gui.debugCategories,
          [key]: enabled,
        },
      },
    });
  };

  return (
    <>
      <div className="settings__row">
        <div className="settings__label">
          <span>{t("settings:debugOutput")}</span>
          <small className="muted">{t("settings:debugOutputHint")}</small>
        </div>
      </div>
      {DEBUG_CATEGORY_ROWS.map(([key, label]) => (
        <label className="settings__row settings__row--check" key={key}>
          <input
            type="checkbox"
            checked={settings.gui.debugCategories[key]}
            onChange={(event) => setCategory(key, event.target.checked)}
          />
          <span className="settings__check-text">{t(`settings:${label}`)}</span>
        </label>
      ))}
    </>
  );
}
