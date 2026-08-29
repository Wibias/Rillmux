import { describe, expect, it } from "vitest";
import {
  POLL_FALLBACK_REFRESH_MS,
  POLL_OVERLAY_READY_MAX_ATTEMPTS,
  applyConfirmedPredictionVote,
  isClosedPredictionError,
  mergeConfirmedPredictionVoteSnapshot,
  nextPollOverlayReadyAttempt,
  overlayRectMoved,
  pollOverlayRect,
  pollOverlayShouldPollGql,
  predictionAcceptsVotes,
  predictionOverlayVisible,
  PREDICTION_LOCKED_DISMISS_MS,
  lockedPredictionDismissAfterMs,
} from "./pollOverlay";

describe("pollOverlayRect", () => {
  it("sits at the bottom of owned chat, not the video host", () => {
    expect(
      pollOverlayRect(
        { x: 810, y: 20, width: 300, height: 450 },
        { x: 0, y: 0, width: 1280, height: 800 },
      ),
    ).toEqual({ x: 822, y: 118, width: 276, height: 340 });
  });

  it("falls back to the main-window chat column when Chatterino is absent", () => {
    expect(
      pollOverlayRect(null, { x: 900, y: 80, width: 340, height: 700 }),
    ).toEqual({ x: 912, y: 428, width: 316, height: 340 });
  });

  it("does not invent a host when chat is not on screen", () => {
    expect(pollOverlayRect(null, null)).toBeNull();
  });

  it("ignores tiny overlay jitter so the window is not moved every tick", () => {
    const a = { x: 800, y: 100, width: 300, height: 240 };
    expect(overlayRectMoved(a, { ...a, x: 801, y: 102 })).toBe(false);
    expect(overlayRectMoved(a, { ...a, x: 820 })).toBe(true);
  });
});

describe("poll overlay GQL", () => {
  it("falls back quickly enough to catch a short poll when Hermes push is unavailable", () => {
    expect(POLL_FALLBACK_REFRESH_MS).toBeLessThanOrEqual(15_000);
    expect(pollOverlayShouldPollGql(false)).toBe(true);
    expect(pollOverlayShouldPollGql(true)).toBe(false);
  });
});

describe("prediction vote window", () => {
  it("keeps a locked prediction visible until the short recap delay elapses", () => {
    expect(predictionOverlayVisible("ACTIVE")).toBe(true);
    expect(predictionOverlayVisible("LOCKED")).toBe(true);
    expect(predictionOverlayVisible("RESOLVED")).toBe(false);
    expect(lockedPredictionDismissAfterMs("ACTIVE")).toBeNull();
    expect(lockedPredictionDismissAfterMs("LOCKED")).toBe(
      PREDICTION_LOCKED_DISMISS_MS,
    );
    expect(PREDICTION_LOCKED_DISMISS_MS).toBe(5_000);
  });

  it("locks voting when the prediction is LOCKED or the window has elapsed", () => {
    expect(
      predictionAcceptsVotes({
        status: "LOCKED",
        createdAt: "2026-08-20T20:00:00Z",
        windowSeconds: 120,
      }),
    ).toBe(false);
    expect(
      predictionAcceptsVotes({
        status: "ACTIVE",
        createdAt: new Date().toISOString(),
        windowSeconds: 120,
      }),
    ).toBe(true);
  });

  it("treats a closed-prediction API error as overlay cleanup, not a schema bug", () => {
    expect(
      isClosedPredictionError(
        'Cannot query field "predictionEvent" on type "MakePredictionPayload".',
      ),
    ).toBe(true);
    expect(
      isClosedPredictionError("This prediction is no longer accepting votes"),
    ).toBe(true);
    expect(isClosedPredictionError("Not enough Channel Points to make that prediction")).toBe(
      false,
    );
  });
});

describe("confirmed prediction participation", () => {
  const prediction = {
    id: "prediction-1",
    status: "ACTIVE",
    predictedOutcomeId: null,
    predictedPoints: null,
  };

  it("restores a successful local vote when a later snapshot omits participation", () => {
    expect(
      applyConfirmedPredictionVote(prediction, {
        eventId: "prediction-1",
        outcomeId: "outcome-blue",
        points: 10_500,
      }),
    ).toEqual({
      ...prediction,
      predictedOutcomeId: "outcome-blue",
      predictedPoints: 10_500,
    });
  });

  it("does not carry a local vote into another or removed prediction", () => {
    const confirmed = {
      eventId: "prediction-1",
      outcomeId: "outcome-blue",
      points: 10_500,
    };
    expect(applyConfirmedPredictionVote({ ...prediction, id: "prediction-2" }, confirmed)).toEqual({
      ...prediction,
      id: "prediction-2",
    });
    expect(applyConfirmedPredictionVote(null, confirmed)).toBeNull();
  });

  it("builds one confirmed snapshot that can be shared with the peer window", () => {
    const snapshot = {
      channelLogin: "xthesolutiontv",
      balance: 61_634,
      prediction: null,
    };
    expect(
      mergeConfirmedPredictionVoteSnapshot(snapshot, prediction, {
        eventId: "prediction-1",
        outcomeId: "outcome-blue",
        points: 10_500,
      }),
    ).toEqual({
      ...snapshot,
      prediction: {
        ...prediction,
        predictedOutcomeId: "outcome-blue",
        predictedPoints: 10_500,
      },
    });
  });
});

describe("poll overlay ready handshake", () => {
  it("retries a lost ready event until state is acknowledged", () => {
    expect(nextPollOverlayReadyAttempt(0, false)).toBe(1);
    expect(nextPollOverlayReadyAttempt(1, false)).toBe(2);
    expect(nextPollOverlayReadyAttempt(2, true)).toBeNull();
  });

  it("bounds ready retries when the host never acknowledges", () => {
    expect(nextPollOverlayReadyAttempt(POLL_OVERLAY_READY_MAX_ATTEMPTS, false)).toBeNull();
  });
});
