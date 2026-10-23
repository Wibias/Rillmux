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
  onWatch?: (stream: HelixStream) => void;
  onTogglePin?: (login: string) => void;
  onClose: () => void;
}

export function StreamActionsMenuPanel({
  menuId,
  panelRef,
  position,
  stream,
  pinned,
  onWatch,
  onTogglePin,
  onClose,
}: PanelProps) {
  const { t } = useTranslation(["common", "routes"]);
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
    </div>
  );
}
