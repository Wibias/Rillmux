# Channel Points poll overlay — Design

**Status:** Approved
**Date:** 2026-08-20
**Product:** Streamlink Twitch GUI
**PR:** 3 of 3. No Chatterino fork.

## Goal

If the user opts in, show active Channel Points polls for watched streams and let them vote from a small overlay over chat.

## Decisions

- New setting `streaming.channelPointsPolls`, default off.
- Requires Channel Points presence plus website auth. If those are off, the toggle does nothing useful.
- Overlay sits over owned Chatterino when present, else over the main window chat column. Not over video.
- One poll at a time: active chat channel first, then first ready session.
- Vote with the existing website OAuth GQL session. No TV-claim identity.
- No Chatterino7 fork in this PR.

## Detection

Poll Helix-unavailable private GQL:

- `ChannelPointsContext` already runs per watched login. Extend the parsed snapshot with `activePoll` when present.
- Refresh on the existing 15s Channel Points timer. If the field is absent in the current hash, keep the overlay hidden rather than invent a second undocumented query in v1.

## Overlay

- Frameless always-on-top window, same pattern as the raid overlay.
- Shows title, remaining time, choices, and point cost.
- Clicking a choice calls `channel_points_vote_poll`.
- Dismiss hides that poll id until it changes.

## Out of scope

- Predictions
- Automatic voting
- Chatterino plugin or fork
- Voting without website auth
