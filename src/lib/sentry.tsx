import {
  Component,
  useEffect,
  useRef,
  type ErrorInfo,
  type ReactNode,
} from "react";
import { useSettingsStore } from "./settings/store";
import { invoke, isTauri } from "./tauri";

const dsn = import.meta.env.VITE_SENTRY_DSN as string | undefined;

type SentrySdk = typeof import("./sentry-sdk");

let sdkPromise: Promise<SentrySdk> | null = null;
let initialized = false;

function loadSentrySdk(): Promise<SentrySdk> {
  sdkPromise ??= import("./sentry-sdk");
  return sdkPromise;
}

/**
 * Redact anything that could carry credentials:
 * - Bearer headers
 * - signed Streamlink/Twitch CDN URLs (?sig=…&token=…)
 * - Twitch token-shaped strings (30-char lowercase alnum)
 * Query strings are stripped from URLs because session URLs are signed.
 */
const SCRUB_PATTERNS: RegExp[] = [
  /Bearer\s+[A-Za-z0-9\-_.]+/gi,
  /[?&](sig|token|oauth_token|access_token|code)=[^&\s"']+/gi,
  /\b[a-z0-9]{30}\b/g,
];

function scrubString(value: string): string {
  let out = value;
  for (const re of SCRUB_PATTERNS) {
    out = out.replace(re, (m) =>
      m.startsWith("?") || m.startsWith("&")
        ? m[0] + "[redacted]"
        : "[redacted]",
    );
  }
  return out;
}

function stripUrlQuery(value: string): string {
  try {
    const url = new URL(value);
    url.search = "";
    return url.toString();
  } catch {
    return value;
  }
}

function scrubData(value: unknown): unknown {
  if (typeof value === "string") return scrubString(stripUrlQuery(value));
  if (Array.isArray(value)) return value.map(scrubData);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [key, v] of Object.entries(value)) {
      out[key] =
        key.toLowerCase() === "authorization" ? "[redacted]" : scrubData(v);
    }
    return out;
  }
  return value;
}

async function ensureInit(): Promise<SentrySdk | null> {
  if (!dsn) return null;
  const Sentry = await loadSentrySdk();
  if (initialized) return Sentry;

  Sentry.init({
    dsn,
    enabled: true,
    sendDefaultPii: false,
    tracesSampleRate: 0,
    beforeSend(event) {
      // Strip auth-ish leftovers from request, messages and breadcrumbs.
      if (event.request) {
        if (event.request.headers) {
          delete event.request.headers.Authorization;
          delete event.request.headers.authorization;
        }
        if (event.request.url) {
          event.request.url = stripUrlQuery(event.request.url);
        }
        if (event.request.query_string) {
          event.request.query_string = "";
        }
      }
      if (event.message) {
        event.message = scrubString(event.message);
      }
      if (event.exception?.values) {
        for (const ex of event.exception.values) {
          if (ex.value) ex.value = scrubString(ex.value);
        }
      }
      if (event.breadcrumbs) {
        for (const crumb of event.breadcrumbs) {
          if (crumb.message) crumb.message = scrubString(crumb.message);
          if (crumb.data) crumb.data = scrubData(crumb.data) as typeof crumb.data;
        }
      }
      return event;
    },
  });
  initialized = true;
  return Sentry;
}

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

export function captureAppError(error: unknown, context?: string): void {
  if (!dsn || !useSettingsStore.getState().settings.sentryEnabled) return;
  void ensureInit()
    .then((Sentry) => {
      if (!Sentry || !useSettingsStore.getState().settings.sentryEnabled) return;
      Sentry.captureException(
        error,
        context ? { tags: { context } } : undefined,
      );
    })
    .catch(() => undefined);
}

type AppErrorBoundaryProps = {
  children: ReactNode;
  fallback: ReactNode;
};

type AppErrorBoundaryState = {
  failed: boolean;
};

/** Keeps React crash containment synchronous while reporting lazily to Sentry. */
export class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = { failed: false };

  static getDerivedStateFromError(): AppErrorBoundaryState {
    return { failed: true };
  }

  componentDidCatch(error: Error, _info: ErrorInfo) {
    captureAppError(error, "react_error_boundary");
  }

  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}
