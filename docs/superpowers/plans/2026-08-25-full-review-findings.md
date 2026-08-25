# Full Review Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close every confirmed finding from the 2026-08-25 full repository review without broad refactors.

**Architecture:** Keep the current Tauri/React and Rust streaming architecture. Fix each root cause at its ownership boundary: CI workflow evaluation, Twitch EventSub session lifecycle, per-process Chatterino ownership, updater/HUD frontend lifecycle, Tauri overlay ACLs, Streamlink secret transport, and bounded diagnostics.

**Tech Stack:** GitHub Actions, Rust/Tauri 2, Tokio, React/TypeScript/Vitest, Windows process APIs.

**Spec:** Current `main` review findings at `7e57a906ba1415e106b61b60f7e813e3313cf06d`.

## Global Constraints

- Windows-only desktop behavior must remain intact.
- No shell construction for process launch.
- Do not expose Twitch tokens to the frontend.
- Keep updater signing and Streamlink SHA verification unchanged.
- Keep release and debug Rillmux concurrently runnable.
- All production changes require regression coverage.

---

### Task 1: CI security gates

**Files:** `.github/workflows/codeql.yml`, `.github/workflows/ci.yml`, `scripts/review-findings.test.ts`

- [ ] Add a failing regression test for job-level `matrix` use and missing explicit shard rustfmt.
- [ ] Verify RED in PR CI.
- [ ] Build the CodeQL matrix from `needs.changes` output instead of referencing `matrix` in job-level `if`.
- [ ] Explicitly `rustfmt --check` all `include!` streaming shards.
- [ ] Verify frontend tests and CI workflow success.

### Task 2: EventSub lifecycle

**Files:** `src-tauri/src/eventsub.rs`, `src-tauri/tests/eventsub_lifecycle_contract.rs`

- [ ] Add failing contracts for `reconnect_url`, `keepalive_timeout_seconds`, reconnect handoff and auth-rejected subscription retry.
- [ ] Parse Twitch session reconnect metadata.
- [ ] Connect the replacement websocket before closing the old socket and retain subscriptions across the handoff.
- [ ] Enforce the server-provided keepalive deadline.
- [ ] Bubble 401/403 subscription creation failures so the supervisor reacquires auth.
- [ ] Verify focused and full Rust tests.

### Task 3: Chatterino instance ownership

**Files:** `src-tauri/src/streaming/foundation.rs`, `src-tauri/src/streaming/windows_layout.rs`, `src-tauri/src/streaming/tools_process.rs`, `src-tauri/tests/chatterino_instance_ownership_contract.rs`

- [ ] Add failing ownership contracts.
- [ ] Generate a per-Rillmux-process dock owner marker.
- [ ] Tag Chatterino with the exact owner marker and only discover/close matching processes.
- [ ] Isolate debug and release Chatterino dock profiles.
- [ ] Verify existing Chatterino contracts plus full Rust tests.

### Task 4: Frontend lifecycle regressions

**Files:** `src/components/UpdateBanner.tsx`, `src/components/ChannelPointsHudSync.tsx`, `scripts/review-findings.test.ts`

- [ ] Require a 60-minute updater interval and disabled-HUD short circuit.
- [ ] Restore hourly update checks while preventing overlapping checks.
- [ ] Skip website-auth/keyring reads entirely while the HUD is disabled.
- [ ] Verify frontend tests and build.

### Task 5: Overlay privilege boundary

**Files:** `src-tauri/src/lib.rs`, `src-tauri/build.rs`, `src-tauri/capabilities/*.json`, `src/components/ChannelPointsHud.tsx`, `src/components/ChannelPointsHudSync.tsx`, `scripts/tauri-capabilities.test.ts`, `src-tauri/tests/overlay_command_scope_contract.rs`

- [ ] Add failing capability/command contracts.
- [ ] Make overlay self-placement target the injected calling `WebviewWindow` only.
- [ ] Give main a channel-derived HUD placement command instead of arbitrary labels.
- [ ] Split raid/poll/points overlay capabilities to least privilege.
- [ ] Verify capability tests and Rust tests.

### Task 6: Streamlink OAuth secret transport

**Files:** `src-tauri/src/twitch_web_auth.rs`, `src-tauri/src/streaming/runtime.rs`, `src-tauri/tests/streamlink_auth_injection_contract.rs`

- [ ] Flip the existing contract so command-line token injection is forbidden.
- [ ] Write the token to a randomized ephemeral Streamlink config and pass only its path via `--config`.
- [ ] Keep the file alive until Streamlink has parsed startup arguments, then remove it during session setup/cleanup.
- [ ] Verify user default/plugin configs remain loadable and full Rust tests pass.

### Task 7: Bounded diagnostics

**Files:** `src-tauri/src/diagnostics.rs`, `scripts/review-findings.test.ts`

- [ ] Add a failing rotation contract.
- [ ] Rotate/truncate `rillmux.log` at a bounded size before append.
- [ ] Verify diagnostics tests and full Rust tests.

### Task 8: Final verification

- [ ] Review exact changed-file set and PR diff.
- [ ] Verify frontend test/build/audit, Rust fmt/shard-rustfmt/clippy/check/test/audit, CodeQL/GHAS.
- [ ] Remove temporary implementation-only files if any.
- [ ] Mark PR ready only after exact-head checks are green.
