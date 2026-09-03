import { useEffect, useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "../lib/settings/store";
import {
  applyConfirmedPredictionVote,
  isClosedPredictionError,
  isPollOverlay,
  mergeConfirmedPredictionVoteSnapshot,
  nextPollOverlayReadyAttempt,
  overlayLiveEventId,
  overlayRectMoved,
  pollOverlayRect,
  pollOverlaySessionView,
  POLL_FALLBACK_REFRESH_MS,
  POLL_OVERLAY_READY_RETRY_MS,
  pollOverlayShouldPollGql,
  predictionAcceptsVotes,
  predictionRemainingSeconds,
  lockedPredictionDismissAfterMs,
  type ConfirmedPredictionVote,
  type OverlayRect,
} from "../lib/streaming/pollOverlay";
import { useWatchingStore } from "../lib/streaming/store";
import { invoke, isTauri } from "../lib/tauri";
import { listenWhileMounted } from "../lib/tauri/ownAsyncSubscription";
import { subscribePollOverlayHostListeners } from "../lib/streaming/pollOverlayHostListeners";
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

interface PollOverlayPredictionVoteConfirmed {
  channel: string;
  snapshot: ChannelPointsSnapshot;
  confirmed: ConfirmedPredictionVote;
}

const OVERLAY_LABEL = "poll-overlay";
const MIN_PREDICTION_POINTS = 10;
const MAX_PREDICTION_POINTS = 250_000;

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
  const [pos, scale] = await Promise.all([
    win.innerPosition().catch(() => null),
    win.scaleFactor().catch(() => 1),
  ]);
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
    const [{ WebviewWindow }, { LogicalPosition, LogicalSize }] =
      await Promise.all([
        import("@tauri-apps/api/webviewWindow"),
        import("@tauri-apps/api/dpi"),
      ]);
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

type OverlaySnapshotSet = Dispatch<SetStateAction<ChannelPointsSnapshot | null>>;

async function votePollChoice(args: {
  poll: ChannelPointsPoll | null | undefined;
  channel: string | null;
  votingId: string | null;
  choiceId: string;
  setVotingId: (id: string | null) => void;
  setSnapshot: OverlaySnapshotSet;
  setError: (error: string | null) => void;
}) {
  const { poll, channel, votingId, choiceId } = args;
  if (!poll || !channel || poll.votedChoiceId || votingId) return;
  args.setVotingId(choiceId);
  try {
    const next = await invoke<ChannelPointsSnapshot>("channel_points_vote_poll", {
      channelLogin: channel,
      pollId: poll.id,
      choiceId,
      cost: poll.cost,
    });
    args.setSnapshot(next);
    args.setError(null);
  } catch (reason) {
    args.setError(reason instanceof Error ? reason.message : String(reason));
  } finally {
    args.setVotingId(null);
  }
}

async function votePredictionOutcome(args: {
  prediction: ChannelPointsPrediction | null | undefined;
  channel: string | null;
  votingId: string | null;
  outcomeId: string;
  clampedStake: number;
  overlayWindow: boolean;
  setVotingId: (id: string | null) => void;
  setSnapshot: OverlaySnapshotSet;
  setError: (error: string | null) => void;
  dismissed: MutableRefObject<Set<string>>;
  confirmedPredictionVote: MutableRefObject<ConfirmedPredictionVote | null>;
}) {
  const { prediction, channel, votingId, outcomeId, clampedStake, overlayWindow } =
    args;
  if (!prediction || !channel || votingId) return;
  if (!predictionAcceptsVotes(prediction)) return;
  if (
    prediction.predictedOutcomeId ||
    args.confirmedPredictionVote.current?.eventId === prediction.id
  ) {
    return;
  }
  args.setVotingId(outcomeId);
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
    args.confirmedPredictionVote.current = confirmed;
    const merged = mergeConfirmedPredictionVoteSnapshot(
      next,
      prediction,
      confirmed,
    );
    args.setSnapshot(merged);
    args.setError(null);
    if (isTauri()) {
      if (overlayWindow) {
        void emit("poll-overlay-vote-confirmed", {
          channel,
          snapshot: merged,
          confirmed,
        } satisfies PollOverlayPredictionVoteConfirmed);
      } else {
        lastHostSnapshot = merged;
        void emit("poll-overlay-state", merged);
      }
    }
  } catch (reason) {
    const message = reason instanceof Error ? reason.message : String(reason);
    if (isClosedPredictionError(message)) {
      args.dismissed.current.add(prediction.id);
      args.setSnapshot((current) =>
        current?.prediction?.id === prediction.id
          ? { ...current, prediction: null }
          : current,
      );
      args.setError(null);
      if (overlayWindow && isTauri()) {
        await emit("poll-overlay-dismiss", { pollId: prediction.id });
        await getCurrentWindow().close().catch(() => undefined);
      }
      return;
    }
    args.setError(message);
  } finally {
    args.setVotingId(null);
  }
}

