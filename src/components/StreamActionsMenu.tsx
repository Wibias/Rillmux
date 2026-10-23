import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import type { HelixStream } from "../lib/twitch/helix";
import { StreamActionsMenuPanel } from "./StreamActionsMenuPanel";
import { useStreamMenuPosition } from "./useStreamMenuPosition";
import "./StreamActionsMenu.css";

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
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const menuId = `${useId()}-stream-menu`;
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
      return;
    }
    const timer = window.setTimeout(() => {
      panelRef.current
        ?.querySelector<HTMLElement>('[role="menuitem"]')
        ?.focus();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [open]);

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
              onWatch={onWatch}
              onTogglePin={onTogglePin}
              onClose={() => setOpen(false)}
            />,
            document.body,
          )
        : null}
    </div>
  );
}
