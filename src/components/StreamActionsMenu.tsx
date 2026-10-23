import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  followChannel,
  isFollowingChannel,
  unfollowChannel,
  type HelixStream,
} from "../lib/twitch/helix";
import { twitchChannelUrl } from "../lib/browse/format";
import { followedStreamsQueryKey } from "../lib/notifications/followedLive";
import { useAuthStore } from "../lib/auth/store";
import { isTauri } from "../lib/tauri";
import "./StreamActionsMenu.css";

const FOLLOW_SCOPE = "user:edit:follows";
const PANEL_MARGIN = 8;

async function openExternal(url: string) {
  if (isTauri()) {
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

function followStateQueryKey(
  userId: string | null | undefined,
  broadcasterId: string,
) {
  return [
    "channel-follow-state",
    userId ?? "anonymous",
    broadcasterId,
  ] as const;
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
  const queryClient = useQueryClient();
  const session = useAuthStore((s) => s.session);
  const startLogin = useAuthStore((s) => s.startLogin);
  const [open, setOpen] = useState(false);
  const [confirmingUnfollow, setConfirmingUnfollow] = useState(false);
  const [pending, setPending] = useState(false);
  const [actionError, setActionError] = useState(false);
  const [position, setPosition] = useState<{
    top: number;
    left: number;
  } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const menuId = `${useId()}-stream-menu`;

  const userId = session?.loggedIn ? session.userId : null;
  const canFollow = Boolean(
    userId && session?.scopes.includes(FOLLOW_SCOPE),
  );
  const followState = useQuery({
    queryKey: followStateQueryKey(userId, stream.user_id),
    queryFn: () => isFollowingChannel(userId!, stream.user_id),
    enabled: open && Boolean(userId),
    staleTime: 60_000,
  });
  const following = followState.data === true;

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        !rootRef.current?.contains(target) &&
        !panelRef.current?.contains(target)
      ) {
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

  useEffect(() => {
    if (!confirmingUnfollow) return;
    panelRef.current
      ?.querySelector<HTMLElement>(".stream-menu__confirm-actions button")
      ?.focus();
  }, [confirmingUnfollow]);

  useEffect(() => {
    if (!open) {
      setConfirmingUnfollow(false);
      setActionError(false);
      setPosition(null);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const timer = window.setTimeout(() => {
      panelRef.current
        ?.querySelector<HTMLElement>('[role="menuitem"]')
        ?.focus();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [open]);

  useLayoutEffect(() => {
    if (!open) return;
    const update = () => {
      const trigger = triggerRef.current;
      const panel = panelRef.current;
      if (!trigger || !panel) return;
      const rect = trigger.getBoundingClientRect();
      const width = panel.offsetWidth;
      const height = panel.offsetHeight;
      const maxLeft = window.innerWidth - width - PANEL_MARGIN;
      const left = Math.max(
        PANEL_MARGIN,
        Math.min(rect.right - width, maxLeft),
      );
      let top = rect.bottom + 4;
      if (top + height > window.innerHeight - PANEL_MARGIN) {
        const above = rect.top - height - 4;
        top =
          above >= PANEL_MARGIN
            ? above
            : Math.max(
                PANEL_MARGIN,
                window.innerHeight - height - PANEL_MARGIN,
              );
      }
      setPosition({ top, left });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open, confirmingUnfollow, pending, actionError, followState.data]);

  const runFollowAction = async (next: boolean) => {
    if (!userId) return;
    setPending(true);
    setActionError(false);
    try {
      if (next) {
        await followChannel(userId, stream.user_id);
      } else {
        await unfollowChannel(userId, stream.user_id);
      }
      queryClient.setQueryData(
        followStateQueryKey(userId, stream.user_id),
        next,
      );
      await queryClient.invalidateQueries({
        queryKey: followedStreamsQueryKey(userId),
      });
      setConfirmingUnfollow(false);
      setOpen(false);
    } catch {
      setConfirmingUnfollow(false);
      setActionError(true);
    } finally {
      setPending(false);
    }
  };

  const panel = open ? (
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
        <div className="stream-menu__confirm">
          <p className="stream-menu__confirm-text">
            {t("routes:streamUnfollowConfirm", {
              channel: stream.user_name,
            })}
          </p>
          <div className="stream-menu__confirm-actions">
            <button
              type="button"
              onClick={() => setConfirmingUnfollow(false)}
              disabled={pending}
            >
              {t("common:cancel")}
            </button>
            <button
              type="button"
              className="stream-menu__danger"
              onClick={() => void runFollowAction(false)}
              disabled={pending}
            >
              {t("routes:streamUnfollow")}
            </button>
          </div>
        </div>
      ) : (
        <>
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
          {userId && !canFollow ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                void startLogin();
              }}
            >
              {t("routes:streamFollowPermission")}
            </button>
          ) : null}
          {canFollow ? (
            <button
              type="button"
              role="menuitem"
              disabled={pending || followState.isPending}
              onClick={() => {
                if (following) {
                  setActionError(false);
                  setConfirmingUnfollow(true);
                } else {
                  void runFollowAction(true);
                }
              }}
            >
              {followState.isPending
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
                setOpen(false);
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
      )}
    </div>
  ) : null;

  return (
    <div className="stream-menu" ref={rootRef}>
      <button
        type="button"
        ref={triggerRef}
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
      {panel ? createPortal(panel, document.body) : null}
    </div>
  );
}
