// Cached website-auth status with failure backoff.

pub(crate) struct WebsiteAuthRuntime {
    cache: Mutex<WebsiteAuthCache>,
}

impl WebsiteAuthRuntime {
    pub(crate) fn new() -> Self {
        Self {
            cache: Mutex::new(WebsiteAuthCache {
                status: None,
                retry_at: None,
                backoff: STATUS_FAILURE_BACKOFF_START,
                last_error: None,
            }),
        }
    }

    fn lock_cache(&self) -> MutexGuard<'_, WebsiteAuthCache> {
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn get_status<F>(&self, load: F) -> Result<TwitchWebAuthStatus, TwitchWebAuthError>
    where
        F: Fn() -> Result<TwitchWebAuthStatus, TwitchWebAuthError>,
    {
        self.get_status_at(Instant::now(), load)
    }

    pub(crate) fn get_status_at<F>(
        &self,
        now: Instant,
        load: F,
    ) -> Result<TwitchWebAuthStatus, TwitchWebAuthError>
    where
        F: Fn() -> Result<TwitchWebAuthStatus, TwitchWebAuthError>,
    {
        {
            let cache = self.lock_cache();
            if let Some(status) = cache.status.clone() {
                return Ok(status);
            }
            if let Some(retry_at) = cache.retry_at {
                if now < retry_at {
                    return Err(TwitchWebAuthError::Message(
                        cache
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "website auth unavailable".into()),
                    ));
                }
            }
        }
        match load() {
            Ok(status) => {
                self.remember(status.clone());
                Ok(status)
            }
            Err(error) => {
                let mut cache = self.lock_cache();
                cache.status = None;
                let delay = cache.backoff;
                cache.retry_at = Some(now + delay);
                cache.backoff = delay.saturating_mul(2).min(STATUS_FAILURE_BACKOFF_MAX);
                cache.last_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    pub(crate) fn remember(&self, status: TwitchWebAuthStatus) {
        let mut cache = self.lock_cache();
        cache.status = Some(status);
        cache.retry_at = None;
        cache.backoff = STATUS_FAILURE_BACKOFF_START;
        cache.last_error = None;
    }
}

fn website_auth_runtime() -> &'static WebsiteAuthRuntime {
    static RUNTIME: OnceLock<WebsiteAuthRuntime> = OnceLock::new();
    RUNTIME.get_or_init(WebsiteAuthRuntime::new)
}
