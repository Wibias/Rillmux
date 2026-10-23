import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  followChannel,
  isFollowingChannel,
  unfollowChannel,
  type HelixStream,
} from "../lib/twitch/helix";
import { followedStreamsQueryKey } from "../lib/notifications/followedLive";
import { useAuthStore } from "../lib/auth/store";
import { StreamActionsMenuPanel } from "./StreamActionsMenuPanel";
import { useStreamMenuPosition } from "./useStreamMenuPosition";
import "./StreamActionsMenu.css";

const FOLLOW_SCOPE = "user:edit:follows";

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
  const { t } = useTranslation(["routes"]);
  const queryClient = useQueryClient();
  const session = useAuthStore((s) => s.session);
  const startLogin = useAuthStore((s) => s.startLogin);
  const [open, setOpen] = useState(false);
  const [confirmingUnfollow, setConfirmingUnfollow] = useState(false);
  const [pending, setPending] = useState(false);
  const [actionError, setActionError] = useState(false);
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
  const position = useStreamMenuPosition(open, triggerRef, panelRef);

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
    if (!open) {
      setConfirmingUnfollow(false);
      setActionError(false);
      return;
    }
    const timer = window.setTimeout(() => {
      panelRef.current
        ?.querySelector<HTMLElement>('[role="menuitem"]')
        ?.focus();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!confirmingUnfollow) return;
    panelRef.current
      ?.querySelector<HTMLElement>(".stream-menu__confirm-actions button")
      ?.focus();
  }, [confirmingUnfollow]);

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
      {open
        ? createPortal(
            <StreamActionsMenuPanel
              menuId={menuId}
              panelRef={panelRef}
              position={position}
              stream={stream}
              pinned={pinned}
              confirmingUnfollow={confirmingUnfollow}
              pending={pending}
              actionError={actionError}
              following={following}
              followStatePending={followState.isPending}
              showFollowPermission={Boolean(userId) && !canFollow}
              showFollowAction={canFollow}
              onWatch={onWatch}
              onTogglePin={onTogglePin}
              onClose={() => setOpen(false)}
              onStartLogin={() => {
                setOpen(false);
                void startLogin();
              }}
              onFollowToggle={() => {
                if (following) {
                  setActionError(false);
                  setConfirmingUnfollow(true);
                } else {
                  void runFollowAction(true);
                }
              }}
              onCancelUnfollow={() => setConfirmingUnfollow(false)}
              onConfirmUnfollow={() => void runFollowAction(false)}
            />,
            document.body,
          )
        : null}
    </div>
  );
}
