# native-setup-check

- **Intent:** Confirm the desktop shell can probe Streamlink/mpv/Chatterino via Tauri (`get_doctor_report`), not the Vite fallback error.
- **Entry:** `/about` Setup check (`DoctorPanel`, Recheck).
- **Source:** `src/components/DoctorPanel.tsx`, `src-tauri/src/lib.rs` `get_doctor_report`, `src-tauri/src/doctor.rs`.
- **Lane:** native-only. Web shows `Desktop shell required` and must not be treated as a pass.
- **Credentials:** none.
- **Binaries:** optional. `Streamlink X found` or `Streamlink not found` both prove the command ran.
- **Drive:** `node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs drive native-setup-check` after `launch --lane native`.
- **Success:** h1 About, heading Setup check, Streamlink status line present, no Vite tauri-guard alert.
