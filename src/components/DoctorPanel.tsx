import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke, isTauri } from "../lib/tauri";
import type { DoctorReport } from "../lib/doctor";
import {
  PlayerSetupHelp,
  StreamlinkSetupHelp,
  ChatterinoSetupHelp,
} from "./SetupHelp";
import "./DoctorPanel.css";

function StreamlinkDoctorItem({
  status,
  onRecheck,
  checking,
}: {
  status: DoctorReport["streamlink"];
  onRecheck: () => void;
  checking: boolean;
}) {
  const { t } = useTranslation("routes");
  return (
    <li>
      {status.found
        ? t("doctorStreamlinkOk", { version: status.version ?? "?" })
        : t("doctorStreamlinkMissing")}
      {status.found && status.path ? (
        <p className="doctor__path muted">
          {t("doctorToolPath", { path: status.path })}
        </p>
      ) : null}
      {!status.found ? (
        <StreamlinkSetupHelp
          status={status}
          onRecheck={onRecheck}
          checking={checking}
        />
      ) : null}
    </li>
  );
}

function PlayerDoctorItem({
  status,
  onRecheck,
  checking,
}: {
  status: DoctorReport["mpv"];
  onRecheck: () => void;
  checking: boolean;
}) {
  const { t } = useTranslation("routes");
  return (
    <li>
      {status.found
        ? t("doctorPlayerOk", { version: status.version ?? "?" })
        : t("doctorPlayerMissing")}
      {status.found && status.path ? (
        <p className="doctor__path muted">
          {t("doctorToolPath", { path: status.path })}
        </p>
      ) : null}
      {!status.found ? (
        <PlayerSetupHelp
          status={status}
          onRecheck={onRecheck}
          checking={checking}
        />
      ) : null}
    </li>
  );
}

function ChatterinoDoctorItem({
  status,
  onRecheck,
  checking,
}: {
  status: DoctorReport["chatterino"];
  onRecheck: () => void;
  checking: boolean;
}) {
  const { t } = useTranslation("routes");
  return (
    <li>
      {status.found ? t("doctorChatOk") : t("doctorChatMissing")}
      {status.found && status.path ? (
        <p className="doctor__path muted">
          {t("doctorToolPath", { path: status.path })}
        </p>
      ) : null}
      {!status.found ? (
        <ChatterinoSetupHelp
          status={status}
          onRecheck={onRecheck}
          checking={checking}
        />
      ) : null}
    </li>
  );
}

function DoctorResults({
  report,
  onRecheck,
  checking,
}: {
  report: DoctorReport | null;
  onRecheck: () => void;
  checking: boolean;
}) {
  if (!report) return null;

  return (
    <ul className="doctor__list">
      <StreamlinkDoctorItem
        status={report.streamlink}
        onRecheck={onRecheck}
        checking={checking}
      />
      <PlayerDoctorItem
        status={report.mpv}
        onRecheck={onRecheck}
        checking={checking}
      />
      <ChatterinoDoctorItem
        status={report.chatterino}
        onRecheck={onRecheck}
        checking={checking}
      />
    </ul>
  );
}

export function DoctorPanel() {
  const { t } = useTranslation("routes");
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  const refresh = useCallback(() => {
    if (!isTauri()) {
      setError("Desktop shell required (`npm run tauri:dev`).");
      return;
    }
    setChecking(true);
    invoke<DoctorReport>("get_doctor_report")
      .then((data) => {
        setReport(data);
        setError(null);
      })
      .catch((err: unknown) => {
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => setChecking(false));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <section className="doctor">
      <div className="doctor__header">
        <h2>{t("doctorTitle")}</h2>
        <button
          type="button"
          className="button-secondary button-with-spinner"
          onClick={refresh}
          disabled={checking}
        >
          {checking ? <span className="spinner" aria-hidden /> : null}
          {checking ? t("doctorChecking") : t("doctorRecheck")}
        </button>
      </div>
      {checking ? (
        <p className="doctor__checking muted">
          <span className="spinner" aria-hidden />
          {t("doctorChecking")}
        </p>
      ) : null}
      {error ? <p className="muted">{error}</p> : null}
      {!report && !error && !checking ? <p className="muted">…</p> : null}
      <DoctorResults report={report} onRecheck={refresh} checking={checking} />
    </section>
  );
}
