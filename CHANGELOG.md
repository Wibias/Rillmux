# Changelog

All notable changes to this Tauri rewrite are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned

- Optional Authenticode-signed installers once a Windows code-signing certificate is available in CI
- Further parity and polish as we dogfood releases

## [0.5.6] — 2026-08-26

### Fixed

- Moving a linked dock to another monitor no longer leaves the monitor picker/divider on the previous display or temporarily makes it unresponsive while mpv and Chatterino are being retiled
- Opening a stream after a Channel Points Prediction has already started now hydrates the current active or locked Prediction immediately, so the overlay appears without waiting for a new realtime Prediction event

## [0.5.5] — 2026-08-26

### Added

- Explicit **Independent**, **Seamless**, and **Multistream** stream-opening modes, so multiple standalone player windows can run at the same time without automatically joining the coordinated multistream layout
- Settings migration that preserves the behavior of existing `seamlessSwitch` configurations while moving stream-opening behavior to the explicit mode model

### Changed

- Independent streams no longer participate in multistream slot capacity, shared tiling, or linked-dock layout behavior merely because more than one stream is open
- Ordinary native layout refreshes only synchronize chat width when the persisted value actually differs, leaving normal player/Chatterino placement to the existing delayed latest-generation retile path

### Fixed

- Opening a second stream no longer re-enters the interactive chat-width setter and synchronously blocks the Rillmux UI while Chatterino is restarting; explicit chat-width changes also avoid a duplicate full native layout apply
- Channel Points HUDs keep their last valid player geometry and open reward catalogs across transient player-HWND lookup gaps during stream/layout transitions
- Adding or removing another running stream no longer tears down every existing Channel Points HUD as a side effect of the HUD synchronization effect restarting
- Existing Channel Points HUDs use a bounded grace period for temporary placement misses while their stream remains running, while genuine stream removal still closes only the corresponding HUD
- Temporary website-auth status lookup failures no longer get treated as logout and destroy otherwise healthy Channel Points HUDs

## [0.5.4] — 2026-08-26

### Added

- Scheduled cleanup for completed GitHub Actions histories whose local workflow files and live branch copies no longer exist, with ref revalidation before destructive cleanup
- Release-like CI contracts for frontend/Sentry bundle splitting and warning-free release-profile Rust builds

### Changed

- Browse, settings, multistream, auth, and supporting frontend code are split into route and stable vendor chunks instead of loading the former monolithic application bundle up front
- Sentry's React SDK is loaded only when telemetry is configured and enabled, keeping the SDK out of the normal initial frontend graph while preserving crash capture through a local error boundary
- Release CI treats Rust warnings as errors and verifies the release profile before packaging; stale NSIS/MSI JSON upload globs were removed while the updater manifest remains intact

### Fixed

- Successful Prediction votes now show an explicit confirmation with the selected stake and immediately disable further voting for that prediction
- Confirmed Prediction participation is synchronized between the host and overlay windows so delayed Twitch snapshots cannot re-enable controls and allow an accidental duplicate point spend
- Poll/prediction overlays now use a bounded ready/acknowledgement handshake and remain hidden until real state is available, preventing the blank gray startup window
- Prediction participation remains visible when a later Twitch snapshot temporarily omits the user's selected outcome or stake
- The poll/prediction dismiss action now says **Close** instead of **Hide**

## [0.5.3] — 2026-08-26

### Added

- Optional Channel Points HUD on each mpv window with live balance, drag/reset positioning, and the custom reward catalog/redemption flow
- Category-filtered Debug Mode diagnostics for stream/windows, watch credit, +50 claims, rewards, polls/predictions, and raids/EventSub in a bounded rotating `rillmux.log`
- Local crash files (panic text + Windows minidumps) under `%APPDATA%\Rillmux\crashes`, plus optional Sentry when enabled
- Current Channel Points private-GQL operation registry and compatibility fallbacks for balance, rewards, claims, polls, and predictions
- Path-aware CI and advanced CodeQL jobs so frontend/Rust scans only run when their relevant paths change

### Changed

