# Channel Points stream HUD — Design

**Status:** Approved (conversation 2026-08-23)
**Date:** 2026-08-23
**Product:** Rillmux

## Goal

Let the user opt in to a Channel Points chip **on each mpv window**: balance, a draggable remembered position, and a rewards catalog they can redeem from. Timed +50 bonuses stay automatic (existing claim path). Polls and predictions stay on the chat overlay, not on this chip.

## Why not the Rillmux window

Video is in mpv, not the webview. Min/max/close live on the Rillmux title bar. The HUD must track each player HWND (same family as the raid overlay), not `#tbo-controls`.

## Decisions

| Topic | Choice |
|---|---|
| Placement | Overlay on **each** owned mpv window |
| Default corner | Top-right, 16px inset from that player’s client rect |
| Visibility | Idle opacity 0.4; hover on the chip → 1.0 |
| Bonus +50 | Automatic as today; chip may flash `+50` after a successful auto-claim; **no claim button** |
| Catalog | Click chip (not a drag) → panel; click a ready reward → redeem |
| Drag memory | One `{x, y}` fraction of the **player** for all streams, persisted in settings |
| Title-bar copy | Out of scope |

## Setting

New `streaming.channelPointsHud` (boolean, default `false`).

- Shown under Streaming next to the existing Channel Points rows.
- Disabled (and treated as off) unless `streaming.channelPoints` is on **and** website auth is connected.
- Does not turn on presence by itself. Hint copy: needs Channel Points presence and website auth; +50 still auto-claims when bonus-claim auth is connected.
- Bump `SETTINGS_SCHEMA_VERSION` (currently 18 → 19). Missing key migrates to `false`. Offset missing migrates to `null` (use default corner).

Persisted offset: `streaming.channelPointsHudOffset: { x: number, y: number } | null`.

- `x` / `y` are the chip’s **top-left** as a fraction of the player inner width/height, range `[0, 1]`.
- `null` means default top-right inset.
- One offset for every stream (not per channel).
- Clamp so the chip (and open catalog, if any) stays fully inside the player with 8px padding.

## Overlay lifecycle

- One frameless, transparent, always-on-top, skip-taskbar webview per **running** session, label `points-hud-<login>`.
- URL: `/?overlay=points-hud&channel=<login>` (same `overlay=` query pattern as the raid overlay in `App.tsx`).
- Rust command `channel_points_hud_place(channelLogin) -> Option<OverlayRect>`:
  - Resolve mpv via existing `find_player_window` (`rillmux-<login>`, then legacy `stgui-<login>`).
  - Return the **player** outer rect in physical pixels (same units as `raid_overlay_place`). Frontend applies offset + chip size and divides by scale factor when calling `WebviewWindow`.
- Main window owns spawn/move/close: sync whenever sessions, layout, or the HUD setting change (reuse dock/session tick; do not busy-loop).
- Hide (close or skip) when: HUD off, session not running, player iconic, or player smaller than 200×120.
- Overlay window size = chip only when catalog closed; grows to chip + panel when open. Reposition after resize so the chip origin stays put and the panel stays inside the player.
- Chip layout box: height 36px, min-width 120px, padding 8px 10px, max-width 220px.
- Catalog panel: max 280×360px, scroll if needed.
- Do not steal focus on create/move. Click-through is **not** used: the window is only as large as the chip/panel, so mpv keeps the rest of the video.

## Chip

- Channel Points glyph + localized balance (tabular nums).
- If more than one HUD is open, also show the channel login.
- Idle opacity 0.4; `:hover` / `:focus-visible` 1.0. `prefers-reduced-motion: reduce`: skip opacity animation; idle 0.85.
- Pointer: drag if movement exceeds 6px before `pointerup`; otherwise treat as click (toggle catalog).
- On drag end, convert chip top-left to fractions of the **current** player rect and persist `channelPointsHudOffset`.
- Optional: if the latest snapshot has `bonusClaimed` and the previous tick did not, show a short `+50` next to the balance (2s). Not a button.

## Catalog

- Toggle on chip click. Closed by a second click on the chip, or when the overlay hides.
- Panel prefers opening **left and down** from the chip; if that overflows the player, flip left/right and up/down until it fits (same clamp as the chip).
- Data: that channel’s **custom** Channel Points rewards from the existing website-auth GQL session used by `channel_points.rs` (extend `ChannelPointsSnapshot`, do not invent a second auth). Include: id, title, image URL (if any), cost, `isPaused`, `isInStock`, `isEnabled`, cooldown remaining if present, whether the reward requires input text.
- Sort: enabled and in-stock first, then by cost ascending.
- Row states: affordable + in stock + not paused + cooldown 0 → redeemable. Otherwise greyed with a one-line reason (paused, not enough points, cooldown, out of stock).
- Click redeemable row with **no** input: call redeem immediately (Twitch site behaviour).
- Input rewards: expand an inline text field + Redeem. Empty text does not submit.
- After success: refresh snapshot (balance + reward list). After error: keep the panel open, show the error on the panel (no modal).
- Images: `img-src` already allows `https:`. If a reward has no image, show the points glyph, not a broken img.

Redeem command: `channel_points_redeem_reward { channelLogin, rewardId, text?: string }` in Rust, using the same website-auth GQL client as `channel_points.rs`. Operation name should match current Twitch (`redeemCommunityPointsCustomReward` or the hash the live site uses). Same account-match rules as other Channel Points mutations. Never log tokens.

## Interaction with existing features

- Presence + auto +50: unchanged. HUD is display + catalog only.
- Poll/prediction overlay: unchanged; stays over chat. Both overlays may be visible at once.
- Auth bar `ChannelPointsStatus`: keep diagnostics there; do not duplicate the long status string on mpv.

## Tests (must exist before calling the feature done)

- Pure: default chip rect (top-right inset) from a player `OverlayRect` + chip size.
- Pure: fraction offset → screen rect; clamp when the chip would overflow.
- Pure: drag vs click (movement ≤ 6px is click).
- Pure: catalog flip when the preferred side overflows.
- Settings migrate: missing HUD keys → `false` / `null`.
- Rust: `channel_points_hud_place` returns none for unknown/tiny/iconic player (unit or existing window-title helpers).
- Overlay URL parser: `overlay=points-hud` + channel.

## Out of scope

- Per-channel saved positions
- HUD in the Rillmux title bar
- Global rewards browser when no stream is open
- Automatic redeem of custom rewards
- Changing auto +50 behaviour
- Non-Windows (no mpv HWND overlay; setting can exist and no-op)

## Risks

- Twitch GQL hashes for reward lists/redeem change without notice (same class as current Channel Points). Fail closed: chip can still show balance from the existing snapshot; catalog empty + error string.
- Many mpv tiles ⇒ many webviews. Cap at the existing session cap (8). Close overlays when sessions stop.
