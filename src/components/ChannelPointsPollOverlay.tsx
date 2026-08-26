import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "../lib/settings/store";
import {
  applyConfirmedPredictionVote,
  isClosedPredictionError,
  overlayRectMoved,
  pollOverlayRect,
  POLL_FALLBACK_REFRESH_MS,
  pollOverlayShouldPollGql,
  predictionAcceptsVotes,
  predictionRemainingSeconds,
  type ConfirmedPredictionVote,
  type OverlayRect,
} from "../lib/streaming/pollOverlay";
import { useWatchingStore } from "../lib/streaming/store";
import { invoke, isTauri } from "../lib/tauri";
import "./ChannelPointsPollOverlay.css";

export interface ChannelPointsPollChoice {
  id: string;
  title: string;
  votes: number;
  points: number;
  totalVoters: number;
}

export interface ChannelPointsPoll {
  id: string;
  title: string;
  status: string;
  remainingSeconds?: number | null;
  cost: number;
  votedChoiceId?: string | null;
  choices: ChannelPointsPollChoice[];
}

export interface ChannelPointsPredictionOutcome {
  id: string;
  title: string;
  points: number;
  users: number;
}

export interface ChannelPointsPrediction {
  id: string;
  title: string;
  status: string;
  createdAt?: string | null;
  windowSeconds?: number | null;
  predictedOutcomeId?: string | null;
  predictedPoints?: number | null;
  outcomes: ChannelPointsPredictionOutcome[];
}

interface ChannelPointsSnapshot {
  channelLogin: string;
  balance: number;
  poll?: ChannelPointsPoll | null;
  prediction?: ChannelPointsPrediction | null;
}

const OVERLAY_LABEL = "poll-overlay";
const MIN_PREDICTION_POINTS = 10;
const MAX_PREDICTION_POINTS = 250_000;

function overlayEventId(snapshot: ChannelPointsSnapshot | null): string | null {
  if (snapshot?.poll?.status === "ACTIVE") return snapshot.poll.id;
  if (
    snapshot?.prediction &&
    (snapshot.prediction.status === "ACTIVE" || snapshot.prediction.status === "LOCKED")
  ) {
    return snapshot.prediction.id;
  }
  return null;
}

export function isPollOverlayWindow() {
  return new URLSearchParams(window.location.search).get("overlay") === "poll";
}

function pollChannelFromSearch() {
  return new URLSearchParams(window.location.search).get("channel")?.trim() ?? null;
}

function overlayUrl(channel: string) {
  const params = new URLSearchParams({ overlay: "poll", channel });
  return `/?${params.toString()}`;
}

async function measureEmbeddedChat(): Promise<OverlayRect | null> {
  const el = document.querySelector(".embedded-chat:not(.embedded-chat--empty)");
  if (!(el instanceof HTMLElement)) return null;
  const box = el.getBoundingClientRect();
  if (box.width < 80 || box.height < 80) return null;
  const win = getCurrentWindow();
  const pos = await win.innerPosition().catch(() => null);
  const scale = await win.scaleFactor().catch(() => 1);
  if (!pos) return null;
  return {
    x: pos.x + box.left * scale,
    y: pos.y + box.top * scale,
    width: box.width * scale,
    height: box.height * scale,
  };
}

async function resolvePollOverlayRect(): Promise<OverlayRect | null> {
  const chatterino = await invoke<OverlayRect | null>("poll_overlay_place").catch(
    () => null,
  );
  const chat = chatterino ?? (await measureEmbeddedChat());
  let main: OverlayRect | null = null;
  if (!chat) {
    const win = getCurrentWindow();
    const pos = await win.outerPosition().catch(() => null);
    const size = await win.outerSize().catch(() => null);
    if (pos && size) {
      main = { x: pos.x, y: pos.y, width: size.width, height: size.height };
    }
  }
  return pollOverlayRect(chat, main);
}

let placedChannel = "";
let overlayOpen = false;
let placing = false;
let lastPlacedRect: OverlayRect | null = null;
let lastHostSnapshot: ChannelPointsSnapshot | null = null;
let pollRefreshRunning = false;

