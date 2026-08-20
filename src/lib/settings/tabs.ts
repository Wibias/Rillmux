export const SETTINGS_TABS = [
  "interface",
  "streaming",
  "player",
  "chat",
  "notifications",
  "hotkeys",
  "channels",
  "general",
] as const;

export type SettingsTab = (typeof SETTINGS_TABS)[number];

export function isSettingsTab(value: string): value is SettingsTab {
  return (SETTINGS_TABS as readonly string[]).includes(value);
}

export function settingsTabFromPath(pathname: string): SettingsTab {
  const parts = pathname.split("/").filter(Boolean);
  const last = parts[parts.length - 1] ?? "";
  return isSettingsTab(last) ? last : "interface";
}

export function settingsTabLabelKey(
  tab: SettingsTab,
): `tab${Capitalize<SettingsTab>}` {
  switch (tab) {
    case "interface":
      return "tabInterface";
    case "streaming":
      return "tabStreaming";
    case "player":
      return "tabPlayer";
    case "chat":
      return "tabChat";
    case "notifications":
      return "tabNotifications";
    case "hotkeys":
      return "tabHotkeys";
    case "channels":
      return "tabChannels";
    case "general":
      return "tabGeneral";
    default: {
      const _never: never = tab;
      return _never;
    }
  }
}
