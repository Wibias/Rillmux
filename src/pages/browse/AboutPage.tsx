import { useEffect, useState } from "react";
import { useLocation } from "react-router";
import { useTranslation } from "react-i18next";
import { DoctorPanel } from "../../components/DoctorPanel";
import { PageSubbar } from "../../components/AppShell";
import { ConnectionHealth } from "../../components/ConnectionHealth";
import { ChangelogDialog } from "../../components/ChangelogDialog";
import { useUpdaterCheck } from "../../components/DeepLinkAndUpdaterBootstrap";
import { isTauri } from "../../lib/tauri";

export function AboutPage() {
  const { t } = useTranslation("routes");
  const location = useLocation();
  const { status, version, error, check, install } = useUpdaterCheck();
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [showChangelog, setShowChangelog] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        if (isTauri()) {
          const { getVersion } = await import("@tauri-apps/api/app");
          const v = await getVersion();
          if (!cancelled) setAppVersion(v);
          return;
        }
      } catch {
        // fall through to package version
      }
      if (!cancelled) {
        setAppVersion(import.meta.env.VITE_APP_VERSION ?? null);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (location.hash !== "#connections") return;
    const frame = window.requestAnimationFrame(() => {
      document.getElementById("connections")?.scrollIntoView({ block: "start" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [location.hash]);

  return (
    <section className="page">
      <PageSubbar title={t("aboutTitle")} lede={t("aboutBlurb")} />
      {appVersion ? (
        <p className="muted about-version">
          {t("aboutVersion", { version: appVersion })}
          <button
            type="button"
            className="button-secondary"
            onClick={() => setShowChangelog(true)}
          >
            {t("viewChangelog")}
          </button>
        </p>
      ) : null}
      <div className="channel-header__actions" style={{ marginBottom: "1rem" }}>
        <button
          type="button"
          className="button-secondary"
          disabled={status === "checking"}
          onClick={() => void check()}
        >
          {status === "checking" ? t("updateChecking") : t("checkUpdates")}
        </button>
        {status === "available" && version ? (
          <button type="button" onClick={() => void install()}>
            {t("updateInstall")} ({version})
          </button>
        ) : null}
      </div>
      {status === "available" && version ? (
        <p>{t("updateAvailable", { version })}</p>
      ) : null}
      {status === "none" ? <p className="muted">{t("updateNone")}</p> : null}
      {status === "error" ? (
        <p className="authbar__error">
          {t("updateError")}
          {error ? ` — ${error}` : ""}
        </p>
      ) : null}
      <ConnectionHealth />
      <DoctorPanel />
      {showChangelog ? (
        <ChangelogDialog onClose={() => setShowChangelog(false)} />
      ) : null}
    </section>
  );
}

