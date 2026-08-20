import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettingsStore } from "../lib/settings/store";
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

interface ChannelPointsSnapshot {
  channelLogin: string;
  balance: number;
  poll?: ChannelPointsPoll | null;
}

const POLL_REFRESH_MS = 8_000;

export function ChannelPointsPollOverlay() {
  const { t } = useTranslation("common");
  const enabled = useSettingsStore(
    (state) =>
      state.settings.streaming.channelPoints &&
      state.settings.streaming.channelPointsPolls,
  );
  const sessions = useWatchingStore((state) => state.sessions);
  const activeChatChannel = useWatchingStore((state) => state.activeChatChannel);
  const [snapshot, setSnapshot] = useState<ChannelPointsSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [votingId, setVotingId] = useState<string | null>(null);
  const dismissed = useRef<Set<string>>(new Set());

  const channel = useMemo(() => {
    const preferred = activeChatChannel?.toLowerCase();
    const running = sessions.filter((session) => session.running);
    const match = preferred
      ? running.find((session) => session.channel.toLowerCase() === preferred)
      : undefined;
    return (match ?? running[0])?.channel.toLowerCase() ?? null;
  }, [activeChatChannel, sessions]);

  useEffect(() => {
    if (!enabled || !isTauri() || !channel) {
      setSnapshot(null);
      setError(null);
      return;
    }
    let active = true;
    const refresh = async () => {
      try {
        const next = await invoke<ChannelPointsSnapshot>("channel_points_refresh", {
          channelLogin: channel,
        });
        if (!active) return;
        setSnapshot(next);
        setError(null);
      } catch (reason) {
        if (!active) return;
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_REFRESH_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [channel, enabled]);

  const poll = snapshot?.poll;
  if (!enabled || !channel || !poll || poll.status !== "ACTIVE") return null;
  if (dismissed.current.has(poll.id)) return null;

  async function vote(choiceId: string) {
    if (!poll || poll.votedChoiceId || votingId) return;
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

  return (
    <aside className="poll-overlay" role="dialog" aria-label={poll.title}>
      <header className="poll-overlay__head">
        <div>
          <strong>{t("pollVoteTitle", { channel })}</strong>
          <p className="poll-overlay__title">{poll.title}</p>
        </div>
        <button
          type="button"
          className="button-secondary"
          onClick={() => {
            dismissed.current.add(poll.id);
            setSnapshot((current) =>
              current ? { ...current, poll: null } : current,
            );
          }}
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
                className={voted ? "poll-overlay__choice is-voted" : "poll-overlay__choice"}
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
    </aside>
  );
}
