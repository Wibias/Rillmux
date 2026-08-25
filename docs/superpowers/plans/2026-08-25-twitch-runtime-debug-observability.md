# Twitch Runtime Debug Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add opt-in, category-filtered local runtime diagnostics that can reconstruct first-start Chatterino/window races and the complete Twitch Channel Points, polls/predictions, rewards, and raid flows.

**Architecture:** Keep `gui.debugMode` as the master switch, persist six category preferences, and sync those preferences into Rust. Rust owns a bounded non-blocking diagnostics queue with one writer and centralized redaction helpers. Frontend orchestration logs correlation events through one Tauri diagnostics command while native Twitch/window modules log directly through category-aware Rust helpers.

**Tech Stack:** React 19, TypeScript, Zustand, Vitest, Tauri 2, Rust, Tokio/std synchronization, Windows Win32 window handling.

**Spec:** `docs/superpowers/specs/2026-08-25-twitch-runtime-debug-observability-design.md`

## Global Constraints

- Existing `gui.debugMode` remains the master switch.
- All six category preferences default to `true` when absent and survive turning Debug Mode off.
- Diagnostics are local only; no remote telemetry is added.
- Producers must not block on log I/O.
- Queue overflow drops debug events and later reports a drop count.
- Never log OAuth tokens, cookies, complete auth headers, complete GQL bodies, reward input text, full claim ids, or full persisted-query hashes.
- Existing 10 MiB rotation and crash/minidump behaviour remain intact.
- Logging failure must never fail a stream, claim, reward action, poll vote/prediction, or raid transition.
- This PR adds observability only unless an instrumentation blocker requires a tiny correctness fix.

---

### Task 1: Persist and expose debug categories

**Files:**
- Modify: `src/lib/settings/types.ts`
- Modify: `src/lib/settings/store.ts`
- Modify: `src/lib/settings/store.test.ts`
- Modify: `src/pages/SettingsPage.tsx`
- Modify: `src/locales/en/settings.json`
- Modify: `src/locales/de/settings.json`

**Interfaces:**
- Produces: `DebugCategories` and `AppSettings.gui.debugCategories`.
- Produces defaults with keys `windows`, `pointsCredit`, `pointsClaim`, `rewards`, `polls`, `raids`.
- Later tasks consume the exact persisted object.

- [ ] **Step 1: Write the failing migration/default tests**

Add assertions that default settings expose all six category flags as `true`, legacy schema 19 settings without `debugCategories` migrate to all `true`, and explicit false values survive migration.

- [ ] **Step 2: Verify RED**

Run `npm test -- src/lib/settings/store.test.ts` and expect failures because `debugCategories` is not defined.

- [ ] **Step 3: Implement the settings model and migration**

Add:

```ts
export interface DebugCategories {
  windows: boolean;
  pointsCredit: boolean;
  pointsClaim: boolean;
  rewards: boolean;
  polls: boolean;
  raids: boolean;
}

export const defaultDebugCategories = (): DebugCategories => ({
  windows: true,
  pointsCredit: true,
  pointsClaim: true,
  rewards: true,
  polls: true,
  raids: true,
});
```

Add `debugCategories` under `gui`, bump `SETTINGS_SCHEMA_VERSION` from `19` to `20`, and merge each boolean from persisted input over defaults.

- [ ] **Step 4: Add General > Debug output controls**

Render the six checkboxes only when `settings.gui.debugMode` is true. Update each checkbox by replacing only the selected key in `settings.gui.debugCategories`. Keep `Open logs` unchanged.

- [ ] **Step 5: Add English/German labels**

Add concise labels and one hint explaining that filters only affect local debug output.

- [ ] **Step 6: Verify GREEN and commit**

Run `npm test -- src/lib/settings/store.test.ts` and `npm run build`. Commit as `feat: add debug output category settings`.

---

### Task 2: Add the bounded category-aware Rust diagnostics transport

**Files:**
- Modify: `src-tauri/src/diagnostics.rs`
- Modify: `src-tauri/src/lib.rs`
- Add/modify focused Rust tests in `src-tauri/src/diagnostics.rs`
- Modify: `src/pages/SettingsPage.tsx`

**Interfaces:**
- Produces Rust `DebugCategory` enum: `Windows`, `PointsCredit`, `PointsClaim`, `Rewards`, `Polls`, `Raids`.
- Produces `DebugCategoryFlags` serde payload with camelCase fields matching TypeScript.
- Produces `set_debug_categories(flags)` and `log_event(category, event, fields)`.
- Produces Tauri commands `diagnostics_set_debug_categories` and `diagnostics_log_event`.

