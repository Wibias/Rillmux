use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub const NETWORK_UNAVAILABLE: &str = "network_unavailable";

/// Shared reqwest client: connection pooling plus sane timeouts so a stalled
/// network can never hang auth/Helix calls (or the UI waiting on them) forever.
///
/// The client can be replaced after a transport failure. Starting the process
/// offline can leave Windows/DNS/connection state unusable until a new client
/// is built; a full app restart used to be the only way to recover.
pub fn shared_client() -> reqwest::Client {
    client_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub fn reset_shared_client() {
    *client_slot().lock().unwrap_or_else(|e| e.into_inner()) = build_client();
}

pub fn is_transient(err: &reqwest::Error) -> bool {
    if err.is_status() {
        return matches!(
            err.status().map(|s| s.as_u16()),
            Some(429 | 500 | 502 | 503 | 504)
        );
    }
    err.is_connect() || err.is_timeout() || err.is_request() || err.is_body()
}

fn client_slot() -> &'static Mutex<reqwest::Client> {
    static CLIENT: OnceLock<Mutex<reqwest::Client>> = OnceLock::new();
    CLIENT.get_or_init(|| Mutex::new(build_client()))
}

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_failure_is_transient() {
        let err = build_client()
            .get("http://127.0.0.1:1/")
            .send()
            .await
            .expect_err("localhost port 1 should fail to connect");
        assert!(is_transient(&err), "{err}");
        assert_eq!(map_display(&err), NETWORK_UNAVAILABLE);
    }

    fn map_display(err: &reqwest::Error) -> &'static str {
        if is_transient(err) {
            NETWORK_UNAVAILABLE
        } else {
            "other"
        }
    }
}
