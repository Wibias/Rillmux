// Unit tests for EventSub.

#[cfg(test)]
fn parse_raid_notification_json(text: &str) -> Option<RaidOutgoing> {
    let env: WsEnvelope = serde_json::from_str(text).ok()?;
    if env.metadata.message_type != "notification" {
        return None;
    }
    parse_raid_notification(&env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_channel_raid_notification() {
        let raw = r#"{
          "metadata": {
            "message_id": "1",
            "message_type": "notification",
            "message_timestamp": "2026-08-01T00:00:00Z",
            "subscription_type": "channel.raid",
            "subscription_version": "1"
          },
          "payload": {
            "subscription": {
              "id": "sub1",
              "status": "enabled",
              "type": "channel.raid",
              "version": "1",
              "condition": { "from_broadcaster_user_id": "111" },
              "transport": { "method": "websocket", "session_id": "s" },
              "created_at": "2026-08-01T00:00:00Z",
              "cost": 0
            },
            "event": {
              "from_broadcaster_user_id": "111",
              "from_broadcaster_user_login": "Alice",
              "from_broadcaster_user_name": "Alice",
              "to_broadcaster_user_id": "222",
              "to_broadcaster_user_login": "Bob",
              "to_broadcaster_user_name": "Bob",
              "viewers": 42
            }
          }
        }"#;
        let raid = parse_raid_notification_json(raw).expect("parse");
        assert_eq!(raid.from_channel, "alice");
        assert_eq!(raid.to_channel, "bob");
        assert_eq!(raid.to_user_id, "222");
        assert_eq!(raid.viewers, Some(42));
        assert_eq!(raid.kind.as_deref(), Some("go"));
    }

    #[test]
    fn identifies_subscription_auth_rejection() {
        assert!(subscription_auth_rejected(401));
        assert!(subscription_auth_rejected(403));
        assert!(!subscription_auth_rejected(429));
        assert!(!subscription_auth_rejected(500));
    }

    #[test]
    fn keepalive_uses_server_timeout_plus_small_grace() {
        let session = WsSession {
            id: "s".into(),
            keepalive_timeout_seconds: Some(10),
            reconnect_url: None,
        };
        assert_eq!(keepalive_duration(&session), Duration::from_secs(12));
    }

    #[test]
    fn classifies_eventsub_errors_without_echoing_details() {
        assert_eq!(
            eventsub_error_class("auth: token unavailable"),
            "authentication"
        );
        assert_eq!(eventsub_error_class("create sub 500"), "subscription");
        assert_eq!(
            eventsub_error_class("session_reconnect missing reconnect_url"),
            "reconnect"
        );
        assert_eq!(
            eventsub_error_class("EventSub welcome timed out"),
            "welcome"
        );
        assert_eq!(eventsub_error_class("ws read: connection closed"), "socket");
        assert_eq!(
            eventsub_error_class("EventSub connection timed out"),
            "connect"
        );
    }

    #[test]
    fn status_payload_is_safe_and_shaped_for_the_ui() {
        let status = EventSubStatus {
            enabled: true,
            watched_channels: 2,
            state: EventSubConnectionState::Retrying,
            last_error: Some("authentication"),
        };
        let value = serde_json::to_value(status).unwrap();
        assert_eq!(value["enabled"], true);
        assert_eq!(value["watchedChannels"], 2);
        assert_eq!(value["state"], "retrying");
        assert_eq!(value["lastError"], "authentication");
        assert!(!value.to_string().to_ascii_lowercase().contains("token"));
    }

    #[tokio::test]
    async fn eventsub_connect_timeout_is_retryable() {
        assert_eq!(CONNECT_TIMEOUT, Duration::from_secs(12));
        let result = tokio::time::timeout(
            Duration::from_millis(10),
            std::future::pending::<Result<(), &'static str>>(),
        )
        .await;
        let error = map_eventsub_connect_result(result).unwrap_err();
        assert_eq!(error, "EventSub connection timed out");
        let class = eventsub_error_class(&error);
        assert!(
            class == "connect" || class == "timeout",
            "timeout must remain a supervisor-retry class, got {class}"
        );
    }
}
