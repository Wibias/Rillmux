import type { RefObject } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { HelixStream } from "../lib/twitch/helix";
import { twitchChannelUrl } from "../lib/browse/format";
import { isTauri } from "../lib/tauri";
import type { StreamMenuPosition } from "./useStreamMenuPosition";

async function openExternal(url: string) {
  if (isTauri()) {
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

interface PanelProps {
  menuId: string;
  panelRef: RefObject<HTMLDivElement | null>;
  position: StreamMenuPosition | null;
  stream: HelixStream;
  pinned: boolean;
  confirmingUnfollow: boolean;
  pending: boolean;
  actionError: boolean;
  following: boolean;
  followStatePending: boolean;
  showFollowPermission: boolean;
  showFollowAction: boolean;
  onWatch?: (stream: HelixStream) => void;
  onTogglePin?: (login: string) => void;
  onClose: () => void;
  onStartLogin: () => void;
  onFollowToggle: () => void;
  onCancelUnfollow: () => void;
  onConfirmUnfollow: () => void;
}

export function StreamActionsMenuPanel({
  menuId,
  panelRef,
  position,
  stream,
  pinned,
  confirmingUnfollow,
  pending,
  actionError,
  following,
  followStatePending,
  showFollowPermission,
  showFollowAction,
  onWatch,
  onTogglePin,
  onClose,
  onStartLogin,
  onFollowToggle,
  onCancelUnfollow,
  onConfirmUnfollow,
}: PanelProps) {
  return (
    <div
      id={menuId}
      ref={panelRef}
      className="stream-menu__panel stream-menu__panel--floating"
      role="menu"
      style={
        position
          ? { top: `${position.top}px`, left: `${position.left}px` }
          : undefined
      }
      data-positioned={position ? "true" : "false"}
    >
      {confirmingUnfollow ? (
        <UnfollowConfirm
          channel={stream.user_name}
          pending={pending}
          onCancel={onCancelUnfollow}
          onConfirm={onConfirmUnfollow}
        />
      ) : (
        <StreamMenuItems
          stream={stream}
          pinned={pinned}
          pending={pending}
          actionError={actionError}
          following={following}
          followStatePending={followStatePending}
          showFollowPermission={showFollowPermission}
          showFollowAction={showFollowAction}
          onWatch={onWatch}
          onTogglePin={onTogglePin}
          onClose={onClose}
          onStartLogin={onStartLogin}
          onFollowToggle={onFollowToggle}
        />
      )}
    </div>
  );
}

function UnfollowConfirm({
  channel,
  pending,
  onCancel,
  onConfirm,
}: {
  channel: string;
  pending: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation(["common", "routes"]);
  return (
    <div className="stream-menu__confirm">
      <p className="stream-menu__confirm-text">
        {t("routes:streamUnfollowConfirm", { channel })}
      </p>
      <div className="stream-menu__confirm-actions">
        <button type="button" onClick={onCancel} disabled={pending}>
          {t("common:cancel")}
        </button>
        <button
          type="button"
          className="stream-menu__danger"
          onClick={onConfirm}
          disabled={pending}
        >
          {t("routes:streamUnfollow")}
        </button>
      </div>
    </div>
  );
}

function StreamMenuItems({
  stream,
  pinned,
  pending,
  actionError,
  following,
  followStatePending,
  showFollowPermission,
  showFollowAction,
  onWatch,
  onTogglePin,
  onClose,
  onStartLogin,
  onFollowToggle,
}: Omit<
  PanelProps,
  | "menuId"
  | "panelRef"
  | "position"
  | "confirmingUnfollow"
  | "onCancelUnfollow"
  | "onConfirmUnfollow"
>) {
  const { t } = useTranslation(["common", "routes"]);
  return (
    <>
      {onWatch ? (
        <button
          type="button"
          role="menuitem"
          onClick={() => {
            onClose();
            onWatch(stream);
          }}
        >
          {t("common:watch")}
        </button>
      ) : null}
      <Link
        role="menuitem"
        to={`/channel/${stream.user_login}`}
        onClick={onClose}
      >
        {t("routes:streamOpenChannel")}
      </Link>
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          onClose();
          void openExternal(twitchChannelUrl(stream.user_login));
        }}
      >
        {t("routes:streamOpenTwitch")}
      </button>
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          onClose();
          void navigator.clipboard.writeText(
            twitchChannelUrl(stream.user_login),
          );
        }}
      >
        {t("routes:streamCopyUrl")}
      </button>
      {showFollowPermission ? (
        <button
          type="button"
          role="menuitem"
          onClick={() => {
            onClose();
            onStartLogin();
          }}
        >
          {t("routes:streamFollowPermission")}
        </button>
      ) : null}
      {showFollowAction ? (
        <button
          type="button"
          role="menuitem"
          disabled={pending || followStatePending}
          onClick={onFollowToggle}
        >
          {followStatePending
            ? t("common:loading")
            : following
            ? t("routes:streamUnfollow")
            : t("routes:streamFollow")}
        </button>
      ) : null}
      {onTogglePin ? (
        <button
          type="button"
          role="menuitem"
          onClick={() => {
            onClose();
            onTogglePin(stream.user_login);
          }}
        >
          {pinned ? t("routes:streamUnpin") : t("routes:streamPin")}
        </button>
      ) : null}
      {actionError ? (
        <p className="stream-menu__error" role="alert">
          {following
            ? t("routes:streamUnfollowFailed")
            : t("routes:streamFollowFailed")}
        </p>
      ) : null}
    </>
  );
}
