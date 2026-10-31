import { hostState } from "./state";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "../../lib/settings/store";
import { applyConfirmedPredictionVote, isPollOverlay, mergeConfirmedPredictionVoteSnapshot, nextPollOverlayReadyAttempt, overlayLiveEventId, pollOverlaySessionView, POLL_FALLBACK_REFRESH_MS, POLL_OVERLAY_READY_RETRY_MS, pollOverlayShouldPollGql, predictionAcceptsVotes, predictionRemainingSeconds, lockedPredictionDismissAfterMs, type ConfirmedPredictionVote } from "../../lib/streaming/pollOverlay";
import { useWatchingStore } from "../../lib/streaming/store";
import { invoke, isTauri } from "../../lib/tauri";
import { listenWhileMounted } from "../../lib/tauri/ownAsyncSubscription";
import { subscribePollOverlayHostListeners } from "../../lib/streaming/pollOverlayHostListeners";
import { ChannelPointsSnapshot, PollOverlayPredictionVoteConfirmed, MIN_PREDICTION_POINTS, MAX_PREDICTION_POINTS } from "./types";
import { pollChannelFromSearch, placeOverlayWindow, closeOverlayWindow } from "./window";
import { votePollChoice, votePredictionOutcome, dismissPollOverlay } from "./actions";

