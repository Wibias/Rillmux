# Channel Points stream HUD Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Opt-in Channel Points chip on each running mpv window: balance, remembered drag position, custom-reward catalog and redeem. No claim button; polls stay on chat.

**Architecture:** Main window syncs up to 8 frameless `points-hud-<login>` webviews. Rust returns the player HWND rect and extends the existing website-auth GQL snapshot with custom rewards. Frontend owns chip geometry, drag, catalog flip, and overlay lifecycle (same family as raid/poll overlays).

**Tech Stack:** Tauri 2, Rust, React, TypeScript, Zustand settings, Vitest, cargo test.

**Spec:** `docs/superpowers/specs/2026-08-23-channel-points-stream-hud-design.md`

## Global Constraints

- Setting `streaming.channelPointsHud` default `false`; schema 18 → 19
- Offset `streaming.channelPointsHudOffset: { x, y } | null`; one position for all streams; fractions of player inner size
- HUD requires `channelPoints` on **and** website auth; does not enable presence
- Overlay label `points-hud-<login>`; URL `/?overlay=points-hud&channel=<login>`
- Chip: 36× min 120 / max 220, idle opacity 0.4 (0.85 if reduced motion), hover 1.0, 16px default top-right inset, 8px clamp padding
- Catalog: max 280×360, prefer left+down, click redeemable row, input rewards expand then redeem
- No claim button; optional `+50` flash when `bonusClaimed` rises
- Cap 8 overlays; hide iconic or player smaller than 200×120
- Windows HWND overlay only; setting may no-op elsewhere
- Do not hide dock grips; do not put HUD in the Rillmux title bar
- Never log website tokens
- Do not commit unless the user asks

## File map

| File | Responsibility |
|------|----------------|
| `src/lib/settings/types.ts` + `store.ts` + `store.test.ts` | HUD flag + offset, schema 19 |
| `src/locales/en/settings.json` + `common.json` | Toggle + chip/catalog copy |
| `src/pages/SettingsPage.tsx` | Checkbox under Channel Points rows |
| `src/lib/streaming/pointsHud.ts` + `.test.ts` | Pure geometry, drag vs click, catalog flip, URL parse, reward sort |
| `src-tauri/src/streaming.rs` + `lib.rs` | `channel_points_hud_place` → player rect or none |
| `src-tauri/src/channel_points.rs` | Parse custom rewards; redeem command |
| `src/components/ChannelPointsHud.tsx` + `.css` | Chip + catalog overlay UI |
| `src/components/ChannelPointsHudSync.tsx` | Main-window spawn/move/close |
| `src/App.tsx` | Overlay route |
| `CHANGELOG.md` | Unreleased note |

---

### Task 1: Settings — `channelPointsHud` + offset

**Files:**
- Modify: `src/lib/settings/types.ts`
- Modify: `src/lib/settings/store.ts`
- Modify: `src/lib/settings/store.test.ts`
- Modify: `src/locales/en/settings.json`
- Modify: `src/pages/SettingsPage.tsx`

**Produces:** `settings.streaming.channelPointsHud: boolean` (default `false`), `channelPointsHudOffset: { x: number; y: number } | null` (default `null`), schema 19.

- [ ] **Step 1:** Bump `SETTINGS_SCHEMA_VERSION` to `19`. Add:

```ts
channelPointsHud: boolean;
channelPointsHudOffset: { x: number; y: number } | null;
```

under `streaming` in `AppSettings` and `defaultSettings` (`false` / `null`).

- [ ] **Step 2:** In `migrateSettings`, merge missing keys to defaults. Normalize offset: object with finite `x`,`y` in `[0,1]`, else `null`.

- [ ] **Step 3:** Settings checkbox after Channel Points polls. Disabled unless `channelPoints` is on. Copy keys: `channelPointsHud`, `channelPointsHudHint` (“Shows a dim Channel Points chip on each mpv window. Needs presence and website auth. Timed +50 bonuses still auto-claim when bonus-claim auth is connected.”).

- [ ] **Step 4:** Tests: empty migrate → `channelPointsHud === false` and offset `null`; garbage offset → `null`; valid `{x:0.2,y:0.3}` preserved.

- [ ] **Step 5:** `npm test -- src/lib/settings/store.test.ts` — pass.

