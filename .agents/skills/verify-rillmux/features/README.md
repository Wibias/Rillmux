# Rillmux feature map

Control: `node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs`

Lanes: **web** = isolated Vite + Chrome/Edge CDP. **native** = `npm run tauri:dev` + WebView2 CDP. Do not prove native-only behavior on the web lane.

| Feature | Web | Native | Credentials | External binaries | Drive |
| --- | --- | --- | --- | --- | --- |
| [followed-login-gate](followed-login-gate.md) | yes | yes (same copy) | no for empty state | no | `drive followed-login-gate` |
| [browse-nav](browse-nav.md) | yes | yes | no | no | `drive browse-nav` |
| [settings-tabs](settings-tabs.md) | yes (in-memory) | persist needs store | no | no | `drive settings-tabs` |
| [about-changelog](about-changelog.md) | yes | yes | no | no | `drive about-changelog` |
| [tauri-guard](tauri-guard.md) | yes (banner present) | inverse (banner absent) | no | no | `drive tauri-guard` (web only) |
| [native-setup-check](native-setup-check.md) | no | yes | no | Streamlink/mpv/Chatterino optional (status still shown) | `drive native-setup-check` |
 | [mpv-volume-boost](mpv-volume-boost.md) | control only | yes (persist) | no | no | `drive mpv-volume-boost` |
| [stream-card-game-line](stream-card-game-line.md) | yes (same copy) | yes | no for `/streams` | no | `drive stream-card-game-line` |
| Twitch device login | no | yes | **yes** | no | blocked without user OAuth |
| Website auth / Streamlink start | no | yes | **yes** | Streamlink + player | blocked without token + binaries |
| Chatterino dock | no | yes | no | Chatterino7 | blocked if exe missing |
| Channel Points HUD | no | yes | **yes** | mpv + website auth | blocked |
| Raid overlay | no | yes | **yes** | live raid | blocked |

Current-environment blockers are recorded by `doctor --lane native --json` (`blockedReasons`, `tools.*.found`).