async function placeOverlayWindow(channel: string) {
  if (!isTauri() || placing) return;
  placing = true;
  try {
    const rect = await resolvePollOverlayRect();
    if (!rect) {
      await closeOverlayWindow();
      return;
    }
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const { LogicalPosition, LogicalSize } = await import("@tauri-apps/api/dpi");
    const scale = await getCurrentWindow().scaleFactor().catch(() => 1);
    const x = Math.round(rect.x / scale);
    const y = Math.round(rect.y / scale);
    const width = Math.round(rect.width / scale);
    const height = Math.round(rect.height / scale);
    const existing = overlayOpen
      ? await WebviewWindow.getByLabel(OVERLAY_LABEL)
      : null;
    if (existing && placedChannel === channel) {
      if (lastPlacedRect && !overlayRectMoved(lastPlacedRect, rect)) {
        return;
      }
      lastPlacedRect = rect;
      await existing.setAlwaysOnTop(true).catch(() => undefined);
      await existing.setPosition(new LogicalPosition(x, y)).catch(() => undefined);
      await existing.setSize(new LogicalSize(width, height)).catch(() => undefined);
      await invoke("poll_overlay_raise").catch(() => undefined);
      return;
    }
    placedChannel = channel;
    overlayOpen = true;
    lastPlacedRect = rect;
    await existing?.close().catch(() => undefined);
    new WebviewWindow(OVERLAY_LABEL, {
      url: overlayUrl(channel),
      title: "Poll",
      decorations: false,
      transparent: true,
      shadow: false,
      skipTaskbar: true,
      alwaysOnTop: true,
      resizable: false,
      visible: false,
      focus: false,
      x,
      y,
      width,
      height,
    });
  } finally {
    placing = false;
  }
}

async function closeOverlayWindow() {
  if (!isTauri() || !overlayOpen) {
    placedChannel = "";
    overlayOpen = false;
    lastPlacedRect = null;
    return;
  }
  overlayOpen = false;
  placedChannel = "";
  lastPlacedRect = null;
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const overlay = await WebviewWindow.getByLabel(OVERLAY_LABEL);
  await overlay?.close().catch(() => undefined);
}

