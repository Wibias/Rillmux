// Unit tests for viewer presence.

#[cfg(test)]
mod tests {
    use super::*;

    fn target(session_id: &str, login: &str) -> ViewerPresenceTarget {
        ViewerPresenceTarget {
            session_id: session_id.into(),
            channel_login: login.into(),
            channel_id: "1234".into(),
            broadcast_id: format!("broadcast-{session_id}"),
        }
    }

    #[test]
    fn base64_encodes_without_padding_errors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn payload_uses_real_stream_and_viewer_identifiers() {
        let value = build_minute_watched_payload(&target("s1", "Example_Channel"), "9001");
        let parsed: serde_json::Value = serde_json::from_str(&value).unwrap();
        let properties = &parsed[0]["properties"];

        assert_eq!(parsed[0]["event"], "minute-watched");
        assert_eq!(properties["channel_id"], "1234");
        assert_eq!(properties["broadcast_id"], "broadcast-s1");
        assert_eq!(properties["player"], "site");
        assert_eq!(properties["user_id"], "9001");
        assert_eq!(properties["live"], true);
        assert_eq!(properties["channel"], "example_channel");
        assert_eq!(properties["hidden"], false);
        assert_eq!(properties["logged_in"], true);
        assert_eq!(properties["muted"], false);
        assert_eq!(properties["location"], "channel");
    }

    #[test]
    fn playback_payload_matches_current_web_player_contract() {
        let payload = build_playback_access_token_payload("example");
        assert_eq!(payload["operationName"], "PlaybackAccessToken");
        assert_eq!(payload["variables"]["login"], "example");
        assert_eq!(payload["variables"]["isLive"], true);
        assert_eq!(payload["variables"]["isVod"], false);
        assert_eq!(payload["variables"]["playerType"], "picture-by-picture");
        assert_eq!(payload["variables"]["platform"], "web");
        assert_eq!(
            payload["extensions"]["persistedQuery"]["sha256Hash"],
            PLAYBACK_ACCESS_TOKEN_HASH
        );
    }

    #[test]
    fn usher_url_matches_current_web_player_contract() {
        let url = build_usher_url("example", "signature", "token").unwrap();
        let query = url.query_pairs().collect::<HashMap<_, _>>();

        assert_eq!(
            query.get("sig").map(|value| value.as_ref()),
            Some("signature")
        );
        assert_eq!(
            query.get("token").map(|value| value.as_ref()),
            Some("token")
        );
        assert_eq!(query.get("cdm").map(|value| value.as_ref()), Some("wv"));
        assert_eq!(
            query.get("player_version").map(|value| value.as_ref()),
            Some("1.22.0")
        );
        assert_eq!(
            query.get("player_type").map(|value| value.as_ref()),
            Some("pulsar")
        );
        assert_eq!(
            query.get("player_backend").map(|value| value.as_ref()),
            Some("mediaplayer")
        );
        assert_eq!(
            query
                .get("playlist_include_framerate")
                .map(|value| value.as_ref()),
            Some("true")
        );
        assert_eq!(
            query.get("allow_source").map(|value| value.as_ref()),
            Some("true")
        );
        assert_eq!(
            query.get("transcode_mode").map(|value| value.as_ref()),
            Some("cbr_v1")
        );
    }

    #[test]
    fn success_interval_stays_within_known_good_bounds() {
        for seed in [0, 1, 15, 16, 31, u64::MAX] {
            let seconds = success_interval_from_seed(seed).as_secs();
            assert!((MIN_SUCCESS_INTERVAL_SECS..=MAX_SUCCESS_INTERVAL_SECS).contains(&seconds));
        }
    }

    #[test]
    fn extracts_dynamic_twitch_client_version() {
        let html = r#"<script>window.__twilightBuildID = "12345678-1234-abcd-9876-0123456789ab";</script>"#;
        assert_eq!(
            extract_client_version(html).as_deref(),
            Some("12345678-1234-abcd-9876-0123456789ab")
        );
        assert_eq!(
            extract_client_version("window.__twilightBuildID = \"bad\";"),
            None
        );
    }

    #[test]
    fn parses_master_and_media_playlists_with_relative_urls() {
        let master_url =
            Url::parse("https://usher.ttvnw.net/api/channel/hls/example.m3u8").unwrap();
        let master = concat!(
            "#EXTM3U\r\n",
            "#EXT-X-STREAM-INF:BANDWIDTH=3000000\r\n",
            "https://video-edge.example.net/high/index.m3u8\r\n",
            "#EXT-X-STREAM-INF:BANDWIDTH=160000\r\n",
            "low/index.m3u8\r\n",
        );
        let media_url = select_media_playlist_url(master, &master_url).unwrap();
        assert_eq!(
            media_url.as_str(),
            "https://usher.ttvnw.net/api/channel/hls/low/index.m3u8"
        );

        let media = concat!(
            "#EXTM3U\r\n",
            "#EXT-X-TARGETDURATION:2\r\n",
            "#EXTINF:2.0,\r\n",
            "segment-10.ts\r\n",
            "#EXTINF:2.0,\r\n",
            "segment-11.ts\r\n",
        );
        let segment_url = select_media_segment_url(media, &media_url).unwrap();
        assert_eq!(
            segment_url.as_str(),
            "https://usher.ttvnw.net/api/channel/hls/low/segment-11.ts"
        );
    }

    #[test]
    fn sanitizes_deduplicates_and_caps_targets() {
        let selected = select_targets(vec![
            target(" one ", "Example"),
            target("one", "ignored"),
            target("two", "Second_Channel"),
            target("three", "third"),
        ]);

        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].session_id, "one");
        assert_eq!(selected[0].channel_login, "example");
        assert_eq!(selected[1].session_id, "two");
        assert_eq!(selected[1].channel_login, "second_channel");
        assert_eq!(selected[2].session_id, "three");
        assert_eq!(selected[2].channel_login, "third");
    }

    #[test]
    fn caps_presence_workers_at_layout_maximum() {
        let selected = select_targets(
            (1..=9)
                .map(|index| target(&format!("s{index}"), &format!("channel{index}")))
                .collect(),
        );

        assert_eq!(selected.len(), 8);
        assert_eq!(selected[0].session_id, "s1");
        assert_eq!(selected[7].session_id, "s8");
    }

    #[test]
    fn rejects_invalid_target_identifiers() {
        let mut invalid_channel_id = target("one", "example");
        invalid_channel_id.channel_id = "not-a-number".into();
        let mut invalid_login = target("two", "bad-login");
        invalid_login.channel_id = "2".into();

        assert!(select_targets(vec![invalid_channel_id, invalid_login]).is_empty());
    }

    #[test]
    fn extracts_runtime_urls() {
        let html =
            r#"<script src="https://static.twitchcdn.net/config/settings.abc123.js"></script>"#;
        assert_eq!(
            extract_settings_url(html).as_deref(),
            Some("https://static.twitchcdn.net/config/settings.abc123.js")
        );

        let settings = r#"window.__settings={"spade_url":"https:\/\/spade.twitch.tv\/track"};"#;
        assert_eq!(
            extract_spade_url(settings).as_deref(),
            Some("https://spade.twitch.tv/track")
        );
    }
}
