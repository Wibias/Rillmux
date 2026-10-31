// Playback token resolution, HLS playlist walking, and segment touching.

async fn resolve_runtime(channel_login: &str) -> Result<RuntimeConfig, ProtocolError> {
    let page_url = format!("{TWITCH_URL}/{channel_login}");
    let page_url = Url::parse(&page_url)
        .map_err(|_| ProtocolError::new("runtime-config", "invalid Twitch channel URL"))?;
    let (page, _) = fetch_text(&page_url, "runtime-config").await?;
    let client_version =
        extract_client_version(&page).unwrap_or_else(|| FALLBACK_CLIENT_VERSION.to_string());

    let settings_url = extract_settings_url(&page).ok_or_else(|| {
        ProtocolError::new(
            "runtime-config",
            "Twitch runtime settings URL was not found",
        )
    })?;
    let settings_url = validate_https_url(&settings_url, "runtime-config")?;
    let (settings, _) = fetch_text(&settings_url, "runtime-config").await?;
    let spade_url = extract_spade_url(&settings).ok_or_else(|| {
        ProtocolError::new("runtime-config", "Twitch Spade endpoint was not found")
    })?;
    let spade_url = validate_https_url(&spade_url, "runtime-config")?;

    Ok(RuntimeConfig {
        spade_url,
        client_version,
    })
}

#[derive(Debug, Deserialize)]
struct PlaybackTokenResponse {
    data: Option<PlaybackTokenData>,
    errors: Option<Vec<GraphQlError>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackTokenData {
    stream_playback_access_token: Option<PlaybackAccessToken>,
}

#[derive(Debug, Deserialize)]
struct PlaybackAccessToken {
    signature: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct GraphQlError {
    message: Option<String>,
}

async fn fetch_playback_access_token(
    channel_login: &str,
    token: &str,
    device_id: &str,
    client_session: &str,
    client_version: &str,
) -> Result<PlaybackAccessToken, ProtocolError> {
    let payload = build_playback_access_token_payload(channel_login);
    let response = shared_client()
        .post(GQL_URL)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .header(AUTHORIZATION, format!("OAuth {token}"))
        .header("Client-Id", crate::twitch_web_auth::WEB_CLIENT_ID)
        .header("Client-Session-Id", client_session)
        .header("Client-Version", client_version)
        .header("X-Device-Id", device_id)
        .header(REFERER, format!("{TWITCH_URL}/{channel_login}"))
        .json(&payload)
        .send()
        .await
        .map_err(|_| ProtocolError::new("playback-token", "Twitch GraphQL request failed"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProtocolError::with_status(
            "playback-token",
            status,
            "Twitch rejected the playback-token request",
        ));
    }

    let body = response
        .json::<PlaybackTokenResponse>()
        .await
        .map_err(|_| {
            ProtocolError::with_status(
                "playback-token",
                status,
                "Twitch returned an invalid playback-token response",
            )
        })?;
    if let Some(token) = body
        .data
        .and_then(|data| data.stream_playback_access_token)
        .filter(|value| !value.signature.is_empty() && !value.value.is_empty())
    {
        return Ok(token);
    }

    let message = body
        .errors
        .unwrap_or_default()
        .into_iter()
        .filter_map(|error| error.message)
        .next()
        .map(|message| truncate_message(&message))
        .unwrap_or_else(|| "Twitch returned no playback access token".to_string());
    Err(ProtocolError::with_status(
        "playback-token",
        status,
        message,
    ))
}

pub(crate) fn build_playback_access_token_payload(channel_login: &str) -> serde_json::Value {
    serde_json::json!({
        "operationName": "PlaybackAccessToken",
        "variables": {
            "login": channel_login,
            "isLive": true,
            "isVod": false,
            "vodID": "",
            "playerType": "picture-by-picture",
            "platform": "web"
        },
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": PLAYBACK_ACCESS_TOKEN_HASH
            }
        }
    })
}

fn build_usher_url(
    channel_login: &str,
    signature: &str,
    token: &str,
) -> Result<Url, ProtocolError> {
    let mut url = Url::parse(&format!("{USHER_URL}{channel_login}.m3u8"))
        .map_err(|_| ProtocolError::new("master-playlist", "invalid Twitch playlist URL"))?;
    url.query_pairs_mut()
        .append_pair("sig", signature)
        .append_pair("token", token)
        .append_pair("cdm", "wv")
        .append_pair("player_version", "1.22.0")
        .append_pair("player_type", "pulsar")
        .append_pair("player_backend", "mediaplayer")
        .append_pair("playlist_include_framerate", "true")
        .append_pair("allow_source", "true")
        .append_pair("transcode_mode", "cbr_v1");
    Ok(url)
}

async fn fetch_text(url: &Url, stage: &'static str) -> Result<(String, StatusCode), ProtocolError> {
    let response = shared_client()
        .get(url.clone())
        .header(USER_AGENT, USER_AGENT_VALUE)
        .send()
        .await
        .map_err(|_| ProtocolError::new(stage, "Twitch request failed"))?;
    response_text(response, stage).await
}

async fn response_text(
    response: Response,
    stage: &'static str,
) -> Result<(String, StatusCode), ProtocolError> {
    let status = response.status();
    if !status.is_success() {
        return Err(ProtocolError::with_status(
            stage,
            status,
            "Twitch returned an unsuccessful response",
        ));
    }
    let text = response.text().await.map_err(|_| {
        ProtocolError::with_status(stage, status, "Twitch response could not be read")
    })?;
    Ok((text, status))
}