export function ChannelPointsPollOverlay() {
  const { t } = useTranslation("common");
  const overlayWindow = isPollOverlayWindow();
  const settingsEnabled = useSettingsStore(
    (state) =>
      state.settings.streaming.channelPoints &&
      state.settings.streaming.channelPointsPolls,
  );
  const enabled = overlayWindow || settingsEnabled;
  const runningChannel = useWatchingStore((state) => {
    const preferred = state.activeChatChannel?.toLowerCase();
    const running = state.sessions
      .filter((session) => session.running)
      .map((session) => session.channel.toLowerCase());
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
      lastHostSnapshot = merged;
      setSnapshot(merged);
      setError(null);
      const eventId = overlayEventId(merged);
      if (!overlayWindow && eventId && !dismissed.current.has(eventId)) {
        void placeOverlayWindow(channel);
        void emit("poll-overlay-state", merged);
      } else if (!overlayWindow && eventId) {
        void emit("poll-overlay-state", merged);
      }
    };
    const refresh = async (useCache: boolean) => {
      if (pollRefreshRunning) return;
      pollRefreshRunning = true;
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
        pollRefreshRunning = false;
      }
    };
    if (pollOverlayShouldPollGql(overlayWindow)) {
      void refresh(false);
    }
    let unlistenPubsub: (() => void) | undefined;
    if (pollOverlayShouldPollGql(overlayWindow)) {
      void listen("channel-points-pubsub", () => {
        void refresh(true);
      }).then((fn) => {
        unlistenPubsub = fn;
      });
    }
    const timer = pollOverlayShouldPollGql(overlayWindow)
      ? window.setInterval(() => void refresh(false), POLL_FALLBACK_REFRESH_MS)
      : 0;
    return () => {
      active = false;
      if (timer) window.clearInterval(timer);
      unlistenPubsub?.();
    };
  }, [channel, enabled, overlayWindow]);

  useEffect(() => {
    if (overlayWindow || !isTauri()) return;
    let unlisten: (() => void) | undefined;
    void listen<{ pollId: string }>("poll-overlay-dismiss", (event) => {
      const pollId = event.payload?.pollId?.trim();
      if (!pollId) return;
      dismissed.current.add(pollId);
      if (confirmedPredictionVote.current?.eventId === pollId) {
        confirmedPredictionVote.current = null;
      }
      setSnapshot((current) => {
        if (!current) return current;
        if (current.poll?.id === pollId) return { ...current, poll: null };
        if (current.prediction?.id === pollId) return { ...current, prediction: null };
        return current;
      });
      void closeOverlayWindow();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [overlayWindow]);

  useEffect(() => {
    if (overlayWindow || !isTauri() || !channel) return;
    let unlisten: (() => void) | undefined;
    void listen<{ channel: string }>("poll-overlay-ready", (event) => {
      const readyChannel = event.payload?.channel?.trim().toLowerCase();
      if (readyChannel !== channel) return;
      if (lastHostSnapshot?.channelLogin.toLowerCase() !== channel) return;
      void emit("poll-overlay-state", lastHostSnapshot);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [channel, overlayWindow]);

  useEffect(() => {
    if (!overlayWindow || !isTauri() || !channel) return;
    let unlisten: (() => void) | undefined;
    void listen<ChannelPointsSnapshot>("poll-overlay-state", (event) => {
      if (!event.payload) return;
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
      unlisten = fn;
      void emit("poll-overlay-ready", { channel });
    });
    return () => unlisten?.();
  }, [channel, overlayWindow]);

  const poll = snapshot?.poll;
  const prediction = snapshot?.prediction;
  const showPoll =
    Boolean(enabled && channel && poll && poll.status === "ACTIVE") &&
    !(poll && dismissed.current.has(poll.id));
  const showPrediction =
    Boolean(
      enabled &&
        channel &&
        prediction &&
        (prediction.status === "ACTIVE" || prediction.status === "LOCKED"),
    ) && !(prediction && dismissed.current.has(prediction.id));
  const showOverlay = showPoll || showPrediction;
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

  async function vote(choiceId: string) {
    if (!poll || !channel || poll.votedChoiceId || votingId) return;
    setVotingId(choiceId);
    try {
      const next = await invoke<ChannelPointsSnapshot>("channel_points_vote_poll", {
        channelLogin: channel,
        pollId: poll.id,
        choiceId,
        cost: poll.cost,
      });
      setSnapshot(next);
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setVotingId(null);
    }
  }

  async function votePrediction(outcomeId: string) {
    if (!prediction || !channel || votingId) return;
    if (!predictionAcceptsVotes(prediction)) return;
    if (
      prediction.predictedOutcomeId ||
      confirmedPredictionVote.current?.eventId === prediction.id
    ) {
      return;
    }
    setVotingId(outcomeId);
    try {
      const next = await invoke<ChannelPointsSnapshot>(
        "channel_points_vote_prediction",
        {
          channelLogin: channel,
          eventId: prediction.id,
          outcomeId,
          points: clampedStake,
        },
      );
      const confirmed: ConfirmedPredictionVote = {
        eventId: prediction.id,
        outcomeId,
        points: clampedStake,
      };
      confirmedPredictionVote.current = confirmed;
      const responsePrediction =
        next.prediction?.id === prediction.id ? next.prediction : prediction;
      setSnapshot({
        ...next,
        prediction: applyConfirmedPredictionVote(responsePrediction, confirmed),
      });
      setError(null);
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      if (isClosedPredictionError(message)) {
        dismissed.current.add(prediction.id);
        setSnapshot((current) =>
          current?.prediction?.id === prediction.id
            ? { ...current, prediction: null }
            : current,
        );
        setError(null);
        if (overlayWindow && isTauri()) {
          await emit("poll-overlay-dismiss", { pollId: prediction.id });
          await getCurrentWindow().close().catch(() => undefined);
        }
        return;
      }
      setError(message);
    } finally {
      setVotingId(null);
    }
  }

  async function dismiss() {
    const eventId = overlayEventId(snapshot);
    if (!eventId) return;
    dismissed.current.add(eventId);
    if (confirmedPredictionVote.current?.eventId === eventId) {
      confirmedPredictionVote.current = null;
    }
    setSnapshot((current) => {
      if (!current) return current;
      if (current.poll?.id === eventId) return { ...current, poll: null };
      if (current.prediction?.id === eventId) return { ...current, prediction: null };
      return current;
    });
    if (overlayWindow && isTauri()) {
      await emit("poll-overlay-dismiss", { pollId: eventId });
      await getCurrentWindow().close().catch(() => undefined);
    }
  }

  if (!channel) return null;
  if (overlayWindow && !showOverlay) return null;
  if (!showOverlay) return null;

  const cardClass = overlayWindow
    ? "poll-overlay poll-overlay--window"
    : "poll-overlay";

  if (showPrediction && prediction && !showPoll) {
    const locked = !predictionOpen;
    const alreadyPredicted = Boolean(prediction.predictedOutcomeId);
    return (
      <aside className={cardClass} role="dialog" aria-label={prediction.title}>
        <header className="poll-overlay__head">
          <div>
            <strong>{t("pollPredictionTitle", { channel })}</strong>
            <p className="poll-overlay__title">{prediction.title}</p>
          </div>
          <button
            type="button"
            className="button-secondary"
            onClick={() => void dismiss()}
          >
            {t("pollVoteDismiss")}
          </button>
        </header>
        <p className="muted">
          {locked
            ? t("pollPredictionLocked")
            : remainingPrediction != null
              ? t("pollVoteRemaining", { seconds: remainingPrediction })
              : t("pollPredictionStake")}
        </p>
        {alreadyPredicted ? (
          <p className="poll-overlay__confirmation" role="status">
            <span aria-hidden="true">✓</span> {t("pollPredictionPlaced")}
          </p>
        ) : null}
        {locked || alreadyPredicted ? null : (
          <label className="poll-overlay__stake">
            <span>{t("pollPredictionStake")}</span>
            <input
              type="number"
              min={MIN_PREDICTION_POINTS}
              max={maxStake}
              step={10}
              value={clampedStake}
              onChange={(event) => {
                const next = Number(event.target.value);
                if (!Number.isFinite(next)) return;
                setStake(next);
              }}
            />
          </label>
        )}
        <ul className="poll-overlay__choices">
          {prediction.outcomes.map((outcome) => {
            const voted = prediction.predictedOutcomeId === outcome.id;
            return (
              <li key={outcome.id}>
                <button
                  type="button"
                  className={
                    voted ? "poll-overlay__choice is-voted" : "poll-overlay__choice"
                  }
                  disabled={locked || alreadyPredicted || votingId !== null}
                  onClick={() => void votePrediction(outcome.id)}
                >
                  <span>{outcome.title}</span>
                  <span className="muted">
                    {voted
                      ? t("pollPredicted", {
                          points: prediction.predictedPoints ?? clampedStake,
                        })
                      : `${outcome.points} · ${outcome.users}`}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
        {error ? <p className="authbar__error">{error}</p> : null}
      </aside>
    );
  }

  if (!showPoll || !poll) return null;

  return (
    <aside className={cardClass} role="dialog" aria-label={poll.title}>
      <header className="poll-overlay__head">
        <div>
          <strong>{t("pollVoteTitle", { channel })}</strong>
          <p className="poll-overlay__title">{poll.title}</p>
        </div>
        <button type="button" className="button-secondary" onClick={() => void dismiss()}>
          {t("pollVoteDismiss")}
        </button>
      </header>
      <p className="muted">
        {t("pollVoteCost", { cost: poll.cost })}
        {poll.remainingSeconds != null
          ? ` · ${t("pollVoteRemaining", { seconds: poll.remainingSeconds })}`
          : ""}
      </p>
      <ul className="poll-overlay__choices">
        {poll.choices.map((choice) => {
          const voted = poll.votedChoiceId === choice.id;
          return (
            <li key={choice.id}>
              <button
                type="button"
                className={voted ? "poll-overlay__choice is-voted" : "poll-overlay__choice"}
                disabled={Boolean(poll.votedChoiceId) || votingId !== null}
                onClick={() => void vote(choice.id)}
              >
                <span>{choice.title}</span>
                <span className="muted">{voted ? t("pollVoted") : choice.votes}</span>
              </button>
            </li>
          );
        })}
      </ul>
      {error ? <p className="authbar__error">{error}</p> : null}
    </aside>
  );
}
