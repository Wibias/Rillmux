//! Inventory of Twitch's unsupported/private GraphQL operations used by Rillmux.
//!
//! Keep volatile operation names, persisted-query hashes and full-query
//! fallbacks here so a Twitch web-client change can be reviewed in one place.
//! Never invent a persisted hash: capture it from a real Twitch request and
//! keep a full-query fallback only when Twitch is known to accept it.

// These metadata types intentionally exist as a maintenance/audit inventory.
// Runtime request builders consume the constants below directly, while tests
// validate the richer registry. Keep the allowance narrow to these items.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivateGqlAuth {
    Website,
    TvClaim,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct QueryFallback {
    pub operation_name: &'static str,
    pub document: &'static str,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrivateGqlOperation {
    pub family: &'static str,
    pub auth: PrivateGqlAuth,
    pub persisted_hashes: &'static [&'static str],
    pub query_fallbacks: &'static [QueryFallback],
}

pub(crate) const CHANNEL_POINTS_CONTEXT_HASHES: [&str; 4] = [
    "1530a003a7d374b0380b79db0be0534f30ff46e61cffa2bc0e2468a909fbc024",
    "9988086babc615a918a1e9a722ff41d98847acac822645209ac7379eecb27152",
    "7fe050e3761eb2cf258d70ee1a21cbd76fa8cf3d7e7b12fc437e7029d446b5e3",
    "374314de591e69925fce3ddc2bcf085796f56ebb8cad67a0daa3165c03adc345",
];
pub(crate) const VIEWABLE_POLL_HASHES: [&str; 1] =
    ["d37a38ac165e9a15c26cd631d70070ee4339d48ff4975053e622b918ce638e0f"];
pub(crate) const VIEWABLE_POLL_QUERIES: [&str; 3] = [
    r#"query ViewableChannelPoll($login: String!) { channel(name: $login) { currentPoll { id title status remainingDurationMilliseconds settings { communityPointsVotes { isEnabled cost } } self { voter { choices { pollChoice { id } } } } choices { id title totalVoters votes { total communityPoints } } } } }"#,
    r#"query ViewableChannelPoll($login: String!) { user(login: $login) { channel { currentPoll { id title status remainingDurationMilliseconds settings { communityPointsVotes { isEnabled cost } } self { voter { choices { pollChoice { id } } } } choices { id title totalVoters votes { total communityPoints } } } } } }"#,
    r#"query ViewableChannelPoll($login: String!) { user(login: $login) { currentPoll { id title status remainingDurationMilliseconds settings { communityPointsVotes { isEnabled cost } } choices { id title totalVoters votes { total communityPoints } } } } }"#,
];
pub(crate) const CHANNEL_POINTS_PREDICTION_CONTEXT_HASH: &str =
    "beb846598256b75bd7c1fe54a80431335996153e358ca9c7837ce7bb83d7d383";
pub(crate) const PREDICTION_QUERY: &str = r#"query ViewablePredictions($login: String!) { channel(name: $login) { id activePredictionEvents { id title status createdAt predictionWindowSeconds outcomes { id title totalPoints totalUsers } self { prediction { points outcome { id } } } } } }"#;
pub(crate) const PREDICTION_QUERY_USER: &str = r#"query ViewablePredictions($login: String!) { user(login: $login) { channel { id activePredictionEvents { id title status createdAt predictionWindowSeconds outcomes { id title totalPoints totalUsers } self { prediction { points outcome { id } } } } } } }"#;
pub(crate) const PREDICTION_QUERY_BARE: &str = r#"query ViewablePredictions($login: String!) { channel(name: $login) { id activePredictionEvents { id title status createdAt predictionWindowSeconds outcomes { id title totalPoints totalUsers } } } }"#;
pub(crate) const MAKE_PREDICTION_HASH: &str =
    "b44682ecc88358817009f20e69d75081b1e58825bb40aa53d5dbadcc17c881d8";
pub(crate) const MAKE_PREDICTION_QUERY: &str = r#"mutation MakePrediction($input: MakePredictionInput!) { makePrediction(input: $input) { error { code } prediction { id points outcome { id } event { id title status } } } }"#;
pub(crate) const CLAIM_COMMUNITY_POINTS_HASH: &str =
    "46aaeebe02c99afdf4fc97c7c0cba964124bf6b0af229395f1f6d1feed05b3d0";
pub(crate) const VOTE_POLL_QUERY: &str = r#"mutation VotePoll($input: VotePollInput!) { votePoll(input: $input) { poll { id title status remainingDurationMilliseconds settings { communityPointsVotes { isEnabled cost } } choices { id title totalVoters votes { total communityPoints } } } } }"#;
pub(crate) const VOTE_IN_POLL_QUERY: &str = r#"mutation VoteInPoll($input: VoteInPollInput!) { voteInPoll(input: $input) { poll { id } } }"#;

