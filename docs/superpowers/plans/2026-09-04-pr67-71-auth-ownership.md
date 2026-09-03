# PR67-71 Auth Ownership Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the remaining Twitch auth ownership races exposed by PRs #70 and #71 without broadening the already-reviewed lifecycle work.

**Architecture:** Keep one Rust-side async ownership gate around mutable Twitch token state so session restore, API credential acquisition, refresh, successful device-login persistence, and logout cannot race keyring writes. For legacy pre-#70 token rows, recover the issuing `client_id` from `/oauth2/validate` before proactive refresh while the access token is still valid; when that identity is unknown, never destroy the shared keyring row merely because a fallback client ID was rejected. On the frontend, invalidate stale session-restore completions when login/logout begins.

**Tech Stack:** Rust 2021, Tokio, Tauri 2, React 19, Zustand, Vitest.

**Spec:** Follow-up audit of Rillmux PRs #67-#71, stacked on PR #71 head `4993797a942e37167afb5c0fed449bc50dc64fc8`.

## Global Constraints

- Do not modify PR #71.
- Do not broaden dependency versions from PRs #67-#69.
- Preserve token secrecy and token-bound Twitch Client-Id behavior from PR #70.
- Preserve PR #71 listener/HUD/EventSub lifecycle behavior.
- Fix only evidence-backed auth ownership defects.
- Do not merge.

---

### Task 1: Lock mutable Rust auth token ownership

**Files:**
- Modify: `src-tauri/src/auth/mod.rs`
- Test: `src-tauri/src/auth/mod.rs`
- Test: `src-tauri/tests/auth_device_network_recovery_contract.rs`

**Interfaces:**
- Produces: process-wide `token_state_gate() -> &'static tokio::sync::Mutex<()>`.
- Consumes: existing `load_tokens`, `save_tokens`, `clear_tokens`, `refresh_if_needed`, `session_from_tokens`.

- [ ] **Step 1: Write failing concurrency/contract tests**

Add an async unit test proving a second token-state owner cannot enter while the first holds the gate, and a contract test proving `get_session`, `credentials_for_api`, `logout`, and successful device-token persistence are coordinated by the same gate.

- [ ] **Step 2: Run focused Rust tests and verify RED**

Run the auth unit/integration tests. Expected: FAIL because no token-state gate exists or the entry points are not wired through it.

- [ ] **Step 3: Implement the minimal ownership gate**

Add a `OnceLock<tokio::sync::Mutex<()>>` and acquire it at the token-state mutation/read-refresh boundaries. Do not lock unrelated auth network calls that never touch persisted token state.

- [ ] **Step 4: Run focused tests and verify GREEN**

Expected: PASS and no deadlock.

### Task 2: Recover legacy token client identity before proactive refresh

**Files:**
- Modify: `src-tauri/src/auth/mod.rs`
- Test: `src-tauri/src/auth/mod.rs`
- Test: `src-tauri/tests/auth_device_network_recovery_contract.rs`

**Interfaces:**
- Produces: helper(s) that detect missing/blank token `client_id`, recover it by validating a still-valid access token, and persist it before `refresh_if_needed` chooses a client ID.

- [ ] **Step 1: Write failing migration-order tests**

Cover a legacy `StoredTokens { client_id: None }` row near expiry and assert client-id recovery is ordered before proactive refresh. Also assert an unknown legacy client identity is treated differently from a known token-bound identity when refresh returns 400/401.

- [ ] **Step 2: Run focused tests and verify RED**

Expected: FAIL because current `session_from_tokens` / `credentials_for_api` call `refresh_if_needed` before legacy identity recovery and `refresh_if_needed` clears tokens on every 400/401.

- [ ] **Step 3: Implement minimal migration-safe ordering**

For legacy rows whose access token has not yet expired, validate first, persist `validate.client_id`, then refresh if needed. If a refresh request used only a fallback app client because token identity was still unknown, do not clear the shared keyring row solely on 400/401; return a non-transient auth error instead. Known token-bound refresh rejection retains the existing clear-and-relogin behavior.

- [ ] **Step 4: Run focused tests and verify GREEN**

Expected: PASS.

### Task 3: Invalidate stale frontend session restore across login/logout

**Files:**
- Modify: `src/lib/auth/store.ts`
- Test: `src/lib/auth/sessionRestore.test.ts` or a focused auth-store test if existing harness supports it.

**Interfaces:**
- Consumes: existing `sessionRefreshGeneration` stale-result guard.
- Produces: explicit generation invalidation when a new login or logout transition begins.

- [ ] **Step 1: Write failing behavioral test**

Start a deferred `refreshSession`, begin logout/login, resolve the older refresh, and assert the stale restore cannot overwrite the newer transition state.

- [ ] **Step 2: Run focused frontend test and verify RED**

Expected: FAIL because startLogin/logout currently do not advance `sessionRefreshGeneration`.

- [ ] **Step 3: Implement minimal generation invalidation**

Advance the restore generation at the beginning of login/logout transitions before awaiting native work. Keep retry cleanup behavior unchanged.

- [ ] **Step 4: Run focused tests and verify GREEN**

Expected: PASS.

### Task 4: Full verification and PR hygiene

**Files:**
- Delete before final PR if desired: `docs/superpowers/plans/2026-09-04-pr67-71-auth-ownership.md`

- [ ] **Step 1: Run frontend gates**

`npm test`, `npm run build`, `npm run doctor`, dependency audit gate.

- [ ] **Step 2: Run Rust gates**

`cargo fmt --check`, streaming rustfmt shard check, `cargo clippy -- -D warnings`, `cargo check`, release warning check, `cargo test`, `cargo audit`.

- [ ] **Step 3: Review PRs #67-#69 dependency/workflow updates**

Confirm no concrete Rillmux behavior regression from keyring/uuid/Sentry, React/TanStack/Router/Vite-plugin, or CodeQL/release-action patch updates. Do not change them absent evidence.

- [ ] **Step 4: Open stacked follow-up PR**

Base on `fix/runtime-lifecycle-hardening` while PR #71 remains open. Document the PR #67-#71 audit, fixed findings, reviewed-no-finding dependency updates, and verification. Do not merge.
