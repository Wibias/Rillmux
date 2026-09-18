# tauri-guard

- **Intent:** Opening the Vite UI in a browser must not look like a working desktop session.
- **Entry:** Any chrome route; `src/components/TauriGuardBanner.tsx` when `isTauri()` is false.
- **Lane:** web-only assertion (banner present). Native must not show this banner (`native-setup-check`).
- **Drive:** `node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs drive tauri-guard`
- **Success:** `role=alert` includes `Desktop shell required` and mentions `npm run tauri:dev`.
- **Other states:** Banner must be absent in the real Tauri window. Do not change product copy to make Vite look authenticated.
