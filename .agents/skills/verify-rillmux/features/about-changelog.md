# about-changelog

- **Intent:** Confirm the running build version and open the in-app changelog.
- **Entry:** `/about`, button View changelog (`src/pages/BrowsePages.tsx` AboutPage, `ChangelogDialog`).
- **Lane:** web-drivable and native-drivable. Updater install is native/signed-build.
- **Drive:** `node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs drive about-changelog`
- **Success:** h1 About, then open ARIA dialog labelled Changelog (`role="dialog"`, `routes:changelogTitle`).
- **Other states:** Check for updates is a no-op or error outside signed Tauri; DoctorPanel tool checks need the desktop shell.
