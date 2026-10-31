// Channel Points context, vote, and reward behavior.
#[test]
fn builds_channel_points_context_payload() {
    let payload = context_payload("example", CHANNEL_POINTS_CONTEXT_HASHES[0]);
    assert_eq!(payload["operationName"], "ChannelPointsContext");
    assert_eq!(payload["variables"]["channelLogin"], "example");
    assert_eq!(
        payload["extensions"]["persistedQuery"]["sha256Hash"],
        CHANNEL_POINTS_CONTEXT_HASHES[0]
    );
}

#[test]
fn builds_prediction_context_payload() {
    let payload = prediction_context_payload("example");
    assert_eq!(payload["operationName"], "ChannelPointsPredictionContext");
    assert_eq!(payload["variables"]["channelLogin"], "example");
    assert_eq!(payload["variables"]["count"], 1);
    assert_eq!(
        payload["extensions"]["persistedQuery"]["sha256Hash"],
        CHANNEL_POINTS_PREDICTION_CONTEXT_HASH
    );
}

#[test]
fn builds_vote_poll_query_payload_without_persisted_hash() {
    let payload = vote_poll_query_payload("poll-1", "choice-2", 10);
    assert_eq!(payload["operationName"], "VotePoll");
    assert_eq!(payload["variables"]["input"]["pollID"], "poll-1");
    assert_eq!(payload["variables"]["input"]["choiceID"], "choice-2");
    assert_eq!(payload["variables"]["input"]["cost"], 10);
    assert!(payload.get("extensions").is_none());
}

#[test]
fn builds_claim_payload() {
    let payload = claim_payload("123", "claim-456");
    assert_eq!(payload["operationName"], "ClaimCommunityPoints");
    assert_eq!(payload["variables"]["input"]["channelID"], "123");
    assert_eq!(payload["variables"]["input"]["claimID"], "claim-456");
    assert_eq!(
        payload["extensions"]["persistedQuery"]["sha256Hash"],
        CLAIM_COMMUNITY_POINTS_HASH
    );
}

#[test]
fn parses_balance_and_available_claim() {
    let body = json!({
        "data": {
            "community": {
                "channel": {
                    "id": "123",
                    "self": {
                        "communityPoints": {
                            "balance": 18450,
                            "availableClaim": { "id": "claim-456" }
                        }
                    }
                }
            }
        }
    });
    let context = parse_context(&body).unwrap();
    assert_eq!(context.channel_id, "123");
    assert_eq!(context.balance, 18450);
    assert_eq!(context.claim_id.as_deref(), Some("claim-456"));
    assert!(context.poll.is_none());
    assert!(context.rewards.is_empty());
}

#[test]
fn parses_custom_channel_points_rewards() {
    let body = json!({
        "data": {
            "community": {
                "channel": {
                    "id": "123",
                    "communityPointsSettings": {
                        "customRewards": [
                            {
                                "id": "r1",
                                "title": "Highlight",
                                "cost": 50,
                                "isPaused": false,
                                "isEnabled": true,
                                "isInStock": true,
                                "isUserInputRequired": false,
                                "prompt": "Clip that play",
                                "image": { "url": "https://example/a.png" }
                            },
                            {
                                "id": "r2",
                                "title": "Shoutout",
                                "cost": 500,
                                "isPaused": true,
                                "isEnabled": true,
                                "isInStock": true,
                                "isUserInputRequired": true,
                                "prompt": "Who should I shout out?",
                                "cooldownSeconds": 30
                            },
                            { "id": "", "title": "bad" }
                        ]
                    },
                    "self": {
                        "communityPoints": {
                            "balance": 100,
                            "availableClaim": null
                        }
                    }
                }
            }
        }
    });
    let context = parse_context(&body).unwrap();
    assert_eq!(context.rewards.len(), 2);
    assert_eq!(context.rewards[0].id, "r1");
    assert_eq!(context.rewards[0].title, "Highlight");
    assert_eq!(context.rewards[0].cost, 50);
    assert_eq!(
        context.rewards[0].image_url.as_deref(),
        Some("https://example/a.png")
    );
    assert!(!context.rewards[0].is_user_input_required);
    assert_eq!(context.rewards[0].prompt.as_deref(), Some("Clip that play"));
    assert!(context.rewards[1].is_paused);
    assert!(context.rewards[1].is_user_input_required);
    assert_eq!(
        context.rewards[1].prompt.as_deref(),
        Some("Who should I shout out?")
    );
    assert_eq!(context.rewards[1].cooldown_seconds, 30);
}