const NO_HASHES: &[&str] = &[];
const NO_FALLBACKS: &[QueryFallback] = &[];
const CONTEXT_HASHES: &[&str] = &CHANNEL_POINTS_CONTEXT_HASHES;
const POLL_HASHES: &[&str] = &VIEWABLE_POLL_HASHES;
const PREDICTION_CONTEXT_HASHES: &[&str] = &[CHANNEL_POINTS_PREDICTION_CONTEXT_HASH];
const MAKE_PREDICTION_HASHES: &[&str] = &[MAKE_PREDICTION_HASH];
const CLAIM_HASHES: &[&str] = &[CLAIM_COMMUNITY_POINTS_HASH];
const POLL_FALLBACKS: &[QueryFallback] = &[
    QueryFallback {
        operation_name: "ViewableChannelPoll",
        document: VIEWABLE_POLL_QUERIES[0],
    },
    QueryFallback {
        operation_name: "ViewableChannelPoll",
        document: VIEWABLE_POLL_QUERIES[1],
    },
    QueryFallback {
        operation_name: "ViewableChannelPoll",
        document: VIEWABLE_POLL_QUERIES[2],
    },
];
const PREDICTION_FALLBACKS: &[QueryFallback] = &[
    QueryFallback {
        operation_name: "ViewablePredictions",
        document: PREDICTION_QUERY,
    },
    QueryFallback {
        operation_name: "ViewablePredictions",
        document: PREDICTION_QUERY_USER,
    },
    QueryFallback {
        operation_name: "ViewablePredictions",
        document: PREDICTION_QUERY_BARE,
    },
];
const MAKE_PREDICTION_FALLBACKS: &[QueryFallback] = &[QueryFallback {
    operation_name: "MakePrediction",
    document: MAKE_PREDICTION_QUERY,
}];
const VOTE_POLL_FALLBACKS: &[QueryFallback] = &[
    QueryFallback {
        operation_name: "VotePoll",
        document: VOTE_POLL_QUERY,
    },
    QueryFallback {
        operation_name: "VoteInPoll",
        document: VOTE_IN_POLL_QUERY,
    },
];

#[allow(dead_code)]
pub(crate) const OPERATIONS: &[PrivateGqlOperation] = &[
    PrivateGqlOperation {
        family: "ChannelPointsContext",
        auth: PrivateGqlAuth::Website,
        persisted_hashes: CONTEXT_HASHES,
        query_fallbacks: NO_FALLBACKS,
    },
    PrivateGqlOperation {
        family: "ViewableChannelPoll",
        auth: PrivateGqlAuth::Website,
        persisted_hashes: POLL_HASHES,
        query_fallbacks: POLL_FALLBACKS,
    },
    PrivateGqlOperation {
        family: "ChannelPointsPredictionContext",
        auth: PrivateGqlAuth::Website,
        persisted_hashes: PREDICTION_CONTEXT_HASHES,
        query_fallbacks: PREDICTION_FALLBACKS,
    },
    PrivateGqlOperation {
        family: "MakePrediction",
        auth: PrivateGqlAuth::Website,
        persisted_hashes: MAKE_PREDICTION_HASHES,
        query_fallbacks: MAKE_PREDICTION_FALLBACKS,
    },
    PrivateGqlOperation {
        family: "ClaimCommunityPoints",
        auth: PrivateGqlAuth::TvClaim,
        persisted_hashes: CLAIM_HASHES,
        query_fallbacks: NO_FALLBACKS,
    },
    PrivateGqlOperation {
        family: "VotePoll",
        auth: PrivateGqlAuth::Website,
        persisted_hashes: NO_HASHES,
        query_fallbacks: VOTE_POLL_FALLBACKS,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn persisted_hashes_are_real_sha256_shapes() {
        for operation in OPERATIONS {
            for hash in operation.persisted_hashes {
                assert_eq!(hash.len(), 64, "{} hash must be 64 chars", operation.family);
                assert!(
                    hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
                    "{} hash must be hexadecimal",
                    operation.family
                );
            }
        }
    }

    #[test]
    fn operation_families_are_unique() {
        let mut seen = HashSet::new();
        for operation in OPERATIONS {
            assert!(
                seen.insert(operation.family),
                "duplicate GQL family {}",
                operation.family
            );
        }
    }

    #[test]
    fn query_fallbacks_name_the_operation_they_contain() {
        for operation in OPERATIONS {
            for fallback in operation.query_fallbacks {
                assert!(fallback.document.contains(fallback.operation_name));
            }
        }
    }

    #[test]
    fn vote_poll_never_sends_an_unverified_persisted_hash() {
        let vote = OPERATIONS
            .iter()
            .find(|operation| operation.family == "VotePoll")
            .unwrap();
        assert!(vote.persisted_hashes.is_empty());
        assert_eq!(
            vote.query_fallbacks
                .iter()
                .map(|fallback| fallback.operation_name)
                .collect::<Vec<_>>(),
            vec!["VotePoll", "VoteInPoll"]
        );
    }

    #[test]
    fn claim_operation_is_bound_to_tv_identity() {
        let claim = OPERATIONS
            .iter()
            .find(|operation| operation.family == "ClaimCommunityPoints")
            .unwrap();
        assert_eq!(claim.auth, PrivateGqlAuth::TvClaim);
    }
}
