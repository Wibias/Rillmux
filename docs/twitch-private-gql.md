# Twitch private GraphQL operations

Rillmux uses a small set of Twitch web-client GraphQL operations for Channel Points features that are not available through Twitch's public API. These are unsupported private interfaces and can change without notice.

## Source of truth

All volatile operation names, persisted-query hashes and full-query fallbacks live in `src-tauri/src/twitch_gql_operations.rs`. `channel_points.rs` contains request/response behaviour, not copied magic hashes.

## Rules

1. Never invent or guess a persisted-query hash. Capture it from a real Twitch web or TV request.
2. Add a newly captured hash before older known-good hashes so rollback remains possible.
3. Keep a full GraphQL document only when Twitch demonstrably accepts non-persisted queries for that operation.
4. Keep the auth identity explicit. `ClaimCommunityPoints` is the dedicated TV-claim session; normal context, polls, predictions and rewards use website auth.
5. If an operation breaks, fail that feature with a bounded error. Do not broaden OAuth scopes or switch client identities as a fallback.

## Current families

- `ChannelPointsContext`: website auth, persisted hashes only
- `ViewableChannelPoll`: website auth, persisted hash plus full-query fallbacks
- `ViewablePredictions`: website auth, full-query fallbacks
- `MakePrediction`: website auth, persisted hash plus full-query fallback
- `ClaimCommunityPoints`: TV-claim auth, persisted hash only
- `VotePoll`: website auth, full-query fallbacks only

`VotePoll` deliberately has no persisted hash. A previous value was synthetic rather than captured from Twitch, which guaranteed a useless failed request before the real query fallbacks.

## Refresh procedure

When Twitch changes an operation:

1. Reproduce the request in Twitch's own web or TV client.
2. Record the exact `operationName`, variables shape, persisted SHA-256 hash if present, and client identity used.
3. Update only the matching family in `twitch_gql_operations.rs`.
4. Add or update a contract fixture for the payload/response shape.
5. Run `cargo test`, then smoke-test the affected Channel Points action with a real account.
