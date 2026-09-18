---
name: verify-rillmux
description: >
  Launch, health-check, and drive Rillmux through a persistent control surface:
  Vite+CDP for React chrome, and npm run tauri:dev + WebView2 CDP for native
  setup checks. Capture head-bound receipts. Do not use for unit tests, GitHub
  release, or rewriting product code.
---

# Verify Rillmux

Windows-only Tauri + React Twitch/Streamlink client. Classification: **EXTEND**. The web lane (isolated Vite + Chrome/Edge CDP) remains the default `launch` path. A native lane was added behind the same helper for behaviors that require Tauri commands.

Isolation: **serial** per lane. Web uses a unique Vite port and browser profile. Native uses `npm run tauri:dev` (port **1420**), isolated `APPDATA`, and debug `webview-dev`. Do not start a second driver against the same lane. Do not steal a foreign process on 1420.

## Launch

Web (unchanged default):

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs launch
```

Vite on a free 18700–18999 port plus Edge/Chrome CDP. `--preview` uses production `dist/`. Ready when snapshot `hydrated` and `appName` is `Rillmux`.

Native:

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs launch --lane native
```

Starts the same Tauri/Vite pipeline as `npm run tauri:dev` via `npx tauri dev --config` with a verifier-owned overlay that only adds WebView2 `additionalBrowserArgs` for CDP. `APPDATA` points at `.agents/skills/verify-rillmux/.runtime/native-appdata-<runId>/` (seeds `onboardingDone`). Ready when the WebView shell is hydrated and the Vite **Desktop shell required** banner is absent.

`--dry-run` prints commands without starting processes. Native launch fails with an actionable error if cargo/rustc/Tauri CLI are missing or port 1420 is occupied by a foreign process.

## Doctor

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs doctor --json
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs doctor --lane native --json
```

JSON always includes `lanes.web` and `lanes.native`. Web `ready` is Vite+CDP chrome. Native reports `canLaunch`, tool probes (cargo, rustc, Tauri CLI, Streamlink, mpv, Chatterino — paths/versions only), port 1420 occupancy, and `instance.ready` after native launch. OS credentials are **not** read (`auth.inspected: false`). Default exit code still follows the **web** lane unless `--lane native`.

Project static React scan: `npm run doctor`.

## Control surface

Mode: helper
Command: node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs
Helper: scripts/control-rillmux.mjs

Subcommands: `help`, `launch`, `doctor`, `inspect`, `goto --path`, `click --name`, `screenshot`, `drive <feature>`, `receipt`, `cleanup`, `status`.

`--lane web|native` selects the CDP target for inspect/goto/click/screenshot/cleanup. `drive native-setup-check` always uses the native WebView. No arbitrary eval backdoor.

## Drive

Web features: `about-changelog`, `followed-login-gate`, `settings-tabs`, `tauri-guard`, `browse-nav`. Changelog is an ARIA `role="dialog"` (not a native `<dialog>`). Snapshot waits use that handle.

Deterministic verifier tests: `node --test .agents/skills/verify-rillmux/scripts/web-snapshot.test.mjs .agents/skills/verify-rillmux/scripts/native-cdp.test.mjs .agents/skills/verify-rillmux/scripts/receipt-resources.test.mjs`. Receipt `started_resources` are run-scoped (native receipts must not inherit a prior web Vite row).

Native feature: `drive native-setup-check` — About → Setup check via Tauri `get_doctor_report` (found **or** missing Streamlink is success; that is the native invoke). Do not treat Vite chrome as proof of this command.

Twitch login, website auth, Streamlink/mpv playback, Chatterino dock, Channel Points HUD, raids: credential- or binary-dependent; report `blocked` with the missing prerequisite.

Native features beyond the setup check: `drive mpv-volume-boost` (Settings Player volume booster: options, summary text, Tauri store round trip, control left back at Off) and `drive stream-card-game-line` (`/streams` card meta lines against live Twitch data).

The native lane's APPDATA override does **not** redirect the settings store: `plugin-store` resolves `%APPDATA%\com.wibias.rillmux\settings.json` through the shell path, outside the isolated tree. `drive mpv-volume-boost` snapshots and restores that file around the drive so a run leaves the real profile unchanged.

## Evidence

Receipts under `.agents/skills/verify-rillmux/evidence/`. Bind `head_sha` to full `git rev-parse HEAD`. Native receipts use `surface: desktop`.

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs receipt --feature native-setup-check --result pass
```

Validate with `validate-verification-skill.mjs` and `--receipt`.

## Cleanup

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs cleanup
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs cleanup --lane native
```

Kills only recorded PIDs (`taskkill /PID /T`). Does not kill by process name. Keeps `evidence/`.

## Feature map

See [features/README.md](features/README.md).