- [ ] **Step 1: Write failing Rust tests**

Test category filtering, short-id/hash redaction, and non-blocking bounded enqueue/drop accounting using a small test queue capacity.

- [ ] **Step 2: Verify RED**

Run `cargo test --manifest-path src-tauri/Cargo.toml diagnostics` and expect missing APIs.

- [ ] **Step 3: Implement category flags and redaction helpers**

Use atomics for the six category flags. Add helpers that return short safe identifiers only, e.g. first 6 + last 4 characters for ids and first 8 characters for hashes.

- [ ] **Step 4: Implement bounded asynchronous queue**

Use `std::sync::mpsc::sync_channel` with `try_send`, a single lazily started writer thread, and an atomic dropped-event counter. The writer adds millisecond wall-clock timestamps, writes console + `rillmux.log`, preserves existing rotation, and emits a synthesized `[WARN][diagnostics] dropped.count=N` line before the next successfully written event after overflow.

- [ ] **Step 5: Preserve existing logging semantics**

Keep `log_line` as the always-write path used by important existing errors. Keep `log_debug` gated by the master switch. New `log_event` additionally checks its category before enqueue.

- [ ] **Step 6: Add Tauri commands and frontend synchronization**

Register `diagnostics_set_debug_categories` and `diagnostics_log_event` in `src-tauri/src/lib.rs`. In `SettingsBootstrap`, whenever hydrated main-window debug state changes, sync both master flag and category flags to Rust. Overlay webviews remain excluded.

- [ ] **Step 7: Verify GREEN and commit**

Run diagnostics Rust tests, `cargo fmt --check`, and frontend tests/build. Commit as `feat: add filtered async runtime diagnostics`.

---

### Task 3: Instrument stream, Chatterino, HWND, layout, and HUD lifecycle

**Files:**
- Modify: `src/lib/streaming/store.ts`
- Modify: `src-tauri/src/streaming/tools_process.rs`
- Modify: `src-tauri/src/streaming/windows_layout.rs`
- Modify: `src-tauri/src/streaming/overlays.rs`
- Modify: `src/components/ChannelPointsHud.tsx`
- Modify/add focused contract tests under `scripts/review-findings.test.ts` and existing streaming tests.

**Interfaces:**
- Consume `diagnostics_log_event` from frontend and `diagnostics::log_event(DebugCategory::Windows, ...)` natively.
- Correlation fields use lowercase channel and existing stream session id when known.

- [ ] **Step 1: Add failing lifecycle contract tests**

Assert that frontend stream start/status/Chatterino sync code emits windows-category events and that native files contain invalid-handle/geometry instrumentation at message/layout/HUD application points.

- [ ] **Step 2: Verify RED**

Run focused Vitest/source-contract tests; expect missing diagnostics calls.

- [ ] **Step 3: Instrument frontend orchestration**

Log `watch.start`, resolved non-secret launch metadata, `stream_start.return`, ready transitions, Chatterino sync skip/open/close/generation outcomes, layout schedule/apply requests, and stream stop requests/results.

- [ ] **Step 4: Instrument native window lifecycle**

At Chatterino window discovery/message/close and layout application, log HWND in hex, `IsWindow`, rect, operation, and Win32 error code on failures. Never add retries solely for debug.

- [ ] **Step 5: Instrument HUD geometry**

Log placement input, host/player rect, saved fractional offset, and final native overlay rect for each apply.

- [ ] **Step 6: Verify GREEN and commit**

Run focused tests plus frontend build and Rust fmt/check. Commit as `feat: trace stream and window lifecycle`.

---

### Task 4: Instrument Channel Points credit, +50 claims, and rewards

**Files:**
- Modify: `src/lib/streaming/store.ts`
- Modify: `src-tauri/src/channel_points_realtime.rs`
- Modify: `src-tauri/src/viewer_presence.rs`
- Modify: `src-tauri/src/channel_points.rs`
- Modify: `src-tauri/src/twitch_gql_operations.rs` only if a safe query-label helper belongs there.
- Modify/add Rust contract tests under `src-tauri/tests/` and source contracts under `scripts/review-findings.test.ts`.

**Interfaces:**
- Categories: `PointsCredit`, `PointsClaim`, `Rewards`.
- Full tokens/payloads are never passed into formatting calls.

- [ ] **Step 1: Add failing source/behaviour contracts**

