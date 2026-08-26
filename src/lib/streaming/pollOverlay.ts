import type { OverlayRect } from "./raid";

export type { OverlayRect };

export type PredictionVoteState = {
  status: string;
  createdAt?: string | null;
  windowSeconds?: number | null;
};

export type ConfirmedPredictionVote = {
  eventId: string;
  outcomeId: string;
  points: number;
};

type PredictionParticipationState = {
  id: string;
  predictedOutcomeId?: string | null;
  predictedPoints?: number | null;
};

type PredictionSnapshot<TPrediction extends PredictionParticipationState> = {
  prediction?: TPrediction | null;
};

export function applyConfirmedPredictionVote<T extends PredictionParticipationState>(
  prediction: T | null,
  confirmed: ConfirmedPredictionVote | null,
): T | null {
  if (!prediction || !confirmed || prediction.id !== confirmed.eventId) {
    return prediction;
  }
  if (
    prediction.predictedOutcomeId &&
    prediction.predictedOutcomeId !== confirmed.outcomeId
  ) {
    return prediction;
  }
  return {
    ...prediction,
    predictedOutcomeId: confirmed.outcomeId,
    predictedPoints: prediction.predictedPoints ?? confirmed.points,
  };
}

export function mergeConfirmedPredictionVoteSnapshot<
  TPrediction extends PredictionParticipationState,
  TSnapshot extends PredictionSnapshot<TPrediction>,
>(
  snapshot: TSnapshot,
  fallbackPrediction: TPrediction | null,
  confirmed: ConfirmedPredictionVote,
): TSnapshot & { prediction: TPrediction | null } {
  const responsePrediction = snapshot.prediction ?? null;
  const candidate =
    responsePrediction ??
    (fallbackPrediction?.id === confirmed.eventId ? fallbackPrediction : null);
  return {
    ...snapshot,
    prediction: applyConfirmedPredictionVote(candidate, confirmed),
  };
}

/** Safety net when Hermes poll/prediction pushes are unavailable. */
export const POLL_FALLBACK_REFRESH_MS = 10_000;

export const POLL_OVERLAY_READY_RETRY_MS = 250;
export const POLL_OVERLAY_READY_MAX_ATTEMPTS = 20;

export function nextPollOverlayReadyAttempt(
  attempts: number,
  acknowledged: boolean,
): number | null {
  if (acknowledged || attempts >= POLL_OVERLAY_READY_MAX_ATTEMPTS) return null;
  return attempts + 1;
}

export function pollOverlayShouldPollGql(overlayWindow: boolean): boolean {
  return !overlayWindow;
}

export function predictionRemainingSeconds(
  prediction: PredictionVoteState,
  now = Date.now(),
): number | null {
  if (prediction.status === "LOCKED") return 0;
  if (!prediction.createdAt || prediction.windowSeconds == null) return null;
  const end = Date.parse(prediction.createdAt) + prediction.windowSeconds * 1000;
  if (Number.isNaN(end)) return null;
  return Math.max(0, Math.floor((end - now) / 1000));
}

export function predictionAcceptsVotes(
  prediction: PredictionVoteState,
  now = Date.now(),
): boolean {
  if (prediction.status !== "ACTIVE") return false;
  return predictionRemainingSeconds(prediction, now) !== 0;
}

export function isClosedPredictionError(message: string): boolean {
  const text = message.toLowerCase();
  return (
    text.includes("no longer accepting") ||
    text.includes("predictionevent") ||
    text.includes("makepredictionpayload")
  );
}

const POLL_OVERLAY_WIDTH = 360;
const POLL_OVERLAY_HEIGHT = 340;
const POLL_OVERLAY_INSET = 12;

export function overlayRectMoved(
  a: OverlayRect,
  b: OverlayRect,
  slop = 4,
): boolean {
  return (
    Math.abs(a.x - b.x) > slop ||
    Math.abs(a.y - b.y) > slop ||
    Math.abs(a.width - b.width) > slop ||
    Math.abs(a.height - b.height) > slop
  );
}

/** Sit over owned Chatterino or the in-app chat column — never over video. */
export function pollOverlayRect(
  chat: OverlayRect | null,
  main: OverlayRect | null,
): OverlayRect | null {
  const host = chat ?? main;
  if (!host) return null;
  const width = Math.max(
    200,
    Math.min(POLL_OVERLAY_WIDTH, host.width - POLL_OVERLAY_INSET * 2),
  );
  const height = Math.max(
    160,
    Math.min(POLL_OVERLAY_HEIGHT, host.height - POLL_OVERLAY_INSET * 2),
  );
  return {
    x: Math.round(host.x + POLL_OVERLAY_INSET),
    y: Math.round(host.y + host.height - height - POLL_OVERLAY_INSET),
    width: Math.round(width),
    height: Math.round(height),
  };
}
