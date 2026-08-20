import { useEffect, useId, useRef, useState } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { HelixStream } from "../lib/twitch/helix";
import { twitchChannelUrl } from "../lib/browse/format";
import { isTauri } from "../lib/tauri";
import "./StreamActionsMenu.css";

async function openExternal(url: string) {
  if (isTauri()) {
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export function StreamActionsMenu({
  stream,
  pinned = false,
  onWatch,
  onTogglePin,
}: {
  stream: HelixStream;
  pinned?: boolean;
  onWatch?: (stream: HelixStream) => void;
  onTogglePin?: (login: string) => void;
}) {
  const { t } = useTranslation(["common", "routes"]);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const menuId = `${useId()}-stream-menu`;

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="stream-menu" ref={rootRef}>
      <button
        type="button"
        className="stream-menu__trigger"
        aria-expanded={open}
        aria-haspopup="menu"
        aria-controls={menuId}
        aria-label={t("routes:streamMore")}
        onClick={(event) => {
          event.stopPropagation();
          setOpen((value) => !value);
        }}
      >
        <span aria-hidden="true">⋮</span>
      </button>
      {open ? (
        <div id={menuId} className="stream-menu__panel" role="menu">
          {onWatch ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                onWatch(stream);
              }}
            >
              {t("common:watch")}
            </button>
          ) : null}
          <Link
            role="menuitem"
            to={`/channel/${stream.user_login}`}
            onClick={() => setOpen(false)}
          >
            {t("routes:streamOpenChannel")}
          </Link>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              void openExternal(twitchChannelUrl(stream.user_login));
            }}
          >
            {t("routes:streamOpenTwitch")}
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
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
                setOpen(false);
                onTogglePin(stream.user_login);
              }}
            >
              {pinned ? t("routes:streamUnpin") : t("routes:streamPin")}
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
