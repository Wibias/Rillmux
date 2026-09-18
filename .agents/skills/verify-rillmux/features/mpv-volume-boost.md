# mpv-volume-boost

- **Intent:** Confirm the opt-in mpv volume booster exists on the Player tab, offers only the documented levels, reports itself in the presets summary, and survives a store round trip.
- **Entry:** `/settings` -> Player tab -> **Volume boost** select.
- **Source:** `src/lib/settings/mpv.ts` (`MPV_VOLUME_BOOSTS`, `normalizeMpvVolumeBoost`), `src/pages/SettingsTabs.tsx`, `src/lib/settings/store.ts`.
- **Lane:** native-only. Persistence goes through the Tauri store, so Vite chrome can only prove the control, not the round trip.
- **Credentials:** none.
- **Binaries:** none. No stream is started; mpv is not launched.
- **Drive:** `node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs drive mpv-volume-boost` after `launch --lane native`.
- **Success:** `Volume boost` select present with options `100,130,150,200,300`, default `100` labelled "Off (100%)", setting 200 updates the presets summary to `volume boost 200%`, and the value is still `200` after a WebView reload.
- **Other states:** the composed `--volume-max`/`--volume` player args and the attach-time IPC volume are not observable here; they need a real stream start (Twitch token + Streamlink + mpv) and are covered by unit tests instead.
