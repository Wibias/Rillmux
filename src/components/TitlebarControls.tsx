import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { invoke, isTauri } from "../lib/tauri";

export function TitlebarControls() {
  const { t } = useTranslation("common");
  const [maximized, setMaximized] = useState(false);
  const snapTimer = useRef<number | null>(null);

  const cancelSnap = () => {
    if (snapTimer.current !== null) {
      window.clearTimeout(snapTimer.current);
      snapTimer.current = null;
    }
  };

  // decorum raises the Windows snap layout flyout for a DOM caption button by
  // simulating its Win+Z hotkey; there is no native button left to hover.
  const armSnap = () => {
    cancelSnap();
    snapTimer.current = window.setTimeout(() => {
      snapTimer.current = null;
      void getCurrentWindow()
        .setFocus()
        .then(() => invoke("plugin:decorum|show_snap_overlay"));
    }, 620);
  };

  useEffect(() => {
    if (!isTauri()) return;
    const win = getCurrentWindow();
    let disposed = false;
    void win.isMaximized().then((value) => {
      if (!disposed) setMaximized(value);
    });
    const unlisten = win.onResized(() => {
      void win.isMaximized().then((value) => {
        if (!disposed) setMaximized(value);
      });
    });
    return () => {
      disposed = true;
      if (snapTimer.current !== null) window.clearTimeout(snapTimer.current);
      void unlisten.then((stop) => stop());
    };
  }, []);

  if (!isTauri()) return null;

  const win = getCurrentWindow();
  return (
    <div className="shell__win-controls">
      <button
        type="button"
        aria-label={t("windowMinimize")}
        title={t("windowMinimize")}
        onClick={() => void win.minimize()}
      >
        <svg viewBox="0 0 12 12" width="16" height="16" aria-hidden>
          <path fill="currentColor" d="M2 6.25h8v1H2z" />
        </svg>
      </button>
      <button
        type="button"
        aria-label={maximized ? t("windowRestore") : t("windowMaximize")}
        title={maximized ? t("windowRestore") : t("windowMaximize")}
        onMouseEnter={armSnap}
        onMouseLeave={cancelSnap}
        onClick={() => {
          cancelSnap();
          void win.toggleMaximize();
        }}
      >
        {maximized ? (
          <svg viewBox="0 0 12 12" width="16" height="16" aria-hidden>
            <path
              fill="none"
              stroke="currentColor"
              strokeWidth="1.1"
              d="M3.5 4.5h5v5h-5zM4.5 4.5V3.2h5.3V8.5H8.5"
            />
          </svg>
        ) : (
          <svg viewBox="0 0 12 12" width="16" height="16" aria-hidden>
            <rect
              x="2.5"
              y="2.5"
              width="7"
              height="7"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.1"
            />
          </svg>
        )}
      </button>
      <button
        type="button"
        className="shell__win-close"
        aria-label={t("windowClose")}
        title={t("windowClose")}
        onClick={() => void invoke("app_quit")}
      >
        <svg viewBox="0 0 12 12" width="16" height="16" aria-hidden>
          <path
            fill="none"
            stroke="currentColor"
            strokeWidth="1.2"
            d="M3 3l6 6M9 3L3 9"
          />
        </svg>
      </button>
    </div>
  );
}
