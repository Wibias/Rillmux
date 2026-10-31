// Minute-watched Spade telemetry delivery.

fn spade_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .pool_max_idle_per_host(0)
            .build()
            .expect("valid Spade HTTP client")
    })
}

fn telemetry_transport_detail(error: &reqwest::Error) -> String {
    let kind = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_request() {
        "request"
    } else if error.is_body() {
        "body"
    } else if error.is_decode() {
        "decode"
    } else if error.is_redirect() {
        "redirect"
    } else {
        "transport"
    };

    let mut causes = Vec::new();
    let mut source = error.source();
    while let Some(cause) = source {
        let message = cause.to_string();
        if !message.is_empty() && !causes.iter().any(|existing| existing == &message) {
            causes.push(message);
        }
        if causes.len() >= 3 {
            break;
        }
        source = cause.source();
    }

    if causes.is_empty() {
        kind.to_string()
    } else {
        format!("{kind}: {}", truncate_message(&causes.join(" -> ")))
    }
}

fn telemetry_transport_message(endpoint: &Url, error: &reqwest::Error) -> String {
    let host = endpoint
        .host_str()
        .unwrap_or("unknown Twitch telemetry host");
    let detail = telemetry_transport_detail(error);
    if error.is_timeout() {
        format!("Twitch telemetry timed out connecting to {host} ({detail})")
    } else if error.is_connect() {
        format!(
            "Twitch telemetry connection failed for {host}; check DNS, firewall, VPN, or ad/tracker blocking ({detail})"
        )
    } else {
        format!("Twitch telemetry transport failed for {host} ({detail})")
    }
}

async fn send_minute_watched(
    endpoint: &Url,
    target: &ViewerPresenceTarget,
    viewer_id: &str,
) -> Result<StatusCode, ProtocolError> {
    let payload = build_minute_watched_payload(target, viewer_id);
    let encoded = base64_encode(payload.as_bytes());
    let response = spade_client()
        .post(endpoint.clone())
        .header(USER_AGENT, USER_AGENT_VALUE)
        .form(&[("data", encoded.as_str())])
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|error| {
            ProtocolError::new("telemetry", telemetry_transport_message(endpoint, &error))
        })?;
    let status = response.status();
    if status == StatusCode::NO_CONTENT {
        Ok(status)
    } else {
        Err(ProtocolError::with_status(
            "telemetry",
            status,
            "Twitch did not accept the watch telemetry",
        ))
    }
}

pub(crate) fn build_minute_watched_payload(
    target: &ViewerPresenceTarget,
    viewer_id: &str,
) -> String {
    serde_json::json!([{
        "event": "minute-watched",
        "properties": {
            "channel_id": target.channel_id,
            "broadcast_id": target.broadcast_id,
            "player": "site",
            "user_id": viewer_id,
            "live": true,
            "channel": target.channel_login.to_ascii_lowercase(),
            "hidden": false,
            "logged_in": true,
            "muted": false,
            "location": "channel"
        }
    }])
    .to_string()
}

pub(crate) fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut index = 0;
    while index < input.len() {
        let first = input[index];
        let second = input.get(index + 1).copied();
        let third = input.get(index + 2).copied();

        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(
            ALPHABET[(((first & 0b0000_0011) << 4) | second.unwrap_or(0) >> 4) as usize] as char,
        );
        match second {
            Some(second) => output.push(
                ALPHABET[(((second & 0b0000_1111) << 2) | third.unwrap_or(0) >> 6) as usize]
                    as char,
            ),
            None => output.push('='),
        }
        match third {
            Some(third) => output.push(ALPHABET[(third & 0b0011_1111) as usize] as char),
            None => output.push('='),
        }
        index += 3;
    }
    output
}

fn truncate_message(message: &str) -> String {
    message.chars().take(240).collect()
}