async function dismissPollOverlay(args: {
  snapshot: ChannelPointsSnapshot | null;
  overlayWindow: boolean;
  setSnapshot: OverlaySnapshotSet;
  dismissed: MutableRefObject<Set<string>>;
  confirmedPredictionVote: MutableRefObject<ConfirmedPredictionVote | null>;
}) {
  const eventId = overlayLiveEventId(
    args.snapshot?.poll,
    args.snapshot?.prediction,
  );
  if (!eventId) return;
  args.dismissed.current.add(eventId);
  if (args.confirmedPredictionVote.current?.eventId === eventId) {
    args.confirmedPredictionVote.current = null;
  }
  args.setSnapshot((current) => {
    if (!current) return current;
    if (current.poll?.id === eventId) return { ...current, poll: null };
    if (current.prediction?.id === eventId) {
      return { ...current, prediction: null };
    }
    return current;
  });
  if (args.overlayWindow && isTauri()) {
    await emit("poll-overlay-dismiss", { pollId: eventId });
    await getCurrentWindow().close().catch(() => undefined);
  }
}

function useChannelPointsPollOverlay() {
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
      lastHostSnapshot = merged;
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
        if (lastHostSnapshot?.channelLogin.toLowerCase() !== channel) return;
        void emit("poll-overlay-state", lastHostSnapshot);
      },
      onConfirmedVote: (payload) => {
        const confirmedChannel = payload?.channel?.trim().toLowerCase();
        if (confirmedChannel !== channel) return;
        if (payload.snapshot?.channelLogin.toLowerCase() !== channel) return;
        const merged = mergeConfirmedPredictionVoteSnapshot(
          payload.snapshot,
          lastHostSnapshot?.prediction ?? null,
          payload.confirmed,
        );
        confirmedPredictionVote.current = payload.confirmed;
        lastHostSnapshot = merged;
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

function ChannelPointsPredictionCard({
  channel,
  cardClass,
  prediction,
  predictionOpen,
  remainingPrediction,
  maxStake,
  clampedStake,
  setStake,
  votingId,
  error,
  votePrediction,
  dismiss,
  t,
}: {
  channel: string;
  cardClass: string;
  prediction: ChannelPointsPrediction;
  predictionOpen: boolean;
  remainingPrediction: number | null;
  maxStake: number;
  clampedStake: number;
  setStake: (value: number) => void;
  votingId: string | null;
  error: string | null;
  votePrediction: (outcomeId: string) => Promise<void>;
  dismiss: () => Promise<void>;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  const locked = !predictionOpen;
  const alreadyPredicted = Boolean(prediction.predictedOutcomeId);
  return (
    <dialog className={cardClass} open aria-label={prediction.title}>
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
              const next = event.currentTarget.valueAsNumber;
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
    </dialog>
  );
}

function ChannelPointsPollCard({
  channel,
  cardClass,
  poll,
  votingId,
  error,
  vote,
  dismiss,
  t,
}: {
  channel: string;
  cardClass: string;
  poll: ChannelPointsPoll;
  votingId: string | null;
  error: string | null;
  vote: (choiceId: string) => Promise<void>;
  dismiss: () => Promise<void>;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  return (
    <dialog className={cardClass} open aria-label={poll.title}>
      <header className="poll-overlay__head">
        <div>
          <strong>{t("pollVoteTitle", { channel })}</strong>
          <p className="poll-overlay__title">{poll.title}</p>
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
                className={
                  voted ? "poll-overlay__choice is-voted" : "poll-overlay__choice"
                }
                disabled={Boolean(poll.votedChoiceId) || votingId !== null}
                onClick={() => void vote(choice.id)}
              >
                <span>{choice.title}</span>
                <span className="muted">
                  {voted ? t("pollVoted") : choice.votes}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
      {error ? <p className="authbar__error">{error}</p> : null}
    </dialog>
  );
}

export function ChannelPointsPollOverlay() {
  const model = useChannelPointsPollOverlay();
  if (!model.channel) return null;
  if (model.overlayWindow && !model.showOverlay) return null;
  if (!model.showOverlay) return null;

  const cardClass = model.overlayWindow
    ? "poll-overlay poll-overlay--window"
    : "poll-overlay";

  if (model.showPrediction && model.prediction && !model.showPoll) {
    return (
      <ChannelPointsPredictionCard
        channel={model.channel}
        cardClass={cardClass}
        prediction={model.prediction}
        predictionOpen={model.predictionOpen}
        remainingPrediction={model.remainingPrediction}
        maxStake={model.maxStake}
        clampedStake={model.clampedStake}
        setStake={model.setStake}
        votingId={model.votingId}
        error={model.error}
        votePrediction={model.votePrediction}
        dismiss={model.dismiss}
        t={model.t}
      />
    );
  }

  if (!model.showPoll || !model.poll) return null;

  return (
    <ChannelPointsPollCard
      channel={model.channel}
      cardClass={cardClass}
      poll={model.poll}
      votingId={model.votingId}
      error={model.error}
      vote={model.vote}
      dismiss={model.dismiss}
      t={model.t}
    />
  );
}
