import { useLocation, useNavigate } from "react-router";
import { useTranslation } from "react-i18next";
import {
  SETTINGS_TABS,
  settingsTabFromPath,
  settingsTabLabelKey,
} from "../lib/settings/tabs";
import {
  SettingsChannelsPanel,
  SettingsChatPanel,
  SettingsGeneralPanel,
  SettingsHotkeysPanel,
  SettingsInterfacePanel,
  SettingsNotificationsPanel,
  SettingsPlayerPanel,
  SettingsStreamingLayoutPanel,
  SettingsStreamingPlaybackPanel,
} from "./SettingsTabs";
import "./SettingsPage.css";
import "../components/SetupHelp.css";

export function SettingsPage() {
  const { t } = useTranslation(["routes", "settings", "common"]);
  const location = useLocation();
  const navigate = useNavigate();
  const activeTab = settingsTabFromPath(location.pathname);

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
              className={
                activeTab === tab ? "settings__tab is-active" : "settings__tab"
              }
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

          {activeTab === "interface" ? <SettingsInterfacePanel /> : null}
          {activeTab === "streaming" ? (
            <div
              className="settings__group"
              role="tabpanel"
              id="settings-panel"
              aria-labelledby="settings-panel-title"
            >
              <SettingsStreamingPlaybackPanel />
              <SettingsStreamingLayoutPanel />
            </div>
          ) : null}
          {activeTab === "player" ? <SettingsPlayerPanel /> : null}
          {activeTab === "chat" ? <SettingsChatPanel /> : null}
          {activeTab === "notifications" ? (
            <SettingsNotificationsPanel />
          ) : null}
          {activeTab === "hotkeys" ? <SettingsHotkeysPanel /> : null}
          {activeTab === "channels" ? <SettingsChannelsPanel /> : null}
          {activeTab === "general" ? <SettingsGeneralPanel /> : null}
        </div>
      </div>
    </section>
  );
}
