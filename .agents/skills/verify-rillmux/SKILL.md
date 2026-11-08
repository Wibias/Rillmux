---
name: verify-rillmux
description: >
  Launch, health-check, and drive Rillmux through a persistent control surface:
  Vite+CDP for React chrome, and fail-closed npm run tauri:dev + WebView2 CDP
  for native checks after verified Windows user/VM credential isolation.
  Capture head-bound receipts. Do not use for unit tests, GitHub release, or
  rewriting product code.
---

# Verify Rillmux

Windows-only Tauri + React Twitch/Streamlink client. Classification: **EXTEND**. The web lane (isolated Vite + Chrome/Edge CDP) remains the default `launch` path. A native lane was added behind the same helper for behaviors that require Tauri commands.

Isolation: **serial** per lane. Web uses a unique Vite port and browser profile. Native uses `npm run tauri:dev` (port **1420**), a verified separate Windows user/VM credential boundary, isolated `APPDATA`, and debug `webview-dev`. Do not start a second driver against the same lane. Do not steal a foreign process on 1420.

## Launch

Web (unchanged default):

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs launch
```

Vite on a free 18700–18999 port plus Edge/Chrome CDP. `--preview` uses production `dist/`. Ready when snapshot `hydrated` and `appName` is `Rillmux`.

Native:

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs preflight --json
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs launch --lane native
```

Native launch is **fail-closed**. It starts only when `preflight` proves a distinct Windows credential boundary through the current Windows user SID plus a protected, machine-scope HKLM operator attestation. A separate Git worktree, `APPDATA`, environment variables, or a renamed credential service is not proof. The default production Windows account therefore reports `BLOCKED` before any native process is created.

After a verified preflight, the lane starts the same Tauri/Vite pipeline as `npm run tauri:dev` via `npx tauri dev --config` with a verifier-owned overlay that only adds WebView2 `additionalBrowserArgs` for CDP. `APPDATA` points at `.agents/skills/verify-rillmux/.runtime/native-appdata-<runId>/` (workspace-file isolation only). Ready when the WebView shell is hydrated and the Vite **Desktop shell required** banner is absent.

`--dry-run` prints commands without starting processes. Native launch fails with an actionable error if cargo/rustc/Tauri CLI are missing or port 1420 is occupied by a foreign process.

The complete credential inventory, trusted-attestation contract, operator provisioning, and isolated retest procedure are in [references/native-isolation.md](references/native-isolation.md). The verifier never creates Windows accounts or edits the isolation registry record.

## Doctor

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs doctor --json
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs doctor --lane native --json
```

JSON always includes `lanes.web` and `lanes.native`. Web `ready` is Vite+CDP chrome. Native reports `canLaunch`, tool probes (cargo, rustc, Tauri CLI, Streamlink, mpv, Chatterino — paths/versions only), port 1420 occupancy, and `instance.ready` after native launch. OS credentials are **not** read (`auth.inspected: false`). Default exit code still follows the **web** lane unless `--lane native`.

Each native report also includes `nativeIsolationVerified`, `nativeIsolation.status`, the hashed current/production/isolated SIDs, and evidence fields. `canLaunch` is false whenever isolation is BLOCKED. The Doctor itself never reads the credential store; `auth.inspected: false` is not evidence about a launched app. Doctor remains diagnostic: consumers must require `lanes.native.ready === true`, not interpret Exit 0 alone as native readiness.

Project static React scan: `npm run doctor`.

## Control surface

Mode: helper
Command: node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs
Helper: scripts/control-rillmux.mjs

Subcommands: `help`, `preflight`, `launch`, `doctor`, `inspect`, `goto --path`, `click --name`, `screenshot`, `drive <feature>`, `receipt`, `cleanup`, `status`.

`--lane web|native` selects the CDP target for inspect/goto/click/screenshot/cleanup. `drive native-setup-check` always uses the native WebView. No arbitrary eval backdoor.

## Drive

Web features: `about-changelog`, `followed-login-gate`, `settings-tabs`, `tauri-guard`, `browse-nav`. Changelog is an ARIA `role="dialog"` (not a native `<dialog>`). Snapshot waits use that handle.

Deterministic verifier tests:

```text
node --test .agents/skills/verify-rillmux/scripts/control-rillmux.test.mjs .agents/skills/verify-rillmux/scripts/native-isolation.test.mjs .agents/skills/verify-rillmux/scripts/web-snapshot.test.mjs .agents/skills/verify-rillmux/scripts/native-cdp.test.mjs .agents/skills/verify-rillmux/scripts/receipt-resources.test.mjs
```

Receipt `started_resources` are run-scoped (native receipts must not inherit a prior web Vite row). Native drive features re-check the same isolation preflight and fail before touching an existing native app.

Native feature: `drive native-setup-check` — About → Setup check via Tauri `get_doctor_report` (found **or** missing Streamlink is success; that is the native invoke). Do not treat Vite chrome as proof of this command.

Twitch login, website auth, Streamlink/mpv playback, Chatterino dock, Channel Points HUD, raids: credential- or binary-dependent; report `blocked` with the missing prerequisite.

Native features beyond the setup check: `drive mpv-volume-boost` (Settings Player volume booster: options, summary text, Tauri store round trip, control left back at Off) and `drive stream-card-game-line` (`/streams` card meta lines against live Twitch data).

Do not infer settings isolation from the workspace tree. `plugin-store` and Tauri app-data resolution must be inspected in the isolated Windows profile during retest. Native drives requiring persistence are permitted only after the isolation preflight is VERIFIED and must restore any verifier-owned state.

## Evidence

Receipts under `.agents/skills/verify-rillmux/evidence/`. Bind `head_sha` to full `git rev-parse HEAD`. Native receipts use `surface: desktop`.

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs receipt --feature native-setup-check --result pass
```

Validate with `validate-verification-skill.mjs` and `--receipt`.

## Credential safety

The app's core OAuth, playback OAuth, and Channel Points TV OAuth can each read, refresh, migrate, save, or delete entries in the current Windows user's Credential Manager. Core OAuth is requested unconditionally during app startup. Do not launch native merely to see whether it is signed out. Use an operator-provisioned separate standard Windows user or isolated Windows VM, and keep the status BLOCKED until `preflight` is VERIFIED.

## Cleanup

```text
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs cleanup
node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs cleanup --lane native
```

Kills only recorded PIDs (`taskkill /PID /T`). Does not kill by process name. Keeps `evidence/`.

## Feature map

See [features/README.md](features/README.md).
