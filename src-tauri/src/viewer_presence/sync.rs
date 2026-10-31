// Presence sync: reconcile running workers with the requested targets.

pub async fn sync(
    state: SharedViewerPresence,
    enabled: bool,
    targets: Vec<ViewerPresenceTarget>,
) -> Result<ViewerPresenceStatus, ViewerPresenceError> {
    let _sync_guard = state.sync_gate.lock().await;
    state.enabled.store(enabled, Ordering::Release);

    if !enabled {
        state.limited.store(false, Ordering::Release);
        cancel_workers(&state, true);
        return get_status(&state);
    }

    let original_count = targets.len();
    let selected = select_targets(targets);
    let limited = original_count > selected.len() && selected.len() == MAX_WORKERS;
    state.limited.store(limited, Ordering::Release);

    if selected.is_empty() {
        cancel_workers(&state, true);
        state.enabled.store(true, Ordering::Release);
        return get_status(&state);
    }

    let points_auth = crate::twitch_web_auth::load_session()
        .map_err(|error| ViewerPresenceError::Message(error.to_string()))?
        .ok_or_else(|| {
            ViewerPresenceError::Message("Twitch Website Authentication is not configured".into())
        })?;
    let token = points_auth.token;
    let session = crate::auth::get_session()
        .await
        .map_err(|error| ViewerPresenceError::Message(error.to_string()))?;
    let viewer_id = session.user_id.ok_or_else(|| {
        ViewerPresenceError::Message("log in with Twitch before enabling channel points".into())
    })?;
    if !session.logged_in || points_auth.user_id != viewer_id {
        cancel_workers(&state, true);
        state.enabled.store(true, Ordering::Release);
        return Err(ViewerPresenceError::Message(
            "Twitch Website Authentication does not match the current Twitch account".into(),
        ));
    }

    let generation = state.generation.load(Ordering::Acquire);
    let desired = selected
        .iter()
        .map(|target| (target.session_id.clone(), target.clone()))
        .collect::<HashMap<_, _>>();
    let mut spawn = Vec::new();

    {
        let mut workers = state
            .workers
            .lock()
            .map_err(|_| ViewerPresenceError::Message("viewer presence state poisoned".into()))?;
        let mut removed = Vec::new();
        workers.retain(|session_id, worker| {
            let keep = desired.get(session_id) == Some(&worker.target) && worker.token == token;
            if !keep {
                worker.cancel.store(true, Ordering::Release);
                removed.push(session_id.clone());
            }
            keep
        });

        if let Ok(mut diagnostics) = state.diagnostics.lock() {
            for session_id in removed {
                diagnostics.remove(&session_id);
            }
        }

        for target in selected {
            if workers.contains_key(&target.session_id) {
                continue;
            }
            let cancel = Arc::new(AtomicBool::new(false));
            workers.insert(
                target.session_id.clone(),
                Worker {
                    target: target.clone(),
                    token: token.clone(),
                    cancel: cancel.clone(),
                },
            );
            set_diagnostic(&state, &target, "starting", None, None, None);
            spawn.push((target, cancel));
        }
    }

    for (target, cancel) in spawn {
        let worker_state = state.clone();
        let worker_viewer_id = viewer_id.clone();
        let worker_token = token.clone();
        tauri::async_runtime::spawn(async move {
            run_worker(
                worker_state,
                target,
                worker_viewer_id,
                worker_token,
                generation,
                cancel,
            )
            .await;
        });
    }

    get_status(&state)
}