Require logs for selected presence targets, Hermes lifecycle, minute-watched result, claim discovery/skip/attempt/result, ChannelPointsContext candidate prefix/result, reward count, redeem result, and balance snapshot/delta.

- [ ] **Step 2: Verify RED**

Run focused tests; expect instrumentation markers to be absent.

- [ ] **Step 3: Instrument frontend target selection**

Before `viewer_presence_sync`, log enabled state and selected channel/session target metadata without auth material.

- [ ] **Step 4: Instrument Hermes and minute-watched**

Log generation, connect/auth/subscription ACKs, ready/not-ready, reconnect reason, worker start/stop, telemetry attempt result/status, and compact error reason.

- [ ] **Step 5: Instrument +50 claims**

Log claim availability, redacted id, skip reason, auth path, attempt, HTTP/GQL result, and next observed balance where available.

- [ ] **Step 6: Instrument reward catalogue and redemption**

Log each persisted-query candidate by short prefix, candidate failure/success, reward collection parse path/count, redemption cost/input-present boolean, and compact mutation result. Never log input text.

- [ ] **Step 7: Verify GREEN and commit**

Run Channel Points Rust tests/contracts plus fmt/check and frontend tests. Commit as `feat: trace Channel Points runtime`.

---

### Task 5: Instrument polls/predictions and raids

**Files:**
- Modify: `src-tauri/src/channel_points_realtime.rs`
- Modify: `src-tauri/src/channel_points.rs`
- Modify: `src/lib/streaming/pollOverlay.ts` and/or owning poll overlay component if required by current flow.
- Modify: `src-tauri/src/eventsub.rs`
- Modify: `src/lib/streaming/store.ts`
- Modify: `src/components/RaidBanner.tsx`
- Modify/add focused tests for poll and raid lifecycle.

**Interfaces:**
- Categories: `Polls`, `Raids`.
- Reuse existing session/channel correlation and redaction helpers.

- [ ] **Step 1: Add failing poll/raid contracts**

Require subscription request/ACK/fallback, overlay lifecycle, action amount/result, EventSub sync/connect/reconnect, raid receipt/dedupe/countdown/follow/replacement session lifecycle.

- [ ] **Step 2: Verify RED**

Run focused tests; expect missing instrumentation.

- [ ] **Step 3: Instrument polls/predictions**

Log topic subscriptions and ACKs, fallback activation, discovered/updated/cleared state, overlay placement/open/close, redacted choice/outcome id, point amount, mutation result, and subsequent balance snapshot when present.

- [ ] **Step 4: Instrument raids/EventSub**

Log sync inputs, connection transitions, subscription creation, raid from/to/viewers, dedupe decision, overlay/countdown, user/automatic follow decision, old session stop, new session start, and final layout/HUD reconciliation.

- [ ] **Step 5: Verify GREEN and commit**

Run focused tests plus frontend/Rust checks. Commit as `feat: trace polls predictions and raids`.

---

### Task 6: Full verification, privacy review, and PR

**Files:**
- Review all changed files; only fix verification or privacy defects found.

**Interfaces:**
- No new API beyond Tasks 1-5.

- [ ] **Step 1: Secret-leak source review**

Search changed diagnostics calls for token/cookie/auth-header/raw-payload/reward-input values. Confirm only redacted ids/hash prefixes are emitted.

- [ ] **Step 2: Run frontend verification**

Run `npm test` and `npm run build`.

- [ ] **Step 3: Run Rust verification**

Run `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, `cargo check --manifest-path src-tauri/Cargo.toml`, and `cargo test --manifest-path src-tauri/Cargo.toml`.

- [ ] **Step 4: Run repository security/audit gates**

Run the same dependency audit/security steps configured by `.github/workflows/ci.yml` and let CodeQL run on the PR.

- [ ] **Step 5: Inspect final diff**

Confirm the branch contains only the design, implementation plan, diagnostics infrastructure, settings/UI changes, instrumentation, and tests.

- [ ] **Step 6: Open draft PR**

Open a PR from `feat/twitch-runtime-debug-observability` to `main` summarizing the six categories, async queue, privacy guarantees, and that behavioural bugs remain intentionally unfixed until the logs identify their root causes.

- [ ] **Step 7: Wait for CI/CodeQL in the current response and fix actionable findings**

Poll workflow runs, inspect failed job logs and PR review threads, fix valid findings, and leave the PR draft only if native authenticated Windows smoke remains the sole unverified item.