- First-load splash uses the app icon, a larger wordmark, and a short fade into the shell
- Title bar is 38px tall, including website-auth / bonus-claims state and caption controls
- Website playback auth stays in Windows Credential Manager and is supplied to Streamlink through a randomized ephemeral launch config instead of persisting a managed token in `config.twitch`
- Docked Chatterino runs in an isolated Rillmux-owned profile and uses exact ownership/main-window detection so unrelated user Chatterino windows stay untouched
- Twitch private GraphQL operation names, hashes, fallbacks, and auth ownership are centralized instead of being scattered through the Channel Points runtime
- Update checks run shortly after startup and hourly while the app remains open
- The native streaming runtime was decomposed into focused shards while keeping its public module API unchanged
- Refreshed frontend, Rust, and GitHub Actions dependencies and kept RustSec/CodeQL gates in the release path

### Fixed

- Followed grid hides a final stream row unless the window is tall enough for the full card text instead of clipping it against pagination
- Debug `tauri:dev` no longer stacks leftover tray icons, can run next to an installed Rillmux, and X actually quits instead of leaving a hidden process
- Linked dock no longer freezes the Rillmux window while multistream grips retile mpv/Chatterino
- Regular dock dividers stay immediately above the dock group without becoming globally TOPMOST over unrelated applications
- Chatterino first-start no longer selects or leaves the blank white/black notebook over the real chat split; startup, restart, and current `Chatterino <version>` main-window selection are hardened
- Chatterino user cards, menus, settings, and unrelated windows are protected from dock duplicate cleanup
- Channel Points context uses current reward-capable query fallbacks again, restoring balance/reward data after Twitch query changes
- Passive Channel Points watch credit starts when a stream transitions to ready instead of remaining at zero presence targets
- +50 bonus claiming works again with the restored watch/presence lifecycle; successful claims are independently observable in Debug Mode
- Channel Points poll and prediction overlays recover when optional Hermes topic subscriptions are unavailable or rejected, with acknowledged subscriptions and a faster GQL safety-net refresh
- Channel Points HUD no longer flashes/jumps while opening the reward catalog and keeps dragged/reset positions clear of caption controls
- Channel Points HUD host logging reports meaningful found/lost transitions instead of repeating the same watchdog state several times per second
- Channel Points HUD ignores hidden or temporarily parked player HWNDs that do not intersect a real monitor, preventing the chip from following transient off-screen player coordinates while preserving legitimate negative multi-monitor coordinates
- Raid EventSub resyncs after settings hydration/toggles and has hardened reconnect URL, socket handoff, keepalive, and auth-session recovery
- Stored Twitch login and related Twitch HTTP entry points recover from transient offline/transport failures without requiring an app restart
- Deep links accept only the documented `stg://watch/<login>` / `stg://channel/<login>` forms and clean up late listener registration safely
- Release builds compile the configured Twitch client identity correctly, native Sentry follows persisted consent, and hard app exit tears down owned playback/chat processes
- Debug logs rotate at 10 MiB instead of growing without bound

### Security

- Split Tauri capabilities by main/raid/poll/points overlays and registered custom commands with the app manifest so privileged commands are not globally callable
- Reduced the dedicated bonus-claim Twitch TV session to the read-only `user_read` scope instead of unrelated account-write scopes
- Removed persistent Streamlink playback-token storage from the user config and tightened release identity, telemetry-consent, deep-link, and external-process boundaries
- Added automated Cargo/RustSec advisory coverage and hardened unsafe Windows version-info pointer validation

## [0.5.2] — 2026-08-22

### Fixed

- Stored Twitch login recovers after starting offline: validate retries when the network returns instead of staying on Login with Twitch until restart
- Prediction overlay uses MakePrediction's `prediction` field (not `predictionEvent`) and clears on completed, cancelled, or resolved events

## [0.5.1] — 2026-08-21

### Changed

- Title bar is 42px tall, with larger caption icons and website-auth / bonus-claims chips
- Default window height is 880px

### Fixed

- Minimize / maximize / close missing in release builds: production CSP dropped the overlay plugin's inline stylesheet while still hiding the HTML fallback
- Bonus claims chip opens a panel like website auth instead of disconnecting on the first click

