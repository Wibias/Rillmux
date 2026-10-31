import { useChannelPointsPollOverlay } from "./pollOverlay/hook";
import { ChannelPointsPredictionCard } from "./pollOverlay/PredictionCard";
import { ChannelPointsPollCard } from "./pollOverlay/PollCard";
import "./ChannelPointsPollOverlay.css";

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

