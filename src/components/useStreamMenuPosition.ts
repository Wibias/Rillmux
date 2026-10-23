import { useLayoutEffect, useState, type RefObject } from "react";

const PANEL_MARGIN = 8;

export interface StreamMenuPosition {
  top: number;
  left: number;
}

export function useStreamMenuPosition(
  open: boolean,
  triggerRef: RefObject<HTMLButtonElement | null>,
  panelRef: RefObject<HTMLDivElement | null>,
): StreamMenuPosition | null {
  const [position, setPosition] = useState<StreamMenuPosition | null>(null);

  useLayoutEffect(() => {
    if (!open) {
      setPosition(null);
      return;
    }

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
      setPosition((current) =>
        current?.top === top && current.left === left
          ? current
          : { top, left },
      );
    };

    update();
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(update);
    if (observer && panelRef.current) {
      observer.observe(panelRef.current);
    }
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open, panelRef, triggerRef]);

  return position;
}