## [0.5.0] — 2026-08-20

Covers browse, Channel Points, and the desktop shell since 0.4.1.

### Added

- Followed **list and grid** views, search (`Ctrl+K`), sort (viewers, uptime, name), hide mature, and pagination that fills the window
- **Pinned favourites** on Followed (pin from the stream menu)
- Stream **actions menu**: watch, pin, channel page, open on Twitch, copy URL
- Live thumbnails refresh every minute and cache-bust so WebView2 actually shows a new preview
- Channel Points presence on every ready Streamlink session, up to 8 streams
- Raid Follow/Stay prompt overlays mpv or Chatterino instead of the hidden app window
- Opt-in Channel Points **polls and predictions** over chat (live Hermes updates; vote or predict from the overlay)
- Settings split into sections: Interface, Streaming, Player, Chat, Notifications, Hotkeys, Channels, Other
- About **View changelog** (last five releases) plus a setup check for Streamlink, mpv, and Chatterino
- Category viewer counts; search category box art at 285×380 instead of Helix’s tiny 52×72 thumbs
- Device Code on login is copyable
- Player install help (winget / Scoop / download) for mpv, VLC, MPC-HC, and PotPlayer
- Frameless window with Windows 11 caption buttons (HTML controls if the overlay is unavailable)
- Short ledes under page titles on browse pages

### Changed

- Browse **Games** is now **Categories**
- Minimum window size is 1024×700
- Settings no longer shows a Streamlink executable picker under Streaming
- Bonus claims sit left of Website auth
- Language filter hides locale codes and makes Clear easier to see
- Followed, go-live notifications, and Multistream share one Helix followed-streams query (100 per page, refreshed every minute)
- Poll/prediction UI follows Hermes; GraphQL is a 60s fallback on the host window only
- Viewer presence updates from session events (30s status fallback); Channel Points balance refresh is throttled to 15s
- Idle watching uses less CPU: dock/session watchdogs sleep when the layout is stable, Watching no longer polls the session list every 4s, HLS playlist lines are ignored after playback starts, and stopping a stream no longer blocks other sessions while the process exits
- Poll overlay only repositions when chat actually moved

### Fixed

- Closing the last stream also tears down leftover dock grips and monitor-number overlays
- Multistream rows can be dragged to reorder
- About/setup checks no longer flash a console window
- Raid prompt stays visible after the source stream ends

## [0.4.1] — 2026-08-14

### Changed

- Update prompt is now a modal window: it shows the release notes (changelog) for the new version with **Download & install** / **Cancel**, and closes when you click outside it (Esc works too)

### Fixed

- Change-monitor handle no longer covers a stream tile's close control at the divider between streams and Chatterino in multistream layouts

## [0.4.0] — 2026-08-09

### Added

- **Twitch Website authentication** for authenticated playback (separate from Device Code Helix login)
- **Channel Points (experimental)**: optional viewer presence while watching; balance display; auto-claim watch/bonus rewards when Website auth is configured
- **Channel Points claim auth**: separate Twitch TV login used only for bonus claims when Website session cannot claim
- **Channel Points diagnostics** in the auth bar (Hermes presence, Spade telemetry, protocol status)
- **Multistream vertical layouts** (`1x2`, `1x3`, `1x4`, `8x1`) and drag-and-drop slot ordering fix
- Dependabot configuration for npm and Cargo

### Fixed

- Viewer presence sync after settings hydration and Twitch login
- 20-second Channel Points heartbeat cadence
- Spade telemetry headers and isolated transport (stale connection recovery)
- Authenticated Hermes WebSocket presence for Channel Points
- Pre-push CI hook works from GitHub Desktop (minimal PATH / Node discovery)
- Various Channel Points watch contract and Web-player alignment fixes

### Changed

- Settings schema adds `streaming.channelPoints` (default off, experimental)
- README release section version reference to 0.4.0

## [0.3.1] — 2026-08-01

### Fixed

