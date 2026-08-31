import { useSettingsStore } from "./settings/store";

const dsn = import.meta.env.VITE_SENTRY_DSN as string | undefined;

type SentrySdk = typeof import("./sentry-sdk");

let sdkPromise: Promise<SentrySdk> | null = null;
let initialized = false;

export function sentryFrontendDsn(): string | undefined {
  return dsn;
}

export function peekSentrySdk(): Promise<SentrySdk> | null {
  return sdkPromise;
}

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

export async function ensureInit(): Promise<SentrySdk | null> {
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
