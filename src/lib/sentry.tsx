import { useEffect, useRef, type ReactNode } from "react";
import { useSettingsStore } from "./settings/store";
import { invoke, isTauri } from "./tauri";
import {
  ensureInit,
  peekSentrySdk,
  sentryFrontendDsn,
} from "./sentryCapture";

/** Syncs the persisted consent toggle with both React and native Sentry. */
export function SentryBootstrap({ children }: { children: ReactNode }) {
  const enabled = useSettingsStore((s) => s.settings.sentryEnabled);
  const hydrated = useSettingsStore((s) => s.hydrated);
  const last = useRef<boolean | null>(null);

  useEffect(() => {
    if (!hydrated) return;

    // Native Sentry must follow the same persisted opt-out even when the
    // frontend DSN is not configured.
    if (isTauri()) {
      void invoke("diagnostics_set_sentry_enabled", { enabled }).catch(
        () => undefined,
      );
    }

    const dsn = sentryFrontendDsn();
    if (!dsn || last.current === enabled) return;
    last.current = enabled;
    let cancelled = false;

    if (enabled) {
      void ensureInit()
        .then((Sentry) => {
          if (cancelled || !Sentry) return;
          const client = Sentry.getClient();
          if (client) client.getOptions().enabled = true;
        })
        .catch(() => {
          if (!cancelled) last.current = null;
        });
    } else {
      // An opted-out user should not download the SDK just so we can disable it.
      const sdkPromise = peekSentrySdk();
      if (!sdkPromise) return;
      void sdkPromise
        .then((Sentry) => {
          if (cancelled) return;
          const client = Sentry.getClient();
          if (client) client.getOptions().enabled = false;
        })
        .catch(() => undefined);
    }

    return () => {
      cancelled = true;
    };
  }, [enabled, hydrated]);

  return children;
}
