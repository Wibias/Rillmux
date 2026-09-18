# stream-card-game-line

- **Intent:** Observe the stream-card meta line `{viewers} viewers - {uptime} - {game}` rendered by `StreamGrid`.
- **Entry:** `/streams` (Top streams, public), `/` (Followed, grid view), `/games/:gameId`, team pages - every `StreamGrid` surface.
- **Source:** `src/components/StreamGrid.tsx`, `src/components/PinnedFavourites.tsx`, `src/locales/en/routes.json` (`streamViewersUptimeGame`).
- **Lane:** native (same copy renders on the web lane).
- **Credentials:** no for `/streams`, which is what the drive uses (the Followed and per-game grids need a session).
- **Binaries:** none.
- **Drive:** `node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs drive stream-card-game-line` after `launch --lane native`.
- **Success:** every card meta reads `{viewers} viewers • {uptime} • {game}` - the game name follows the uptime, no segment is empty, and the viewers/uptime pair still leads.
- **Other states:** live data means the exact streamers change per run; the checks assert the line shape, not fixed names. A stream without a game name renders two segments.