fn select_media_playlist_url(master: &str, master_url: &Url) -> Result<Url, ProtocolError> {
    let mut best: Option<(u64, Url)> = None;
    let mut pending_bandwidth: Option<u64> = None;

    for line in master.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending_bandwidth = rest
                .split(',')
                .find_map(|part| part.trim().strip_prefix("BANDWIDTH="))
                .and_then(|value| value.parse::<u64>().ok());
            continue;
        }

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Ok(url) = master_url.join(line.trim_matches('"')) {
            if url.scheme() == "https" {
                let bandwidth = pending_bandwidth.unwrap_or(u64::MAX);
                if best
                    .as_ref()
                    .map(|(best_bw, _)| bandwidth < *best_bw)
                    .unwrap_or(true)
                {
                    best = Some((bandwidth, url));
                }
            }
        }

        pending_bandwidth = None;
    }

    if let Some((_, url)) = best {
        return Ok(url);
    }
    if master.contains("#EXT-X-TARGETDURATION") || master.contains("#EXTINF") {
        return Ok(master_url.clone());
    }
    Err(ProtocolError::new(
        "master-playlist",
        "Twitch master playlist contained no playable rendition",
    ))
}

fn select_media_segment_url(media: &str, media_url: &Url) -> Result<Url, ProtocolError> {
    let mut urls = playlist_urls(media, media_url);
    for line in media.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("#EXT-X-TWITCH-PREFETCH:") {
            if let Ok(url) = media_url.join(value.trim().trim_matches('"')) {
                urls.push(url);
            }
        }
    }
    urls.pop().ok_or_else(|| {
        ProtocolError::new(
            "media-playlist",
            "Twitch media playlist contained no segment",
        )
    })
}

pub(crate) fn playlist_urls(playlist: &str, base_url: &Url) -> Vec<Url> {
    playlist
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| base_url.join(line.trim_matches('"')).ok())
        .filter(|url| url.scheme() == "https")
        .collect()
}

async fn touch_media_segment(url: &Url) -> Result<StatusCode, ProtocolError> {
    let response = shared_client()
        .head(url.clone())
        .header(USER_AGENT, USER_AGENT_VALUE)
        .send()
        .await
        .map_err(|_| ProtocolError::new("media-segment", "Twitch media-segment request failed"))?;
    let status = response.status();
    if status.is_success() || status.is_redirection() {
        return Ok(status);
    }

    if status == StatusCode::METHOD_NOT_ALLOWED {
        let fallback = shared_client()
            .get(url.clone())
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|_| {
                ProtocolError::new("media-segment", "Twitch media-segment fallback failed")
            })?;
        let fallback_status = fallback.status();
        if fallback_status.is_success() || fallback_status == StatusCode::PARTIAL_CONTENT {
            return Ok(fallback_status);
        }
        return Err(ProtocolError::with_status(
            "media-segment",
            fallback_status,
            "Twitch rejected the media-segment fallback",
        ));
    }

    Err(ProtocolError::with_status(
        "media-segment",
        status,
        "Twitch rejected the media-segment request",
    ))
}

fn validate_https_url(raw: &str, stage: &'static str) -> Result<Url, ProtocolError> {
    let parsed =
        Url::parse(raw).map_err(|_| ProtocolError::new(stage, "invalid Twitch runtime URL"))?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(ProtocolError::new(
            stage,
            "Twitch runtime URL must use HTTPS",
        ));
    }
    Ok(parsed)
}

pub(crate) fn extract_client_version(html: &str) -> Option<String> {
    let marker = "window.__twilightBuildID";
    let tail = &html[html.find(marker)? + marker.len()..];
    let tail = &tail[tail.find('=')? + 1..];
    let quote_index = tail.find(['"', '\''])?;
    let quote = tail.as_bytes().get(quote_index).copied()? as char;
    let value_tail = &tail[quote_index + 1..];
    let end = value_tail.find(quote)?;
    let value = &value_tail[..end];
    if value.len() == 36
        && value.chars().enumerate().all(|(index, character)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                character == '-'
            } else {
                character.is_ascii_hexdigit()
            }
        })
    {
        Some(value.to_string())
    } else {
        None
    }
}

pub(crate) fn extract_settings_url(html: &str) -> Option<String> {
    const PREFIXES: [&str; 2] = [
        "https://static.twitchcdn.net/config/settings",
        "https://assets.twitch.tv/config/settings",
    ];
    PREFIXES.iter().find_map(|prefix| {
        let start = html.find(prefix)?;
        let tail = &html[start..];
        let end = tail
            .find(|character: char| {
                character == '"'
                    || character == '\''
                    || character == '<'
                    || character.is_whitespace()
            })
            .unwrap_or(tail.len());
        Some(tail[..end].replace("\\/", "/"))
    })
}

pub(crate) fn extract_spade_url(settings: &str) -> Option<String> {
    const PATTERNS: [&str; 2] = ["\"spade_url\":\"", "\\\"spade_url\\\":\\\""];
    PATTERNS.iter().find_map(|pattern| {
        let start = settings.find(pattern)? + pattern.len();
        let tail = &settings[start..];
        let end = if pattern.starts_with("\\\"") {
            tail.find("\\\"")?
        } else {
            tail.find('"')?
        };
        Some(tail[..end].replace("\\/", "/"))
    })
}
