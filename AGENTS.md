## Learned User Preferences

- Keep the title bar at 38px; do not shrink it back to 32px when changing window size or other chrome.
- Size requests about the "frame" or "top part" mean the title bar (caption buttons and website-auth / bonus-claims chips) unless they explicitly say the whole window.
- The bonus-claims compact chip must only toggle a panel, matching website auth — never disconnect or start device login on the chip click.
- Version releases should bump `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` together, update `CHANGELOG.md`, and open a PR; ship completed feature work as PRs when asked.
- Manual Actions → Release → Run workflow on `main` should create tag `v{version}` and publish a GitHub Release, not artifacts only.
- Twitch login should recover when the network returns without requiring a full quit and restart.
- The in-app updater should check on launch and again every 60 minutes while the app is running.
- Followed stream cards must show the stream name plus two lines of text under it; if a grid row cannot fit that, do not display the row.
- Dock dividers should only stack above streams and chat, not other programs; do not hide dividers or sidebars while a Chatterino usercard is open — keep grips visible and raise the usercard above them. Do not relocate the user's always-on multi-chat Chatterino window when docking a per-stream chat. Docked per-stream chat must follow the stream across monitors; the move-monitor control stays in its usual place but stacks above chat, not under it.
- Do not name the app data folder after the package identifier (`com.wibias.rillmux`); use a human product name.
- A `tauri:dev` / debug instance should be allowed to run while the installed release is already open; closing the debug app or its console must clear leftover Windows taskbar-overflow / tray icons.
- Channel Points on streams is an opt-in HUD on each mpv player (not the title bar, not desktop-topmost, not the Rillmux chrome): a dim, hover-visible chip that can be dragged and remembers a stream-relative position, with a reset-to-default control (stream top edge, left of caption buttons); it must not sit under caption buttons or be re-parked there when returning to the stream; it must follow the stream across monitors; a click opens the rewards catalog for redeem; timed +50 stays automatic with no claim button.

## Learned Workspace Facts

- Rillmux is a Windows-only Tauri + React Twitch/Streamlink desktop client (repo `Wibias/Rillmux`, identifier `com.wibias.rillmux`).
- The main window is frameless; caption buttons come from `tauri-plugin-window-controls` (`#tbo-controls`). Production CSP is `style-src 'self'`, so overlay caption styles must live in bundled CSS; the plugin injects `--title-bar-height: 32px` on `:root` and 38px must be pinned on `body`, `.shell__titlebar`, and `#tbo-controls`.
- Default window size is 1280×880 (minimum 1024×700).
- App version must stay in sync across `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.
- The Release workflow publishes on `v*` tag push or `workflow_dispatch` from `main`/`master`; dispatch from other branches builds artifacts only.
- Twitch `MakePrediction` payload field is `prediction` (the user's bet, with nested `event`), not `predictionEvent`.
- Hermes prediction topics should clear the overlay on `event-completed` / cancel types and on `event-updated` with RESOLVED or CANCELED statuses.
- Website auth and bonus claims are separate Twitch sessions; bonus claims uses a dedicated device-code TV session for +50 claims. Timed +50 auto-claims when presence and claim auth are connected; the stream HUD is display plus catalog redeem only.
- Security policy lives at `.github/SECURITY.md`; CI and Release workflows pin Actions to full-length commit SHAs and CI uses `contents: read`.
- Installed builds check GitHub `latest.json` a few seconds after launch and every 60 minutes; `tauri:dev` / unsigned Vite windows do not show the update dialog.
- Dock seam grips should sit above streams and chat only; Chatterino usercards and other apps stack above grips via z-order, not by suppressing dividers. Per-stream docked chat uses Chatterino7 (7TV), not stock Chatterino 2, and is a separate window from the user's always-on multi-chat Chatterino. The Channel Points HUD stacks above its mpv player only, not as a desktop-topmost window or over the Rillmux chrome.
- Cursor user-level FFF MCP starts with cwd in the home folder and refuses to index home/root; pass an explicit project path (currently Rillmux) in `~/.cursor/mcp.json`.
