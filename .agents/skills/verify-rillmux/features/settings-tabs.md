# settings-tabs

- **Intent:** Switch Settings sections (Interface, Streaming, Player, …).
- **Entry:** `/settings`, nav Settings. Tabs in `src/pages/SettingsPage.tsx`, labels in `src/locales/en/settings.json`.
- **Lane:** web-drivable for tab chrome. Persistence is native (Tauri store).
- **Drive:** `node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs drive settings-tabs`
- **Success:** Player tab `aria-selected=true` and h2 Player.
- **Other states:** Theme/player path persistence requires Tauri store (blocked on Vite). Debug output filters appear on General only.
