import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { isTauri } from "../lib/tauri";

export function TitlebarControls() {
  const { t } = useTranslation("common");
  const [maximized, setMaximized] = useState(false);

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
        onClick={() => void win.toggleMaximize()}
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
        onClick={() => void win.close()}
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