#[test]
fn parses_custom_rewards_from_graphql_edges() {
    let body = json!({
        "data": {
            "community": {
                "channel": {
                    "id": "123",
                    "communityPointsSettings": {
                        "customRewards": {
                            "edges": [
                                {
                                    "node": {
                                        "id": "edge-1",
                                        "title": "Hype",
                                        "cost": 25,
                                        "isPaused": false,
                                        "isEnabled": true,
                                        "isInStock": true
                                    }
                                }
                            ]
                        }
                    },
                    "self": {
                        "communityPoints": { "balance": 10, "availableClaim": null }
                    }
                }
            }
        }
    });
    let context = parse_context(&body).unwrap();
    assert_eq!(context.rewards.len(), 1);
    assert_eq!(context.rewards[0].id, "edge-1");
    assert_eq!(context.rewards[0].cost, 25);
}

#[test]
fn builds_redeem_reward_payload_without_empty_text() {
    let with_text = redeem_reward_payload("123", "r1", Some(" hello "));
    assert_eq!(
        with_text["operationName"],
        "RedeemCommunityPointsCustomReward"
    );
    assert_eq!(with_text["variables"]["input"]["channelID"], "123");
    assert_eq!(with_text["variables"]["input"]["rewardID"], "r1");
    assert_eq!(with_text["variables"]["input"]["text"], "hello");
    let without = redeem_reward_payload("123", "r1", Some("  "));
    assert!(without["variables"]["input"].get("text").is_none());
}

#[test]
fn parses_active_channel_points_poll() {
    let body = json!({
        "data": {
            "community": {
                "channel": {
                    "id": "123",
                    "activePoll": {
                        "id": "poll-1",
                        "title": "Next game?",
                        "status": "ACTIVE",
                        "remainingSeconds": 42,
                        "cost": 10,
                        "self": { "choiceID": "a" },
                        "choices": [
                            { "id": "a", "title": "Minecraft", "votes": 12, "points": 120 },
                            { "id": "b", "title": "GTA", "votes": 3, "points": 30 }
                        ]
                    },
                    "self": {
                        "communityPoints": {
                            "balance": 100,
                            "availableClaim": null
                        }
                    }
                }
            }
        }
    });
    let context = parse_context(&body).unwrap();
    let poll = context.poll.expect("poll");
    assert_eq!(poll.id, "poll-1");
    assert_eq!(poll.title, "Next game?");
    assert_eq!(poll.cost, 10);
    assert_eq!(poll.remaining_seconds, Some(42));
    assert_eq!(poll.voted_choice_id.as_deref(), Some("a"));
    assert_eq!(poll.choices.len(), 2);
    assert_eq!(poll.choices[0].title, "Minecraft");
}

#[test]
fn parses_viewable_poll_with_channel_points_cost() {
    let body = json!({
        "data": {
            "channel": {
                "viewerPoll": {
                    "id": "poll-2",
                    "title": "Map?",
                    "status": "ACTIVE",
                    "remainingDurationSeconds": 30,
                    "channelPointsVotingEnabled": true,
                    "channelPointsPerVote": 50,
                    "choices": [
                        { "id": "a", "title": "Dust2", "channelPointsVotes": 8 },
                        { "id": "b", "title": "Mirage", "channelPointsVotes": 2 }
                    ]
                }
            }
        }
    });
    let poll = parse_viewable_poll_body(&body).expect("poll");
    assert_eq!(poll.id, "poll-2");
    assert_eq!(poll.cost, 50);
    assert_eq!(poll.remaining_seconds, Some(30));
    assert_eq!(poll.choices[0].votes, 8);
}

#[test]
fn ignores_polls_without_channel_points_voting() {
    let body = json!({
        "data": {
            "channel": {
                "viewerPoll": {
                    "id": "poll-3",
                    "title": "Bits only",
                    "status": "ACTIVE",
                    "channelPointsVotingEnabled": false,
                    "choices": [{ "id": "a", "title": "Yes", "votes": 1 }]
                }
            }
        }
    });
    assert!(parse_viewable_poll_body(&body).is_none());
}

