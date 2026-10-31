// Shared EventSub state, desired subscriptions, and init/sync entry points.

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RaidOutgoing {
    pub from_channel: String,
    pub to_channel: String,
    pub to_user_id: String,
    pub viewers: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

struct EventSubState {
    enabled: bool,
    /// Lowercase logins the UI wants watched for outgoing raids.
    logins: HashSet<String>,
}

fn state() -> &'static Mutex<EventSubState> {
    static S: OnceLock<Mutex<EventSubState>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(EventSubState {
            enabled: true,
            logins: HashSet::new(),
        })
    })
}

fn wake() -> &'static Notify {
    static N: OnceLock<Notify> = OnceLock::new();
    N.get_or_init(Notify::new)
}

static STARTED: AtomicBool = AtomicBool::new(false);

pub fn init(app: AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    debug_raid("eventsub.supervisor.start", "started=true");
    tauri::async_runtime::spawn(async move {
        run_supervisor(app).await;
    });
}

/// Enable/disable + replace the watched login set (lowercase).
pub fn sync(enabled: bool, logins: Vec<String>) {
    let requested_count = logins.len();
    if let Ok(mut g) = state().lock() {
        g.enabled = enabled;
        g.logins = logins
            .into_iter()
            .map(|s| s.to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        debug_raid(
            "eventsub.sync.native",
            &format!(
                "enabled={enabled} requested_count={requested_count} desired_count={}",
                g.logins.len()
            ),
        );
        let active = g.enabled && !g.logins.is_empty();
        drop(g);
        sync_runtime(active);
    }
    wake().notify_waiters();
}

fn desired_state() -> Result<(bool, HashSet<String>), String> {
    let g = state().lock().map_err(|error| error.to_string())?;
    Ok((g.enabled, g.logins.clone()))
}
