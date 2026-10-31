import { useTranslation } from "react-i18next";
import { ChannelPointsPoll } from "./types";

export function ChannelPointsPollCard({
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

