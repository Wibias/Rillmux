// Unit tests for the Hermes presence implementation.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_request_uses_bare_token() {
        let request = authenticate_request("secret-token");
        assert_eq!(request["type"], "authenticate");
        assert_eq!(request["authenticate"]["token"], "secret-token");
        assert!(!request["authenticate"]["token"]
            .as_str()
            .unwrap()
            .starts_with("OAuth "));
    }

    #[test]
    fn builds_private_pubsub_subscription() {
        let request = subscription_request("sub-1", "community-points-user-v1.45537718");
        assert_eq!(request["type"], "subscribe");
        assert_eq!(request["subscribe"]["id"], "sub-1");
        assert_eq!(request["subscribe"]["type"], "pubsub");
        assert_eq!(
            request["subscribe"]["pubsub"]["topic"],
            "community-points-user-v1.45537718"
        );
    }

    #[test]
    fn unix_epoch_date_conversion_is_correct() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
    }

    #[test]
    fn extracts_poll_topic_from_hermes_notification() {
        let notification = json!({
            "type": "notification",
            "notification": {
                "pubsub": {
                    "type": "MESSAGE",
                    "data": {
                        "topic": "polls.123",
                        "message": "{\"type\":\"POLL_UPDATE\",\"data\":{\"poll\":{\"poll_id\":\"p1\"}}}"
                    }
                }
            }
        });
        let (topic, message) = pubsub_topic_and_message(&notification).expect("topic");
        assert_eq!(topic, "polls.123");
        assert_eq!(message["type"], "POLL_UPDATE");
        assert_eq!(message["data"]["poll"]["poll_id"], "p1");
    }

    #[test]
    fn classifies_private_topics_without_logging_ids() {
        assert_eq!(topic_kind("community-points-user-v1.123"), "viewer");
        assert_eq!(topic_kind("video-playback-by-id.456"), "playback");
        assert_eq!(topic_kind("polls.789"), "poll");
        assert_eq!(topic_kind("predictions-channel-v1.789"), "prediction");
        assert_eq!(topic_kind("raid.789"), "raid");
    }

    #[test]
    fn classifies_hermes_errors_without_echoing_details() {
        assert_eq!(
            hermes_error_class("Hermes authentication failed"),
            "authentication"
        );
        assert_eq!(
            hermes_error_class("Hermes subscription failed"),
            "subscription"
        );
        assert_eq!(
            hermes_error_class("Hermes requested reconnect"),
            "reconnect"
        );
        assert_eq!(
            hermes_error_class("Hermes read: connection closed"),
            "socket"
        );
    }

    #[test]
    fn parses_raid_update_as_start_with_countdown() {
        let mut logins = HashMap::new();
        logins.insert("111".into(), "alice".into());
        let (kind, raid) = parse_raid_pubsub(
            "raid.111",
            &json!({
                "type": "raid_update_v2",
                "raid": {
                    "target_id": "222",
                    "target_login": "CVZen",
                    "viewer_count": 80,
                    "force_raid_now_seconds": 90
                }
            }),
            &logins,
        )
        .expect("raid start");
        assert_eq!(kind, RaidPubsubKind::Start);
        assert_eq!(raid.from_channel, "alice");
        assert_eq!(raid.to_channel, "cvzen");
        assert_eq!(raid.to_user_id, "222");
        assert_eq!(raid.viewers, Some(80));
        assert_eq!(raid.remaining_seconds, Some(90));
        assert_eq!(raid.kind.as_deref(), Some("start"));
    }

    #[test]
    fn parses_raid_go_and_cancel() {
        let mut logins = HashMap::new();
        logins.insert("111".into(), "alice".into());
        let go = parse_raid_pubsub(
            "raid.111",
            &json!({
                "type": "raid_go_v2",
                "raid": { "target_id": "222", "target_login": "cvzen" }
            }),
            &logins,
        )
        .expect("raid go");
        assert_eq!(go.0, RaidPubsubKind::Go);
        assert_eq!(go.1.kind.as_deref(), Some("go"));
        assert!(go.1.remaining_seconds.is_none());

        let cancel = parse_raid_pubsub(
            "raid.111",
            &json!({
                "type": "raid_cancel_v2",
                "raid": { "target_id": "222", "target_login": "cvzen" }
            }),
            &logins,
        )
        .expect("raid cancel");
        assert_eq!(cancel.0, RaidPubsubKind::Cancel);
    }
}
