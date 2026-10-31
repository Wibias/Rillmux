import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { EmbeddedChat } from "../../components/EmbeddedChat";
import { PageSubbar } from "../../components/AppShell";
import { useWatchingStore } from "../../lib/streaming/store";
import { watchingPhase, watchingStatusText } from "../../lib/streaming/status";
import { GripIcon } from "../../components/FollowedIcons";
import { useSettingsStore } from "../../lib/settings/store";

export function WatchingPage() {
  const { t } = useTranslation(["routes", "common"]);
  const sessions = useWatchingStore((s) => s.sessions);
  const slotChannels = useWatchingStore((s) => s.slotChannels);
  const refresh = useWatchingStore((s) => s.refresh);
  const stopSession = useWatchingStore((s) => s.stopSession);
  const stopAll = useWatchingStore((s) => s.stopAll);
  const toggleMute = useWatchingStore((s) => s.toggleMute);
  const moveSlot = useWatchingStore((s) => s.moveSlot);
  const activeChatChannel = useWatchingStore((s) => s.activeChatChannel);
  const setActiveChat = useWatchingStore((s) => s.setActiveChat);
  const settings = useSettingsStore((s) => s.settings);
  const chatProvider = settings.chat.provider;
  const multi = !settings.streaming.seamlessSwitch;
  const launchError = useWatchingStore((s) => s.error);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const orderedSessions = multi
    ? slotChannels
        .map((ch) =>
          sessions.find((s) => s.channel.toLowerCase() === ch && s.running),
        )
        .filter((s): s is NonNullable<typeof s> => Boolean(s))
    : sessions;

  return (
    <section className="watching-layout">
      <div className="watching-layout__main">
        <PageSubbar
          title={t("routes:watchingTitle")}
          lede={t("routes:watchingLede")}
          actions={
            sessions.length ? (
              <button
                type="button"
                className="button-secondary"
                onClick={() => void stopAll()}
              >
                {t("routes:watchingStopAll")}
              </button>
            ) : undefined
          }
        />
        {launchError ? <p className="muted">{launchError}</p> : null}
        {!sessions.length ? (
          <div className="empty-panel">
            <strong>{t("routes:watchingEmpty")}</strong>
          </div>
        ) : null}
        <ul className="watching-list">
          {orderedSessions.map((session, index) => {
            const phase = watchingPhase(session.phase);
            const status = watchingStatusText(phase, session.status, t);
            return (
            <li key={session.id} className="watching-list__item">
              <span className="watching-list__handle" aria-hidden>
                <GripIcon />
              </span>
              <div className="watching-list__meta">
                <div className="watching-list__title">
                  {multi ? <span className="muted">#{index + 1} </span> : null}
                  <strong>{session.channel}</strong>
                  {session.game ? (
                    <span className="muted"> • {session.game}</span>
                  ) : null}
                </div>
                {status ? (
                  <p
                    className={`watching-list__status watching-list__status--${phase}`}
                    title={session.status}
                  >
                    {status}
                  </p>
                ) : null}
              </div>
              <div className="watching-list__actions">
                {multi ? (
                  <>
                    <button
                      type="button"
                      className="button-secondary"
                      disabled={index === 0}
                      onClick={() => moveSlot(session.channel, -1)}
                      aria-label="Move up"
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      className="button-secondary"
                      disabled={index >= orderedSessions.length - 1}
                      onClick={() => moveSlot(session.channel, 1)}
                      aria-label="Move down"
                    >
                      ↓
                    </button>
                  </>
                ) : null}
                {chatProvider === "embedded" ? (
                  <button
                    type="button"
                    className="button-secondary"
                    onClick={() => setActiveChat(session.channel)}
                  >
                    {t("routes:chatTitle", { channel: session.channel })}
                  </button>
                ) : null}
                <button
                  type="button"
                  className="button-secondary"
                  aria-pressed={Boolean(session.muted)}
                  onClick={() => void toggleMute(session.id)}
                >
                  {session.muted ? "Unmute" : "Mute"}
                </button>
                <button type="button" onClick={() => void stopSession(session.id)}>
                  {t("common:stop")}
                </button>
              </div>
            </li>
            );
          })}
        </ul>
      </div>
      {chatProvider === "embedded" ? (
        <EmbeddedChat channel={activeChatChannel ?? sessions[0]?.channel ?? null} />
      ) : null}
    </section>
  );
}