- **Grey dock grips usable again**: temporarily TOPMOST while the dock or app is focused (re-asserted so mpv/Chatterino cannot bury them), and demoted only when another program is clearly foreground. A failed window title scan no longer drops the bars under the stream.

## [0.3.0] — 2026-08-01

### Added

- **Follow raids**: EventSub WebSocket watches for outgoing `channel.raid` on streams you’re watching; a 15s banner (Follow now / Stay) then switches that slot’s stream and chat to the raid target. Multistream: only the raiding slot moves. Toggle under Settings → Streaming (`followRaids`, default on).
- **Browse language filter**: multi-select broadcast languages on Top streams and category pages (Helix `language` params). Empty selection = all languages. Persisted as `streaming.streamLanguages`.
- **Teams search**: Browse → Teams looks up a Twitch team by name and opens the team page (live members + watch). Channel → team links unchanged.
- **Per-channel notification mute**: global “notify when followed go live” remains; mute individual channels from the channel page or Settings → Notifications (`mutedFollowed`).

### Notes

- Settings schema advanced through **12 → 14** (follow raids, stream languages, muted followed). Older settings migrate with safe defaults.

## [0.2.1] — 2026-08-01

### Added

- About page shows the running app **version** (Tauri `getVersion()` / package version in browser)

### Fixed

- **No audio / mpv speaker “!”**: JSON IPC was sending `mute: "no"` (truthy string → mute on); mute is now a real boolean, with `--mute=no` on the CLI
- **Linked dock minimize sync**: minimizing mpv (or Chatterino) also minimizes the grey grips and the rest of the group, and restore brings everyone back
- Dock window finder still resolves **minimized** mpv/Chatterino windows (iconic rects used to drop them from the group)
- Grey dock borders are **no longer always-on-top** over unrelated apps; they only raise while the dock group is focused (monitor-number overlays still go topmost briefly)

## [0.2.0] — 2026-07-31

### Added

- **Linked dock** (Windows): thin always-on-top grips to resize chat|video and multistream tiles live; center ◀ ▶ handle (or Ctrl+Shift+M) opens Windows-style monitor numbers to pick a display
- Multistream layouts **2+1** and **8×1**, plus **large-pane position** (left / right / top / bottom) for 2+1 and 3+1
- Per-stream **Mute / Unmute** via mpv IPC on the Watching list
- When a stream goes offline: branded loading art and OSD **“The streamer {channel} went offline”**, then the player closes after 5 seconds (manual Stop still closes immediately)
- **Refresh** button on Followed, Top streams, Top games, and Streams in this category

### Changed

