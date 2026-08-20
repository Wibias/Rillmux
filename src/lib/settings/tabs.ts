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
