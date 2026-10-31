import type { MultistreamPageModel } from "./model";
import { useWatchingStore } from "../../lib/streaming/store";
import { isMultistreamLayout, isUnevenLayout, isUnevenMainSide, layoutCapacity, MULTISTREAM_LAYOUTS, UNEVEN_MAIN_SIDES } from "../../lib/streaming/layout";

export function MultistreamLayoutControls({
  t,
  settings,
  setSettings,
  applyLayout,
  runningCount,
  cap,
  layoutFull,
  launchError,
}: MultistreamPageModel) {
  return (
      <div className="ms-section">
        <label className="ms-layout-row">
          <span>{t("multistream:layoutLabel")}</span>
          <select
            value={settings.streaming.multistreamLayout}
            onChange={(e) => {
              const value = e.target.value;
              if (!isMultistreamLayout(value)) return;
              if (runningCount > layoutCapacity(value)) {
                useWatchingStore.setState({
                  error: `Layout holds ${layoutCapacity(value)} streams. Stop extras first.`,
                });
                return;
              }
              setSettings({
                streaming: { ...settings.streaming, multistreamLayout: value },
              });
              applyLayout();
            }}
          >
            {MULTISTREAM_LAYOUTS.map((layout) => (
              <option key={layout} value={layout}>
                {t(`settings:layout${layout}`)}
              </option>
            ))}
          </select>
        </label>
        {isUnevenLayout(settings.streaming.multistreamLayout) ? (
          <label className="ms-layout-row">
            <span>{t("settings:unevenMainSide")}</span>
            <select
              value={settings.streaming.unevenMainSide}
              onChange={(e) => {
                const value = e.target.value;
                if (!isUnevenMainSide(value)) return;
                setSettings({
                  streaming: { ...settings.streaming, unevenMainSide: value },
                });
                applyLayout();
              }}
            >
              {UNEVEN_MAIN_SIDES.map((side) => (
                <option key={side} value={side}>
                  {t(
                    `settings:mainSide${side[0]!.toUpperCase()}${side.slice(1)}`,
                  )}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <p className="muted ms-slots-meta">
          {t("multistream:slotsUsed", { used: runningCount, cap })}
          {layoutFull ? ` — ${t("multistream:layoutFull")}` : ""}
        </p>
        {launchError ? <p className="muted">{launchError}</p> : null}
      </div>
  );
}