- External chat target is **[Chatterino7](https://github.com/SevenTV/chatterino7)**: doctor/setup links, install commands, and docs recommend it for 7TV name paints, personal emotes, animated avatars, and 4× 7TV/FFZ images. Stock Chatterino 2 still works if installed
- Seamless off turns linked dock on (and the reverse); chat width is configurable when the dock reserves space for Chatterino7
- Docked mpv uses `--keep-open=yes` so the offline goodbye screen can show before quit

### Fixed

- Multistream tile grey bars move with the streams while resizing (no longer lag until mouse-up)
- Monitor move no longer relies on buggy drag-to-cycle; click the handle and pick a numbered screen
- Chatterino usercards are less likely to sit under the seam grips (temporary seam suppress while popups are focused)

## [0.1.1] — 2026-07-30

### Added

- Startup update check: a banner appears when a new release is available, with download progress; the NSIS installer opens (`basicUi`) and the app relaunches into the new version

### Fixed

- `streamlink:fetch` works on Node 22 + Windows (pipeline hang, bsdtar path parsing)
- Updater manifest uses GitHub's sanitized asset names (spaces → dots) so the download URL no longer 404s

## [0.1.0] — 2026-07-30

First public preview of the Windows rewrite (Tauri 2 + React + TypeScript). The classic NW.js + Ember app was removed; this release replaces it.

### Added

- Desktop shell with tray, single-instance, and `stg://` deep links
- Twitch Device Code login with OS keyring token storage
- Browse: followed streams, top streams, games, search, channel, teams
- Streamlink launch path (bundled / system / custom) with mpv-oriented defaults
- Watching sessions, Streamlink status text, and seamless dual-process channel switch
- **Fast stream start**: pre-launched idle mpv (window in ~0.4 s) attached to Streamlink's loopback HTTP server via IPC; player windows open already snapped to their dock tile
- **Branded loading screen** in the player window with phase-accurate OSD status (resolving, pre-roll ads, errors) instead of mpv's "Drop files" idle screen
- **Multistream page** (sidebar): channel search with followed channels ranked first, quick-add from live followed channels, layout picker with capacity indicator, drag & drop slot ordering, per-slot chat selection; all chats open as Chatterino tabs
- Embedded chat by default; Chatterino / browser options in settings
- Settings schema with import/export, hotkeys, per-channel overrides, notifications
- Boot splash to hide the WebView white flash while the UI loads
- First-run setup wizard (Streamlink → player → optional login) and install help when tools are missing
- Sentry wiring (opt-out) and GitHub Actions release pipeline (NSIS + MSI + updater signatures)
- Auto-generated GitHub release notes on `v*` tags (this file is the curated narrative)

### Changed

- Low latency and ad filtering are **opt-in** (defaults off)
- Default mpv args follow upstream wiki Recommendations (verified against current mpv manual): borderless, maximized, loop for Enter-reload, cache + `demuxer-max-back-bytes=1800M`
- mpv install link uses `https://mpv.io/installation/`
- Player settings: plain-language preset summary, **Reset to recommended**, and toggles for wiki mpv flags; clearer Windows install help (winget / Scoop / portable `.7z`)
- Multistream layout selection moved from Settings to the Multistream page

### Security

- Helix API proxied through Rust — the OAuth token never exists in webview JS
- Bundled Streamlink verified, deep links hardened, iframe sandbox + CSP
- Opt-in scrubbed crash reports, React error boundary, CI gates
- react-router-dom v7 → react-router v8.3.0 (GHSA-qwww-vcr4-c8h2)

### Fixed

- Retry player window retiling until every window is placed
- Partially filled layout presets shrink to the running channel count
- Chatterino closes within a second of stream end (process-handle wait + Streamlink EOF prune) instead of up to 40 s
- App window restores when the last stream ends (minimizeOnWatch)
- Stream lifecycle, dock args, device-flow polling, updater manifest

### Notes

- Windows only for this rewrite
- Chatty is intentionally not supported
- Unsigned installers may show a SmartScreen “Unknown publisher” warning until Authenticode is configured

[Unreleased]: https://github.com/Wibias/Rillmux/compare/v0.5.6...HEAD
[0.5.6]: https://github.com/Wibias/Rillmux/compare/v0.5.5...v0.5.6
[0.5.5]: https://github.com/Wibias/Rillmux/compare/v0.5.4...v0.5.5
[0.5.4]: https://github.com/Wibias/Rillmux/compare/v0.5.3...v0.5.4
[0.5.3]: https://github.com/Wibias/Rillmux/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/Wibias/Rillmux/releases/tag/v0.5.2
[0.5.1]: https://github.com/Wibias/Rillmux/releases/tag/v0.5.1
[0.5.0]: https://github.com/Wibias/Rillmux/releases/tag/v0.5.0
[0.4.1]: https://github.com/Wibias/streamlink-twitch-gui/releases/tag/v0.4.1
[0.4.0]: https://github.com/Wibias/streamlink-twitch-gui/releases/tag/v0.4.0
[0.3.1]: https://github.com/Wibias/streamlink-twitch-gui/releases/tag/v0.3.1
[0.3.0]: https://github.com/Wibias/streamlink-twitch-gui/releases/tag/v0.3.0
[0.2.1]: https://github.com/Wibias/streamlink-twitch-gui/releases/tag/v0.2.1
[0.2.0]: https://github.com/Wibias/streamlink-twitch-gui/releases/tag/v0.2.0
[0.1.1]: https://github.com/Wibias/streamlink-twitch-gui/releases/tag/v0.1.1
[0.1.0]: https://github.com/Wibias/streamlink-twitch-gui/releases/tag/v0.1.0