---

### Task 2: Pure HUD geometry + URL + catalog helpers

**Files:**
- Create: `src/lib/streaming/pointsHud.ts`
- Create: `src/lib/streaming/pointsHud.test.ts`

**Produces:**

```ts
export const POINTS_HUD_CHIP_HEIGHT = 36;
export const POINTS_HUD_CHIP_MIN_WIDTH = 120;
export const POINTS_HUD_CHIP_MAX_WIDTH = 220;
export const POINTS_HUD_DEFAULT_INSET = 16;
export const POINTS_HUD_PAD = 8;
export const POINTS_HUD_CATALOG_MAX_WIDTH = 280;
export const POINTS_HUD_CATALOG_MAX_HEIGHT = 360;
export const POINTS_HUD_DRAG_THRESHOLD_PX = 6;
export const POINTS_HUD_MIN_PLAYER_WIDTH = 200;
export const POINTS_HUD_MIN_PLAYER_HEIGHT = 120;

export type OverlayRect = { x: number; y: number; width: number; height: number };
export type HudOffset = { x: number; y: number } | null;
export type CatalogSide = { openLeft: boolean; openDown: boolean };

export function isPointsHudOverlay(search?: string): boolean;
export function pointsHudChannelFromSearch(search?: string): string | null;
export function pointsHudOverlayUrl(channel: string): string; // `/?overlay=points-hud&channel=`
export function pointsHudLabel(channel: string): string; // `points-hud-<login>`
export function playerTooSmallForHud(player: OverlayRect): boolean;
export function clampHudOffset(offset: HudOffset): HudOffset;
export function chipRectForPlayer(player: OverlayRect, offset: HudOffset, chipWidth: number): OverlayRect;
export function offsetFromChipRect(player: OverlayRect, chip: OverlayRect): { x: number; y: number };
export function movementIsDrag(dx: number, dy: number): boolean;
export function catalogSideForChip(player: OverlayRect, chip: OverlayRect, panelW: number, panelH: number): CatalogSide;
export function overlayRectForHud(chip: OverlayRect, panel: OverlayRect | null): OverlayRect;
export function sortCustomRewards<T extends { cost: number; redeemable: boolean }>(rewards: T[]): T[];
export function rewardUnavailableReason(opts: {
  paused: boolean;
  enabled: boolean;
  inStock: boolean;
  cooldownSeconds: number;
  cost: number;
  balance: number;
}): "paused" | "disabled" | "outOfStock" | "cooldown" | "notEnough" | null;
```

Default chip: top-right inset 16px. Offset `{x,y}` is chip **top-left** as a fraction of player width/height. Clamp chip (and open catalog) inside player with 8px padding. Catalog prefers left+down; flip until it fits.

- [ ] **Step 1:** Write failing tests covering: default top-right; fraction offset; overflow clamp; drag vs click (6px); catalog flip; URL parse; `rewardUnavailableReason`; sort redeemable then cost.

- [ ] **Step 2:** Implement helpers.

- [ ] **Step 3:** `npm test -- src/lib/streaming/pointsHud.test.ts` — pass.

---

### Task 3: Rust — player rect for HUD

**Files:**
- Modify: `src-tauri/src/streaming.rs`
- Modify: `src-tauri/src/lib.rs`

**Produces:** `channel_points_hud_place(channel_login) -> Option<OverlayRect>` — **full player** outer rect in physical pixels (not the raid inset rect). None if unknown, iconic, or smaller than 200×120. Non-Windows: always None.

- [ ] **Step 1:** Add a testable filter:

```rust
pub fn channel_points_hud_player_rect(
    player: Option<OverlayRect>,
    iconic: bool,
) -> Option<OverlayRect> {
    let rect = player?;
    if iconic || rect.width < 200 || rect.height < 120 {
        return None;
    }
    Some(rect)
}
```

Unit tests: none for missing/iconic/199×120; some for 800×450.

- [ ] **Step 2:** Windows command: `find_player_window` + `IsIconic` + `overlay_rect_from_hwnd`, then `channel_points_hud_player_rect`. Register in `lib.rs`.

- [ ] **Step 3:** `cargo test channel_points_hud_player_rect` — pass.

