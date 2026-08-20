export type WatchingPhase =
  | "ads"
  | "ready"
  | "error"
  | "starting"
  | "info"
  | "ended";

export function watchingPhase(phase?: string): WatchingPhase {
  switch (phase) {
    case "ads":
    case "ready":
    case "error":
    case "starting":
    case "info":
    case "ended":
      return phase;
    default:
      return "info";
  }
}

/** Later Streamlink log lines must not replace Playing with HLS noise. */
export function nextSessionStatus(
  current: { phase?: string; ready?: boolean; status?: string },
  incoming: { phase: string; ready: boolean; status: string },
): { phase: string; ready: boolean; status: string } {
  if (current.ready && !incoming.ready && incoming.phase !== "ended") {
    return {
      phase: "ready",
      ready: true,
      status: current.status ?? incoming.status,
    };
  }
  return incoming;
}

export function sessionStatusPatch(
  current: { phase?: string; ready?: boolean; status?: string },
  incoming: { phase: string; ready: boolean; status: string },
): {
  next: { phase: string; ready: boolean; status: string };
  changed: boolean;
  becameReady: boolean;
} {
  const next = nextSessionStatus(current, incoming);
  const changed =
    current.phase !== next.phase ||
    current.ready !== next.ready ||
    current.status !== next.status;
  return {
    next,
    changed,
    becameReady: !current.ready && Boolean(next.ready),
  };
}

export function watchingStatusText(
  phase: WatchingPhase,
  fallback: string | undefined,
  t: (key: string) => string,
): string {
  switch (phase) {
    case "ads":
      return t("routes:watchingStatusAds");
    case "ready":
      return t("routes:watchingStatusReady");
    case "error":
      return t("routes:watchingStatusError");
    case "starting":
    case "info":
      return t("routes:watchingStatusStarting");
    case "ended":
      return fallback ?? "";
    default: {
      const _exhaustive: never = phase;
      return _exhaustive;
    }
  }
}