#[test]
fn parses_pubsub_channel_points_poll() {
    let event = json!({
        "type": "POLL_CREATE",
        "data": {
            "poll": {
                "poll_id": "poll-4",
                "title": "Next map?",
                "status": "ACTIVE",
                "remaining_duration_milliseconds": 45_000,
                "settings": {
                    "channel_points_votes": { "is_enabled": true, "cost": 20 }
                },
                "choices": [
                    {
                        "choice_id": "a",
                        "title": "Dust2",
                        "votes": { "total": 4, "channel_points": 4 }
                    },
                    {
                        "choice_id": "b",
                        "title": "Mirage",
                        "votes": { "total": 1, "channel_points": 1 }
                    }
                ]
            }
        }
    });
    ingest_pubsub("polls.999", &event);
    let poll = cached_poll("999").expect("cached poll");
    assert_eq!(poll.id, "poll-4");
    assert_eq!(poll.cost, 20);
    assert_eq!(poll.remaining_seconds, Some(45));
    assert_eq!(poll.choices[0].title, "Dust2");
    assert_eq!(poll.choices[0].votes, 4);
    ingest_pubsub("polls.999", &json!({ "type": "POLL_COMPLETE" }));
    assert!(cached_poll("999").is_none());
}

#[test]
fn parses_user_channel_current_poll() {
    let body = json!({
        "data": {
            "user": {
                "channel": {
                    "currentPoll": {
                        "id": "poll-5",
                        "title": "Game?",
                        "status": "ACTIVE",
                        "remainingDurationMilliseconds": 12_000,
                        "settings": {
                            "channelPointsVotes": { "isEnabled": true, "cost": 15 }
                        },
                        "choices": [
                            { "id": "a", "title": "Yes", "votes": { "total": 2, "channelPoints": 2 } }
                        ]
                    }
                }
            }
        }
    });
    let poll = parse_viewable_poll_body(&body).expect("poll");
    assert_eq!(poll.id, "poll-5");
    assert_eq!(poll.cost, 15);
    assert_eq!(poll.remaining_seconds, Some(12));
}

#[test]
fn parses_helix_active_channel_points_poll() {
    let body = json!({
        "data": [{
            "id": "poll-6",
            "title": "Draw?",
            "status": "ACTIVE",
            "channel_points_voting_enabled": true,
            "channel_points_per_vote": 25,
            "choices": [
                { "id": "a", "title": "Yes", "votes": 4 },
                { "id": "b", "title": "No", "votes": 1 }
            ]
        }]
    });
    let poll = parse_helix_polls(&body).expect("poll");
    assert_eq!(poll.id, "poll-6");
    assert_eq!(poll.cost, 25);
    assert_eq!(poll.choices[0].title, "Yes");
}

#[test]
fn ignores_completed_or_expired_polls_on_stream_load() {
    let completed = json!({
        "data": [{
            "id": "poll-done",
            "title": "Closed",
            "status": "COMPLETED",
            "channel_points_voting_enabled": true,
            "choices": [{ "id": "a", "title": "Yes", "votes": 1 }]
        }]
    });
    assert!(parse_helix_polls(&completed).is_none());

    let expired = json!({
        "data": {
            "channel": {
                "viewerPoll": {
                    "id": "poll-expired",
                    "title": "Ended",
                    "status": "ACTIVE",
                    "remainingSeconds": 0,
                    "channelPointsVotingEnabled": true,
                    "choices": [{ "id": "a", "title": "Yes", "votes": 1 }]
                }
            }
        }
    });
    assert!(parse_viewable_poll_body(&expired).is_none());

    let untitled_status = json!({
        "data": {
            "channel": {
                "viewerPoll": {
                    "id": "poll-nostatus",
                    "title": "Stale",
                    "channelPointsVotingEnabled": true,
                    "choices": [{ "id": "a", "title": "Yes", "votes": 1 }]
                }
            }
        }
    });
    assert!(parse_viewable_poll_body(&untitled_status).is_none());
}
