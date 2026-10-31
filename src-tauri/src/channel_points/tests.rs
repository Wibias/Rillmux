use super::*;

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

#[test]
fn parses_active_prediction() {
    let body = json!({
        "data": {
            "channel": {
                "activePredictionEvents": [
                    {
                        "id": "pred-1",
                        "title": "Will it be a draw?",
                        "status": "ACTIVE",
                        "createdAt": "2026-08-20T20:00:00Z",
                        "predictionWindowSeconds": 120,
                        "outcomes": [
                            { "id": "yes", "title": "Yes", "totalPoints": 400, "totalUsers": 8 },
                            { "id": "no", "title": "No", "totalPoints": 150, "totalUsers": 3 }
                        ],
                        "self": { "prediction": { "points": 50, "outcome": { "id": "yes" } } }
                    }
                ]
            }
        }
    });
    let prediction = parse_prediction(&body).expect("prediction");
    assert_eq!(prediction.id, "pred-1");
    assert_eq!(prediction.title, "Will it be a draw?");
    assert_eq!(prediction.predicted_outcome_id.as_deref(), Some("yes"));
    assert_eq!(prediction.predicted_points, Some(50));
    assert_eq!(prediction.outcomes[0].points, 400);
    assert_eq!(prediction.outcomes.len(), 2);
}

#[test]
fn parses_community_prediction_context_active_and_locked() {
    let active = json!({
        "data": {
            "community": {
                "channel": {
                    "activePredictionEvents": [{
                        "id": "pred-active",
                        "title": "Active?",
                        "status": "ACTIVE",
                        "createdAt": "2026-08-26T20:00:00Z",
                        "predictionWindowSeconds": 120,
                        "outcomes": [
                            { "id": "a", "title": "Yes", "totalPoints": 12, "totalUsers": 2 },
                            { "id": "b", "title": "No", "totalPoints": 8, "totalUsers": 1 }
                        ]
                    }],
                    "lockedPredictionEvents": []
                }
            }
        }
    });
    assert_eq!(parse_prediction(&active).expect("active").id, "pred-active");

    let locked = json!({
        "data": {
            "community": {
                "channel": {
                    "activePredictionEvents": [],
                    "lockedPredictionEvents": [{
                        "id": "pred-locked",
                        "title": "Locked?",
                        "status": "LOCKED",
                        "createdAt": "2026-08-26T20:00:00Z",
                        "predictionWindowSeconds": 60,
                        "outcomes": [
                            { "id": "a", "title": "Yes", "totalPoints": 30, "totalUsers": 3 },
                            { "id": "b", "title": "No", "totalPoints": 20, "totalUsers": 2 }
                        ]
                    }]
                }
            }
        }
    });
    let prediction = parse_prediction(&locked).expect("locked");
    assert_eq!(prediction.id, "pred-locked");
    assert_eq!(prediction.status, "LOCKED");
}

#[test]
fn builds_make_prediction_payload() {
    let payload = make_prediction_payload("event-1", "yes", 25, "tx-1");
    assert_eq!(payload["operationName"], "MakePrediction");
    assert_eq!(payload["variables"]["input"]["eventID"], "event-1");
    assert_eq!(payload["variables"]["input"]["outcomeID"], "yes");
    assert_eq!(payload["variables"]["input"]["points"], 25);
    assert_eq!(payload["variables"]["input"]["transactionID"], "tx-1");
    let query = payload["query"]
        .as_str()
        .expect("inline MakePrediction query");
    assert!(
        query.contains("prediction {"),
        "MakePredictionPayload.prediction is the user bet, not predictionEvent"
    );
    assert!(
        !query.contains("predictionEvent"),
        "Twitch rejects predictionEvent on MakePredictionPayload"
    );
    let persisted = make_prediction_persisted_payload("event-1", "yes", 25, "tx-1");
    assert_eq!(
        persisted["extensions"]["persistedQuery"]["sha256Hash"],
        MAKE_PREDICTION_HASH
    );
}

#[test]
fn maps_make_prediction_errors() {
    let body = json!({
        "data": { "makePrediction": { "error": { "code": "NOT_ENOUGH_POINTS" } } }
    });
    assert_eq!(
        make_prediction_error(&body).as_deref(),
        Some("Not enough Channel Points to make that prediction")
    );
    let closed = json!({
        "data": { "makePrediction": { "error": { "code": "EVENT_NOT_ACTIVE" } } }
    });
    assert_eq!(
        make_prediction_error(&closed).as_deref(),
        Some("This prediction is no longer accepting votes")
    );
    assert!(make_prediction_closed(&closed));
}

#[test]
fn parses_pubsub_prediction_event() {
    let event = json!({
        "id": "pred-2",
        "title": "Next map?",
        "status": "ACTIVE",
        "created_at": "2026-08-20T20:00:00Z",
        "prediction_window_seconds": 60,
        "outcomes": [
            { "id": "a", "title": "Dust2", "total_points": 10, "total_users": 1 },
            { "id": "b", "title": "Mirage", "total_points": 20, "total_users": 2 }
        ]
    });
    let prediction = parse_prediction_event(&event).expect("prediction");
    assert_eq!(prediction.id, "pred-2");
    assert_eq!(prediction.window_seconds, Some(60));
    assert_eq!(prediction.outcomes[1].users, 2);
}