---

### Task 4: Rust — custom rewards + redeem

**Files:**
- Modify: `src-tauri/src/channel_points.rs`
- Modify: `src-tauri/src/lib.rs`

**Produces:** `ChannelPointsReward` on `ChannelPointsSnapshot.rewards`. Command `channel_points_redeem_reward(channel_login, reward_id, text: Option<String>)`.

Reward fields (camelCase): `id`, `title`, `cost`, `imageUrl`, `isPaused`, `inStock`, `isEnabled`, `isUserInputRequired`, `cooldownSeconds`.

Parse from ChannelPointsContext JSON at common pointers (`communityPointsSettings/customRewards`, `community/channel/communityPointsSettings/customRewards`, etc.). Fail closed: empty vec + optional snapshot `rewardsError`.

Redeem: website-auth GQL, same account-match as other mutations. Prefer query:

```graphql
mutation RedeemCommunityPointsCustomReward($input: RedeemCommunityPointsCustomRewardInput!) {
  redeemCommunityPointsCustomReward(input: $input) {
    error { code }
  }
}
```

Input: `channelID`, `rewardID`, optional `text`. Empty text on input rewards → error, do not call GQL. Never log tokens.

- [ ] **Step 1:** Tests: parse two rewards (paused / input / cooldown); ignore malformed; redeem payload includes channelID+rewardID; text omitted when None.

- [ ] **Step 2:** Implement parse + include in snapshot/cache. Implement redeem then refresh snapshot.

- [ ] **Step 3:** `cargo test --lib channel_points` — pass.

---

### Task 5: Overlay UI + main-window sync

**Files:**
- Create: `src/components/ChannelPointsHud.tsx`
- Create: `src/components/ChannelPointsHud.css`
- Create: `src/components/ChannelPointsHudSync.tsx`
- Modify: `src/App.tsx`
- Modify: `src/locales/en/common.json`

**Produces:** Overlay chip/catalog; main window keeps overlays in sync with running sessions (max 8).

Chip: points glyph + localized balance; login if more than one HUD. Idle opacity 0.4 / hover 1.0; reduced motion idle 0.85, no opacity animation. Pointer: >6px move = drag, else click toggles catalog. On drag end persist offset via `setSettings`. Catalog left/down with flip. Redeem via `channel_points_redeem_reward`. After success refresh snapshot. Errors stay on the panel. `+50` flash 2s when `bonusClaimed` rises. Hide catalog when overlay hides. `focus: false` on create. Window size = chip, or chip+panel when open. Reposition after resize.

Sync (main window only): if HUD off or no website auth or no presence setting, close all. Else for each running session (`.take(8)` equivalent): `channel_points_hud_place`; skip/close if null. Create or move `points-hud-<login>`. Close leftovers. Tick with sessions/layout/setting changes (reuse watching store + interval similar to poll overlay; no busy-loop).

- [ ] **Step 1:** `App.tsx`: if `overlay=points-hud`, render `ChannelPointsHud` inside Theme + Settings bootstrap (same as raid/poll). Main tree mounts `ChannelPointsHudSync` next to `ChannelPointsPollOverlay`.

- [ ] **Step 2:** Implement overlay + sync.

- [ ] **Step 3:** `npx tsc --noEmit` and `npm test` — pass.

---

### Task 6: Changelog + verify

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1:** Unreleased Added: opt-in Channel Points chip on each mpv window (balance, drag, redeem catalog).

- [ ] **Step 2:** `npm test` and `cargo test --lib channel_points channel_points_hud` (plus existing streaming overlay tests if touched).

---

## Spec coverage

| Spec item | Task |
|-----------|------|
| Setting + schema 19 + migrate false/null | 1 |
| Disabled unless presence | 1 + 5 runtime |
| Default top-right, offset fractions, clamp | 2 |
| Drag vs click 6px | 2 + 5 |
| Overlay lifecycle, 8 cap, iconic/tiny | 3 + 5 |
| Chip UI, login when many, +50 flash | 5 |
| Catalog + redeem | 4 + 5 |
| Poll overlay unchanged | 5 (separate window) |
| Tests listed in spec | 1, 2, 3, 4 |
| Out of scope title-bar / per-channel pos | not implemented |
