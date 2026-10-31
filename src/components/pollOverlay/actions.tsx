import { hostState } from "./state";
import { type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isClosedPredictionError, mergeConfirmedPredictionVoteSnapshot, overlayLiveEventId, predictionAcceptsVotes, type ConfirmedPredictionVote } from "../../lib/streaming/pollOverlay";
import { invoke, isTauri } from "../../lib/tauri";
import { ChannelPointsPoll, ChannelPointsPrediction, ChannelPointsSnapshot, PollOverlayPredictionVoteConfirmed } from "./types";

export type OverlaySnapshotSet = Dispatch<SetStateAction<ChannelPointsSnapshot | null>>;

export async function votePollChoice(args: {
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

export async function votePredictionOutcome(args: {
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
        hostState.lastHostSnapshot = merged;
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

export async function dismissPollOverlay(args: {
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