#[test]
fn caches_pubsub_prediction_events() {
    ingest_pubsub(
        "predictions-channel-v1.42",
        &json!({
            "type": "event-updated",
            "data": {
                "event": {
                    "id": "pred-3",
                    "title": "Ace?",
                    "status": "ACTIVE",
                    "outcomes": [
                        { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                        { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                    ]
                }
            }
        }),
    );
    let prediction = cached_prediction("42").expect("cached");
    assert_eq!(prediction.id, "pred-3");
    ingest_pubsub(
        "predictions-channel-v1.42",
        &json!({ "type": "event-complete" }),
    );
    assert!(cached_prediction("42").is_none());
}

#[test]
fn clears_prediction_when_event_updated_to_resolved() {
    ingest_pubsub(
        "predictions-channel-v1.43",
        &json!({
            "type": "event-updated",
            "data": {
                "event": {
                    "id": "pred-43",
                    "title": "Done?",
                    "status": "ACTIVE",
                    "outcomes": [
                        { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                        { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                    ]
                }
            }
        }),
    );
    assert!(cached_prediction("43").is_some());
    assert!(ingest_pubsub(
        "predictions-channel-v1.43",
        &json!({
            "type": "event-updated",
            "data": {
                "event": {
                    "id": "pred-43",
                    "title": "Done?",
                    "status": "RESOLVED",
                    "outcomes": [
                        { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                        { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                    ]
                }
            }
        }),
    ));
    assert!(cached_prediction("43").is_none());
}

#[test]
fn caches_locked_prediction_for_overlay_recap() {
    ingest_pubsub(
        "predictions-channel-v1.44",
        &json!({
            "type": "event-updated",
            "data": {
                "event": {
                    "id": "pred-44",
                    "title": "Over?",
                    "status": "ACTIVE",
                    "outcomes": [
                        { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                        { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                    ]
                }
            }
        }),
    );
    assert!(cached_prediction("44").is_some());
    assert!(ingest_pubsub(
        "predictions-channel-v1.44",
        &json!({
            "type": "event-updated",
            "data": {
                "event": {
                    "id": "pred-44",
                    "title": "Over?",
                    "status": "LOCKED",
                    "outcomes": [
                        { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                        { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                    ]
                }
            }
        }),
    ));
    assert_eq!(cached_prediction("44").expect("cached").status, "LOCKED");
}

#[test]
fn clears_prediction_on_event_completed() {
    ingest_pubsub(
        "predictions-channel-v1.45",
        &json!({
            "type": "event-updated",
            "data": {
                "event": {
                    "id": "pred-45",
                    "title": "Over?",
                    "status": "ACTIVE",
                    "outcomes": [
                        { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                        { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                    ]
                }
            }
        }),
    );
    assert!(cached_prediction("45").is_some());
    assert!(ingest_pubsub(
        "predictions-channel-v1.45",
        &json!({ "type": "event-completed" }),
    ));
    assert!(cached_prediction("45").is_none());
}

#[test]
fn ingest_pubsub_reports_whether_the_cache_changed() {
    assert!(ingest_pubsub(
        "predictions-channel-v1.77",
        &json!({
            "type": "event-updated",
            "data": {
                "event": {
                    "id": "pred-77",
                    "title": "Live?",
                    "status": "ACTIVE",
                    "outcomes": [
                        { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                        { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                    ]
                }
            }
        }),
    ));
    assert!(!ingest_pubsub(
        "video-playback-by-id.77",
        &json!({ "type": "viewcount" }),
    ));
}

#[test]
fn cached_snapshot_merges_pubsub_into_last_context() {
    remember_context(
        "cachesnap",
        &ContextState {
            channel_id: "88".into(),
            balance: 42,
            claim_id: None,
            poll: None,
            rewards: vec![],
        },
    );
    assert!(ingest_pubsub(
        "predictions-channel-v1.88",
        &json!({
            "type": "event-updated",
            "data": {
                "event": {
                    "id": "pred-88",
                    "title": "Cached?",
                    "status": "ACTIVE",
                    "outcomes": [
                        { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                        { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                    ]
                }
            }
        }),
    ));
    let snapshot = cached_snapshot("cachesnap").expect("snapshot");
    assert_eq!(snapshot.balance, 42);
    assert_eq!(snapshot.prediction.expect("prediction").id, "pred-88");
}

#[test]
fn parses_community_points_vote_settings() {
    let body = json!({
        "data": {
            "channel": {
                "currentPoll": {
                    "id": "poll-7",
                    "title": "Pick",
                    "status": "ACTIVE",
                    "settings": {
                        "communityPointsVotes": { "isEnabled": true, "cost": 40 }
                    },
                    "choices": [{ "id": "a", "title": "A", "votes": { "total": 3, "communityPoints": 3 } }]
                }
            }
        }
    });
    let poll = parse_viewable_poll_body(&body).expect("poll");
    assert_eq!(poll.cost, 40);
    assert_eq!(poll.choices[0].votes, 3);
}
