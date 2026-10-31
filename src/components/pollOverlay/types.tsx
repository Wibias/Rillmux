import { type ConfirmedPredictionVote } from "../../lib/streaming/pollOverlay";

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

export interface ChannelPointsSnapshot {
  channelLogin: string;
  balance: number;
  poll?: ChannelPointsPoll | null;
  prediction?: ChannelPointsPrediction | null;
}

export interface PollOverlayPredictionVoteConfirmed {
  channel: string;
  snapshot: ChannelPointsSnapshot;
  confirmed: ConfirmedPredictionVote;
}

export const OVERLAY_LABEL = "poll-overlay";
export const MIN_PREDICTION_POINTS = 10;
export const MAX_PREDICTION_POINTS = 250_000;

