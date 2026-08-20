# Thumbnail refresh and Channel Points farming — Design

**Status:** Approved
**Date:** 2026-08-20
**Product:** Streamlink Twitch GUI (Tauri rewrite)
**PR:** 1 of 3 (thumbnails + points). Raid overlay and poll banner stay out of this PR.

## Goal

1. Live stream thumbnails refresh every 60 seconds on an open browse screen, and again when the user opens that screen.
2. Channel Points farming runs for every ready Streamlink session, in multistream or not, up to the layout maximum of 8.

## Decisions (locked)

- Refresh interval: 60 seconds while the live-preview query is mounted
- Open-screen refresh: refetchOnMount always for live stream lists
- Image cache: Append minute-bucket query t= to Helix preview URLs
- Game box art: Unchanged
- Points workers: One worker per ready session, cap 8
- Incomplete Helix IDs: Resolve user_id + broadcast id after stream_start

## Thumbnails

Helix preview URLs do not change when the JPEG changes. WebView2 caches by URL, so a Helix refetch alone keeps the old image.

streamThumbnail() must replace width/height as today, then append a minute-bucket cache-bust query. Keep an existing query string by using & when ? is already present.

Live stream queries that render those previews must also refetch every 60s, refetch on mount, and treat data as immediately stale.

Covered screens: Followed, Top streams, Game streams, Channel live, Team live, Multistream followed-live cards.

Out of scope: game box art, search channel avatars, notification polling.

## Channel Points

Today both frontend (buildPresenceTargets slice 0,2) and Rust (MAX_WORKERS = 2) drop extra streams. Multistream add-from-search also starts with empty Helix IDs, so presence never starts.

Change: frontend and Rust cap = 8. After stream_start, if stream.id or stream.user_id is missing, fetch getChannelStreams(login) and store the live identifiers, then force viewer_presence_sync. Keep the existing ready-running session gate.

## Testing

Unit: streamThumbnail cache-bust, presence cap 8, incomplete-ID recovery helper. Rust: select_targets keeps 8 unique valid targets. Existing Channel Points watch-contract tests stay green.

## Success criteria

An open Followed/Streams screen shows a new preview JPEG about every minute. Opening that screen fetches a fresh Helix page immediately. Four ready multistream sessions all get presence workers. Adding a channel from Multistream search still farms after Helix IDs resolve.
