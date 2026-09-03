import { useEffect, useRef, useState } from "react";
import { useSettingsStore } from "../lib/settings/store";
import {
  describeViewerPresenceStatus,
  PRESENCE_STATUS_FALLBACK_MS,
  shouldRefreshChannelPoints,
  type ViewerPresenceStatus,
} from "../lib/streaming/presence";
import { invoke, isTauri } from "../lib/tauri";
import { listenWhileMounted } from "../lib/tauri/ownAsyncSubscription";
import { createSerializedKick } from "../lib/tauri/serializedKick";

interface ChannelPointsSnapshot {
  channelLogin: string;
  balance: number;
  bonusAvailable: boolean;
  bonusClaimed: boolean;
  claimHttpStatus?: number | null;
  claimError?: string | null;
}

export function ChannelPointsStatus({ compact = false }: { compact?: boolean }) {
  const enabled = useSettingsStore(
    (state) => state.settings.streaming.channelPoints,
  );
  const [status, setStatus] = useState<ViewerPresenceStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [points, setPoints] = useState<Record<string, ChannelPointsSnapshot>>({});
  const [pointsError, setPointsError] = useState<string | null>(null);
  const lastPointsRefresh = useRef(0);
  const pointsRefreshRunning = useRef(false);

  useEffect(() => {
    if (!enabled || !isTauri()) {
      setStatus(null);
      setError(null);
      setPoints({});
      setPointsError(null);
      lastPointsRefresh.current = 0;
      pointsRefreshRunning.current = false;
      return;
    }

    let active = true;

    const refreshPoints = async (presence: ViewerPresenceStatus) => {
      const logins = [
        ...new Set(
          presence.workers.flatMap((worker) => {
            const login = worker.channelLogin.trim().toLowerCase();
            return login ? [login] : [];
          }),
        ),
      ];
      if (!logins.length) {
        if (active) {
          setPoints({});
          setPointsError(null);
        }
        return;
      }

      const now = Date.now();
      if (
        !shouldRefreshChannelPoints(
          lastPointsRefresh.current,
          now,
          pointsRefreshRunning.current,
        )
      ) {
        return;
      }

      lastPointsRefresh.current = now;
      pointsRefreshRunning.current = true;
      try {
        const results = await Promise.allSettled(
          logins.map((channelLogin) =>
            invoke<ChannelPointsSnapshot>("channel_points_refresh", {
              channelLogin,
            }),
          ),
        );
        if (!active) return;

        const failures: string[] = [];
        setPoints((previous) => {
          const next: Record<string, ChannelPointsSnapshot> = {};
          for (const login of logins) {
            if (previous[login]) next[login] = previous[login];
          }
          results.forEach((result, index) => {
            const login = logins[index];
            if (result.status === "fulfilled") {
              next[login] = result.value;
            } else {
              failures.push(
                `${login}: ${
                  result.reason instanceof Error
                    ? result.reason.message
                    : String(result.reason)
                }`,
              );
            }
          });
          return next;
        });
        setPointsError(failures.length ? failures.join(" | ") : null);
      } finally {
        pointsRefreshRunning.current = false;
      }
    };

    const refreshKick = createSerializedKick(async (isCurrent) => {
      try {
        const next = await invoke<ViewerPresenceStatus>("viewer_presence_status");
        if (!isCurrent()) return;
        setStatus(next);
        setError(null);
        await refreshPoints(next);
      } catch (reason) {
        if (!isCurrent()) return;
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    });

    refreshKick.kick();
    const unlistenPresence = listenWhileMounted("viewer-presence-changed", () => {
      refreshKick.kick();
    });
    const unlistenPubsub = listenWhileMounted("channel-points-pubsub", () => {
      refreshKick.kick();
    });
    const timer = window.setInterval(
      () => refreshKick.kick(),
      PRESENCE_STATUS_FALLBACK_MS,
    );
    return () => {
      active = false;
      refreshKick.dispose();
      window.clearInterval(timer);
      unlistenPresence();
      unlistenPubsub();
    };
  }, [enabled]);

  if (!enabled) return null;
  if (compact && !error) return null;

  const presenceSummary = error
    ? `Channel Points diagnostics failed: ${error}`
    : describeViewerPresenceStatus(status);
  const balanceSummary =
    status?.workers
      .map((worker) => {
        const snapshot = points[worker.channelLogin.toLowerCase()];
        if (!snapshot) return null;
        const claimed = snapshot.bonusClaimed ? " · bonus +50 claimed" : "";
        const claimError = snapshot.claimError
          ? ` · bonus claim failed — ${snapshot.claimError}`
          : "";
        return `${worker.channelLogin}: ${snapshot.balance.toLocaleString()} points${claimed}${claimError}`;
      })
      .filter((value): value is string => Boolean(value)) ?? [];

  const summary = [
    presenceSummary,
    balanceSummary.length ? `Balance: ${balanceSummary.join(" | ")}` : null,
    pointsError ? `Balance check failed: ${pointsError}` : null,
  ]
    .filter((value): value is string => Boolean(value))
    .join(" | ");

  return (
    <div
      className={`authbar__playback${compact ? " authbar__playback--compact" : ""}`}
      title={summary}
    >
      <small className={error ? "authbar__error" : "muted"}>
        Channel Points: {summary}
      </small>
    </div>
  );
}
