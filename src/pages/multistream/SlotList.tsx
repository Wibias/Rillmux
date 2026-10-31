import type { MultistreamPageModel } from "./model";
import { GripIcon, VerifiedBadge } from "../../components/FollowedIcons";
import {
  watchingPhase,
  watchingStatusText,
} from "../../lib/streaming/status";
import { slotShiftY } from "../../lib/streaming/reorder";
import { isTwitchPartner } from "../../lib/twitch/helix";

function SlotMeta({
  model,
  channel,
}: {
  model: MultistreamPageModel;
  channel: string;
}) {
  const { t, sessionByChannel, slotUsers } = model;
  const session = sessionByChannel.get(channel);
  const user = slotUsers[channel];
  const phase = watchingPhase(session?.phase);
  const status = watchingStatusText(phase, session?.status, t);

  return (
    <>
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
          <strong>{user?.display_name ?? session?.channel ?? channel}</strong>
          {isTwitchPartner(user) ? <VerifiedBadge /> : null}
          {session?.game ? <span className="muted"> • {session.game}</span> : null}
        </div>
        {status ? (
          <p
            className={`ms-slot__status ms-slot__status--${phase}`}
            title={session?.status}
          >
            {status}
          </p>
        ) : null}
      </div>
    </>
  );
}

function SlotActions({
  model,
  channel,
}: {
  model: MultistreamPageModel;
  channel: string;
}) {
  const {
    t,
    sessionByChannel,
    slotChannels,
    activeChatChannel,
    chatProvider,
    setActiveChat,
    toggleMute,
    stopSession,
  } = model;
  const session = sessionByChannel.get(channel);
  const chatActive = (activeChatChannel ?? slotChannels[0]) === channel;

  return (
    <div className="ms-slot__actions">
      {chatProvider === "embedded" ? (
        <button
          type="button"
          className={`button-secondary${chatActive ? " ms-chat-active" : ""}`}
          aria-pressed={chatActive}
          onClick={() => setActiveChat(channel)}
        >
          {chatActive ? t("multistream:chatActive") : t("multistream:chatPick")}
        </button>
      ) : null}
      {session ? (
        <>
          <button
            type="button"
            className={`button-secondary${session.muted ? " ms-muted" : ""}`}
            aria-pressed={Boolean(session.muted)}
            title={
              session.muted
                ? t("multistream:unmute")
                : t("multistream:mute")
            }
            onClick={() => void toggleMute(session.id)}
          >
            {session.muted ? t("multistream:unmute") : t("multistream:mute")}
          </button>
          <button type="button" onClick={() => void stopSession(session.id)}>
            {t("multistream:stop")}
          </button>
        </>
      ) : null}
    </div>
  );
}

function SlotRow({
  model,
  channel,
  index,
}: {
  model: MultistreamPageModel;
  channel: string;
  index: number;
}) {
  const {
    dragIndex,
    overIndex,
    dragStrideRef,
    onSlotPointerDown,
    onSlotPointerMove,
    endSlotDrag,
  } = model;
  const shift =
    dragIndex !== null && overIndex !== null
      ? slotShiftY(index, dragIndex, overIndex, dragStrideRef.current)
      : 0;

  return (
    <li
      data-ms-slot={index}
      className={[
        "ms-slot",
        dragIndex === index ? "ms-slot--placeholder" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      style={
        dragIndex !== null ? { transform: `translateY(${shift}px)` } : undefined
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
      <SlotMeta model={model} channel={channel} />
      <SlotActions model={model} channel={channel} />
    </li>
  );
}

function DraggingGhost({ model }: { model: MultistreamPageModel }) {
  const {
    dragIndex,
    draggingChannel,
    draggingUser,
    draggingSession,
    draggingPhase,
    draggingStatus,
    ghostRef,
  } = model;
  if (dragIndex === null || !draggingChannel) return null;

  return (
    <div ref={ghostRef} className="ms-slot ms-slot-ghost" aria-hidden>
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
          <p className={`ms-slot__status ms-slot__status--${draggingPhase}`}>
            {draggingStatus}
          </p>
        ) : null}
      </div>
    </div>
  );
}

function SlotCollection({ model }: { model: MultistreamPageModel }) {
  const { slotChannels, dragIndex, slotsRef } = model;
  if (!slotChannels.length) return null;

  return (
    <>
      <ul
        className={[
          "ms-slots",
          dragIndex !== null ? "ms-slots--dragging" : "",
        ]
          .filter(Boolean)
          .join(" ")}
        ref={slotsRef}
      >
        {slotChannels.map((channel, index) => (
          <SlotRow
            key={channel}
            model={model}
            channel={channel}
            index={index}
          />
        ))}
      </ul>
      <DraggingGhost model={model} />
    </>
  );
}

export function MultistreamSlotList(model: MultistreamPageModel) {
  const { t, slotChannels, chatProvider } = model;
  return (
    <div className="ms-section">
      <div className="ms-section__head">
        <h2>{t("multistream:currentStreams")}</h2>
        <span className="muted">{t("multistream:dragHint")}</span>
      </div>
      {slotChannels.length ? (
        <SlotCollection model={model} />
      ) : (
        <p className="muted">{t("multistream:currentEmpty")}</p>
      )}
      {chatProvider === "chatterino" && slotChannels.length ? (
        <p className="muted ms-chatterino-note">
          {t("multistream:chatterinoNote")}
        </p>
      ) : null}
    </div>
  );
}