export function useChannelPointsPollOverlay() {
  const { t } = useTranslation("common");
  const overlayWindow = isPollOverlay();
  const settingsEnabled = useSettingsStore(
    (state) =>
      state.settings.streaming.channelPoints &&
      state.settings.streaming.channelPointsPolls,
  );
  const enabled = overlayWindow || settingsEnabled;
  const runningChannel = useWatchingStore((state) => {
    const preferred = state.activeChatChannel?.toLowerCase();
    const running = state.sessions.flatMap((session) =>
      session.running ? [session.channel.toLowerCase()] : [],
    );
    if (preferred && running.includes(preferred)) return preferred;
    return running[0] ?? null;
  });
  const [snapshot, setSnapshot] = useState<ChannelPointsSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [votingId, setVotingId] = useState<string | null>(null);
  const [stake, setStake] = useState(MIN_PREDICTION_POINTS);
  const [, setTick] = useState(0);
  const dismissed = useRef<Set<string>>(new Set());
  const confirmedPredictionVote = useRef<ConfirmedPredictionVote | null>(null);

  const channel = overlayWindow
    ? pollChannelFromSearch()?.toLowerCase() ?? null
    : runningChannel;

  useEffect(() => {
    if (!enabled || !isTauri() || !channel) {
      setSnapshot(null);
      setError(null);
      return;
    }
    let active = true;
    const applySnapshot = (next: ChannelPointsSnapshot) => {
      if (!active) return;
      const merged: ChannelPointsSnapshot = {
        ...next,
        prediction: applyConfirmedPredictionVote(
          next.prediction ?? null,
          confirmedPredictionVote.current,
        ),
      };
      hostState.lastHostSnapshot = merged;
      setSnapshot(merged);
      setError(null);
      const eventId = overlayLiveEventId(merged.poll, merged.prediction);
      if (!overlayWindow && eventId && !dismissed.current.has(eventId)) {
        void placeOverlayWindow(channel);
        void emit("poll-overlay-state", merged);
      } else if (!overlayWindow && eventId) {
        void emit("poll-overlay-state", merged);
      }
    };
    const refresh = async (useCache: boolean) => {
      if (hostState.pollRefreshRunning) return;
      hostState.pollRefreshRunning = true;
      try {
        let next = useCache
          ? await invoke<ChannelPointsSnapshot | null>("channel_points_cached", {
              channelLogin: channel,
            })
          : await invoke<ChannelPointsSnapshot>("channel_points_refresh", {
              channelLogin: channel,
              includePoll: true,
            });
        if (!next && useCache) {
          next = await invoke<ChannelPointsSnapshot>("channel_points_refresh", {
            channelLogin: channel,
            includePoll: true,
          });
        }
        if (!next) return;
        applySnapshot(next);
      } catch (reason) {
        if (!active) return;
        setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        hostState.pollRefreshRunning = false;
      }
    };
    if (pollOverlayShouldPollGql(overlayWindow)) {
      void refresh(false);
    }
    const unlistenPubsub = pollOverlayShouldPollGql(overlayWindow)
      ? listenWhileMounted("channel-points-pubsub", () => {
          void refresh(true);
        })
      : () => undefined;
    const timer = pollOverlayShouldPollGql(overlayWindow)
      ? window.setInterval(() => void refresh(false), POLL_FALLBACK_REFRESH_MS)
      : 0;
    return () => {
      active = false;
      if (timer) window.clearInterval(timer);
      unlistenPubsub();
    };
  }, [channel, enabled, overlayWindow]);

  useEffect(() => {
    if (overlayWindow || !isTauri()) return;
    return listenWhileMounted<{ pollId: string }>(
      "poll-overlay-dismiss",
      (event) => {
        const pollId = event.payload?.pollId?.trim();
        if (!pollId) return;
        dismissed.current.add(pollId);
        if (confirmedPredictionVote.current?.eventId === pollId) {
          confirmedPredictionVote.current = null;
        }
        setSnapshot((current) => {
          if (!current) return current;
          if (current.poll?.id === pollId) return { ...current, poll: null };
          if (current.prediction?.id === pollId) {
            return { ...current, prediction: null };
          }
          return current;
        });
        void closeOverlayWindow();
      },
    );
  }, [overlayWindow]);

  useEffect(() => {
    if (overlayWindow || !isTauri() || !channel) return;
    return subscribePollOverlayHostListeners<
      { channel?: string },
      PollOverlayPredictionVoteConfirmed
    >({
      listen: (event, handler) =>
        listen(event, (incoming) => handler({ payload: incoming.payload })),
      onReady: (payload) => {
        const readyChannel = payload?.channel?.trim().toLowerCase();
        if (readyChannel !== channel) return;
        if (hostState.lastHostSnapshot?.channelLogin.toLowerCase() !== channel) return;
        void emit("poll-overlay-state", hostState.lastHostSnapshot);
      },
      onConfirmedVote: (payload) => {
        const confirmedChannel = payload?.channel?.trim().toLowerCase();
        if (confirmedChannel !== channel) return;
        if (payload.snapshot?.channelLogin.toLowerCase() !== channel) return;
        const merged = mergeConfirmedPredictionVoteSnapshot(
          payload.snapshot,
          hostState.lastHostSnapshot?.prediction ?? null,
          payload.confirmed,
        );
        confirmedPredictionVote.current = payload.confirmed;
        hostState.lastHostSnapshot = merged;
        setSnapshot(merged);
        setError(null);
        void emit("poll-overlay-state", merged);
      },
    });
  }, [channel, overlayWindow]);

  useEffect(() => {
    if (!overlayWindow || !isTauri() || !channel) return;
    let active = true;
    let acknowledged = false;
    let attempts = 0;
    let retryTimer = 0;
    let unlisten: (() => void) | undefined;

    const clearRetry = () => {
      if (!retryTimer) return;
      window.clearTimeout(retryTimer);
      retryTimer = 0;
    };
    const requestState = () => {
      if (!active) return;
      const nextAttempt = nextPollOverlayReadyAttempt(attempts, acknowledged);
      if (nextAttempt == null) return;
      attempts = nextAttempt;
      void emit("poll-overlay-ready", { channel }).finally(() => {
        if (!active || acknowledged) return;
        retryTimer = window.setTimeout(requestState, POLL_OVERLAY_READY_RETRY_MS);
      });
    };

    void listen<ChannelPointsSnapshot>("poll-overlay-state", (event) => {
      if (!event.payload) return;
      acknowledged = true;
      clearRetry();
      const next = event.payload;
      setSnapshot({
        ...next,
        prediction: applyConfirmedPredictionVote(
          next.prediction ?? null,
          confirmedPredictionVote.current,
        ),
      });
      setError(null);
    }).then((fn) => {
      if (!active) {
        fn();
        return;
      }
      unlisten = fn;
      requestState();
    });

    return () => {
      active = false;
      clearRetry();
      unlisten?.();
    };
  }, [channel, overlayWindow]);

  const poll = snapshot?.poll;
  const prediction = snapshot?.prediction;
  const { showPoll, showPrediction, showOverlay } = pollOverlaySessionView({
    enabled,
    channel,
    poll,
    prediction,
    dismissed: dismissed.current,
  });
  const remainingPrediction = prediction
    ? predictionRemainingSeconds(prediction)
    : null;
  const predictionOpen = Boolean(prediction && predictionAcceptsVotes(prediction));
  const maxStake = Math.max(
    MIN_PREDICTION_POINTS,
    Math.min(MAX_PREDICTION_POINTS, snapshot?.balance ?? MAX_PREDICTION_POINTS),
  );
  const clampedStake = Math.min(maxStake, Math.max(MIN_PREDICTION_POINTS, stake));

  useEffect(() => {
    if (!showPrediction || !predictionOpen) return;
    const timer = window.setInterval(() => setTick((tick) => tick + 1), 1000);
    return () => window.clearInterval(timer);
  }, [predictionOpen, showPrediction]);

  useEffect(() => {
    if (overlayWindow || !prediction) return;
    const delay = lockedPredictionDismissAfterMs(prediction.status);
    if (delay == null) return;
    const id = prediction.id;
    if (dismissed.current.has(id)) return;
    const timer = window.setTimeout(() => {
      dismissed.current.add(id);
      setSnapshot((current) =>
        current?.prediction?.id === id
          ? { ...current, prediction: null }
          : current,
      );
    }, delay);
    return () => window.clearTimeout(timer);
  }, [overlayWindow, prediction?.id, prediction?.status]);

  useEffect(() => {
    if (!overlayWindow || !isTauri() || !showOverlay) return;
    const frame = window.requestAnimationFrame(() => {
      void getCurrentWindow()
        .show()
        .then(() => invoke("poll_overlay_raise"))
        .catch(() => undefined);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [overlayWindow, showOverlay, poll?.id, prediction?.id]);

  useEffect(() => {
    if (overlayWindow || !isTauri()) return;
    if (!showOverlay || !channel) {
      void closeOverlayWindow();
      return;
    }
    void placeOverlayWindow(channel);
  }, [channel, overlayWindow, showOverlay, poll?.id, prediction?.id]);

  return {
    channel,
    overlayWindow,
    showOverlay,
    showPrediction,
    showPoll,
    prediction,
    poll,
    predictionOpen,
    remainingPrediction,
    maxStake,
    clampedStake,
    setStake,
    votingId,
    error,
    vote: (choiceId: string) =>
      votePollChoice({
        poll,
        channel,
        votingId,
        choiceId,
        setVotingId,
        setSnapshot,
        setError,
      }),
    votePrediction: (outcomeId: string) =>
      votePredictionOutcome({
        prediction,
        channel,
        votingId,
        outcomeId,
        clampedStake,
        overlayWindow,
        setVotingId,
        setSnapshot,
        setError,
        dismissed,
        confirmedPredictionVote,
      }),
    dismiss: () =>
      dismissPollOverlay({
        snapshot,
        overlayWindow,
        setSnapshot,
        dismissed,
        confirmedPredictionVote,
      }),
    t,
  };
}

