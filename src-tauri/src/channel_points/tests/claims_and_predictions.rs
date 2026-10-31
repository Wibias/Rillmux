// Channel Points claim, prediction, and cache behavior.
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
