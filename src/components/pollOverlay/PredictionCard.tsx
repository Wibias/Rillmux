import { useTranslation } from "react-i18next";
import { ChannelPointsPrediction, MIN_PREDICTION_POINTS } from "./types";

export function ChannelPointsPredictionCard({
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

