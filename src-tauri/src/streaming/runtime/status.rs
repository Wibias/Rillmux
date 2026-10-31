// Streamlink status parsing and session status emission.

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn display_status(raw: &str) -> String {
    let cleaned = strip_ansi(raw).trim().to_string();
    // Drop Streamlink log prefixes like [cli][info]
    let mut s = cleaned.as_str();
    while s.starts_with('[') {
        if let Some(end) = s.find(']') {
            s = s[end + 1..].trim_start();
        } else {
            break;
        }
    }
    if s.is_empty() {
        cleaned
    } else {
        s.to_string()
    }
}

/// After the player is up, HLS playlist noise must not wake the UI.
fn should_forward_status(already_ready: bool, phase: &str, _ready: bool) -> bool {
    match phase {
        "ended" | "error" | "ads" => true,
        "ready" => !already_ready,
        _ => !already_ready,
    }
}

fn classify_line(line: &str) -> (&'static str, bool) {
    let lower = line.to_lowercase();
    if lower.contains("pre-roll ads") {
        ("ads", false)
    } else if is_fatal_streamlink_error(line) {
        ("error", false)
    } else if lower.contains("player:")
        || lower.contains("starting player")
        || lower.contains("writing to player")
    {
        // Ready = the player process actually started. "Opening stream" only
        // means Streamlink began fetching — it must NOT mark the session
        // ready (layout, handoff and the missing-window grace all key off it).
        ("ready", true)
    } else if lower.contains("opening stream")
        || lower.contains("available streams")
        || lower.contains("found matching plugin")
    {
        ("starting", false)
    } else {
        ("info", false)
    }
}

fn is_fatal_streamlink_error(line: &str) -> bool {
    let lower = line.to_lowercase();
    if lower.contains("[cli][error]") {
        return true;
    }
    let display = display_status(line).to_lowercase();
    display.starts_with("error:") || display.starts_with("error ")
}

fn update_session_status(state: &StreamingState, id: &str, status: &str, phase: &str, ready: bool) {
    if let Ok(mut map) = state.inner.lock() {
        if let Some(session) = map.get_mut(id) {
            // HLS "Low latency streaming…" / playlist reload errors must not
            // replace Playing or clear ready after the player has started.
            if session.info.ready && !ready && phase != "ended" {
                return;
            }
            session.info.status = status.to_string();
            session.info.phase = phase.to_string();
            if ready && !session.info.ready {
                session.ready_at = Some(Instant::now());
            }
            session.info.ready = ready;
        }
    }
}

fn emit_status(app: &AppHandle, payload: StreamStatusPayload) {
    let _ = app.emit("stream-status", payload);
}
