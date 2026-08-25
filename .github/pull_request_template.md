## Summary

- 

## Verification

- [ ] `npm test`
- [ ] `npm run build`
- [ ] `cargo fmt --check`
- [ ] `cargo clippy -- -D warnings`
- [ ] `cargo test`

## Native Windows smoke tests

Complete the relevant items when changing `src-tauri/src/streaming/`, `dock.rs`, `lib.rs`, auth, Tauri capabilities, or release packaging. Mark unrelated items N/A in the PR description.

- [ ] Start and stop a stream; confirm Streamlink/player processes exit as intended.
- [ ] Quit Rillmux with an active stream; confirm owned playback/chat processes follow the documented quit behaviour.
- [ ] Open and close raid, poll/prediction, and Channel Points HUD overlays.
- [ ] Move/dock playback across monitors when window/dock code changed.
- [ ] Confirm the user's unrelated Chatterino window is not moved or terminated when chat ownership code changed.
- [ ] Confirm auth/logout and updater/release flows when those areas changed.
