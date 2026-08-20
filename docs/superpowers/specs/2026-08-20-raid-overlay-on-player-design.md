# Raid overlay on player or Chatterino — Design

**Status:** Approved
**Date:** 2026-08-20
**Product:** Streamlink Twitch GUI
**PR:** 2 of 3. Polls stay out of this PR.

## Goal

When a watched channel raids out, show the existing 15s Follow / Stay prompt over the player or Chatterino. Do not hide it inside the main app window.

## Why the current banner never appears

1. RaidBanner mounts only in the main window. Watching often minimizes or hides that window.
2. RaidBanner drops the prompt as soon as the source session is no longer running. Twitch usually ends that stream when the raid starts, so the prompt deletes itself.

## Decisions

- Keep the same 15s Follow now / Stay actions.
- Show a small always-on-top overlay window, not the main window.
- Place it on the raiding mpv window. If that window is gone, use owned Chatterino. If both are gone, use the main window as fallback.
- Do not dismiss the prompt just because the source session ended.
- Still dismiss if the user clicks Stay, Follow now, or the countdown finishes.
- One overlay at a time. Queue extra raids as today.

## Implementation

- Rust command `raid_overlay_place(fromChannel)` finds the mpv title `stgui-<login>` or owned Chatterino HWND and returns `{x,y,width,height}`.
- Frontend opens or reuses a frameless alwaysOnTop webview labeled `raid-overlay`.
- Overlay HTML/React shows the existing banner copy and calls `followRaid` / stay.
- Main-window RaidBanner no longer renders the in-app strip. It only owns the event listener and overlay lifecycle, or the overlay page owns the listener itself.
- Overlay closes after accept, stay, or last queue item.

## Tests

- Frontend: queue still works; source-session end does not drop the active raid.
- Rust: title helper and placement fallback order (mpv then Chatterino).
