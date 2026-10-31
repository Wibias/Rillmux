import type { MultistreamPageModel } from "./model";
import { GripIcon, VerifiedBadge } from "../../components/FollowedIcons";
import { watchingPhase, watchingStatusText } from "../../lib/streaming/status";
import { slotShiftY } from "../../lib/streaming/reorder";
import { isTwitchPartner } from "../../lib/twitch/helix";

export function MultistreamSlotList({
  t,
  slotChannels,
  sessionByChannel,
  slotUsers,
  dragIndex,
  overIndex,
  dragStrideRef,
  slotsRef,
  ghostRef,
  onSlotPointerDown,
  onSlotPointerMove,
  endSlotDrag,
  activeChatChannel,
  chatProvider,
  setActiveChat,
  toggleMute,
  stopSession,
  draggingChannel,
  draggingUser,
  draggingSession,
  draggingPhase,
  draggingStatus,
}: MultistreamPageModel) {
  return (
      <div className="ms-section">
        <div className="ms-section__head">
          <h2>{t("multistream:currentStreams")}</h2>
          <span className="muted">{t("multistream:dragHint")}</span>
        </div>
        {!slotChannels.length ? (
          <p className="muted">{t("multistream:currentEmpty")}</p>
        ) : (
          <>
          <ul
            className={["ms-slots", dragIndex !== null ? "ms-slots--dragging" : ""]
              .filter(Boolean)
              .join(" ")}
            ref={slotsRef}
          >
            {slotChannels.map((channel, index) => {
              const s = sessionByChannel.get(channel);
              const user = slotUsers[channel];
              const phase = watchingPhase(s?.phase);
              const status = watchingStatusText(phase, s?.status, t);
              const chatActive =
                (activeChatChannel ?? slotChannels[0]) === channel;
              const shift =
                dragIndex !== null && overIndex !== null
                  ? slotShiftY(
                      index,
                      dragIndex,
                      overIndex,
                      dragStrideRef.current,
                    )
                  : 0;
              return (
                <li
                  key={channel}
                  data-ms-slot={index}
                  className={[
                    "ms-slot",
                    dragIndex === index ? "ms-slot--placeholder" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  style={
                    dragIndex !== null
                      ? { transform: `translateY(${shift}px)` }
                      : undefined
                  }
                  onPointerDown={(event) => onSlotPointerDown(event, index)}
                  onPointerMove={onSlotPointerMove}
                  onPointerUp={(event) =>
                    endSlotDrag(event.pointerId, event.currentTarget)
                  }
                  onPointerCancel={(event) =>
                    endSlotDrag(event.pointerId, event.currentTarget)
                  }
                  onDragStart={(event) => event.preventDefault()}
                >
                  <span className="ms-slot__handle" aria-hidden>
                    <GripIcon />
                  </span>
                  {user?.profile_image_url ? (
                    <img
                      src={user.profile_image_url}
                      alt=""
                      className="ms-slot__avatar"
                      draggable={false}
                    />
                  ) : (
                    <span className="ms-slot__avatar ms-slot__avatar--empty" />
                  )}
                  <div className="ms-slot__meta">
                    <div className="ms-slot__title">
                      <strong>{user?.display_name ?? s?.channel ?? channel}</strong>
                      {isTwitchPartner(user) ? <VerifiedBadge /> : null}
                      {s?.game ? (
                        <span className="muted"> • {s.game}</span>
                      ) : null}
                    </div>
                    {status ? (
                      <p
                        className={`ms-slot__status ms-slot__status--${phase}`}
                        title={s?.status}
                      >
                        {status}
                      </p>
                    ) : null}
                  </div>
                  <div className="ms-slot__actions">
                    {chatProvider === "embedded" ? (
                      <button
                        type="button"
                        className={`button-secondary${chatActive ? " ms-chat-active" : ""}`}
                        aria-pressed={chatActive}
                        onClick={() => setActiveChat(channel)}
                      >
                        {chatActive
                          ? t("multistream:chatActive")
                          : t("multistream:chatPick")}
                      </button>
                    ) : null}
                    {s ? (
                      <>
                        <button
                          type="button"
                          className={`button-secondary${s.muted ? " ms-muted" : ""}`}
                          aria-pressed={Boolean(s.muted)}
                          title={
                            s.muted
                              ? t("multistream:unmute")
                              : t("multistream:mute")
                          }
                          onClick={() => void toggleMute(s.id)}
                        >
                          {s.muted
                            ? t("multistream:unmute")
                            : t("multistream:mute")}
                        </button>
                        <button
                          type="button"
                          onClick={() => void stopSession(s.id)}
                        >
                          {t("multistream:stop")}
                        </button>
                      </>
                    ) : null}
                  </div>
                </li>
              );
            })}
          </ul>
          {dragIndex !== null && draggingChannel ? (
            <div
              ref={ghostRef}
              className="ms-slot ms-slot-ghost"
              aria-hidden
            >
              <span className="ms-slot__handle">
                <GripIcon />
              </span>
              {draggingUser?.profile_image_url ? (
                <img
                  src={draggingUser.profile_image_url}
                  alt=""
                  className="ms-slot__avatar"
                  draggable={false}
                />
              ) : (
                <span className="ms-slot__avatar ms-slot__avatar--empty" />
              )}
              <div className="ms-slot__meta">
                <div className="ms-slot__title">
                  <strong>
                    {draggingUser?.display_name ??
                      draggingSession?.channel ??
                      draggingChannel}
                  </strong>
                  {isTwitchPartner(draggingUser) ? <VerifiedBadge /> : null}
                  {draggingSession?.game ? (
                    <span className="muted"> • {draggingSession.game}</span>
                  ) : null}
                </div>
                {draggingStatus ? (
                  <p
                    className={`ms-slot__status ms-slot__status--${draggingPhase}`}
                  >
                    {draggingStatus}
                  </p>
                ) : null}
              </div>
            </div>
          ) : null}
          </>
        )}
        {chatProvider === "chatterino" && slotChannels.length ? (
          <p className="muted ms-chatterino-note">
            {t("multistream:chatterinoNote")}
          </p>
        ) : null}
      </div>
  );
}

