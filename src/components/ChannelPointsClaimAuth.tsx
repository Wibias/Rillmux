import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { invoke, isTauri } from "../lib/tauri";
import {
  applyBonusClaimsChipClick,
  type ChannelPointsClaimAuthStatus,
} from "../lib/auth/claimAuth";
import { CopyableDeviceCode } from "./CopyableDeviceCode";

interface TvDeviceCodeResponse {
  deviceCode: string;
  expiresIn: number;
  interval: number;
  userCode: string;
  verificationUri: string;
}

type TvDevicePoll =
  | { state: "pending" }
  | { state: "slowDown" }
  | { state: "done"; status: ChannelPointsClaimAuthStatus };

function ClaimAuthPanelBody({
  status,
  device,
  connected,
  busy,
  onCancel,
  onDisconnect,
  onStart,
}: {
  status: ChannelPointsClaimAuthStatus | null;
  device: TvDeviceCodeResponse | null;
  connected: boolean;
  busy: boolean;
  onCancel: () => void;
  onDisconnect: () => void;
  onStart: () => void;
}) {
  const { t } = useTranslation("common");

  if (device) {
    return (
      <>
        <p className="muted">{t("bonusClaimsDevicePrompt")}</p>
        <CopyableDeviceCode code={device.userCode} />
        <button type="button" className="button-secondary" onClick={onCancel}>
          {t("cancel")}
        </button>
      </>
    );
  }

  if (connected) {
    return (
      <>
        <p className="muted">
          {t("bonusClaimsConnectedHint", {
            login: status?.login ?? t("playbackAuthCurrentAccount"),
          })}
        </p>
        <button
          type="button"
          className="button-secondary"
          disabled={busy}
          onClick={onDisconnect}
        >
          {busy ? t("bonusClaimsRemoving") : t("bonusClaimsRemove")}
        </button>
      </>
    );
  }

  return (
    <>
      <p className="muted">{t("bonusClaimsExplanation")}</p>
      <button type="button" disabled={busy} onClick={onStart}>
        {busy ? t("bonusClaimsConnecting") : t("bonusClaimsConnect")}
      </button>
    </>
  );
}

function ClaimAuthPanel({
  visible,
  status,
  device,
  connected,
  busy,
  error,
  onCancel,
  onDisconnect,
  onStart,
}: {
  visible: boolean;
  status: ChannelPointsClaimAuthStatus | null;
  device: TvDeviceCodeResponse | null;
  connected: boolean;
  busy: boolean;
  error: string | null;
  onCancel: () => void;
  onDisconnect: () => void;
  onStart: () => void;
}) {
  const { t } = useTranslation("common");
  if (!visible) return null;

  return (
    <div className="authbar__playback-panel">
      <strong>{t("bonusClaimsTitle")}</strong>
      <ClaimAuthPanelBody
        status={status}
        device={device}
        connected={connected}
        busy={busy}
        onCancel={onCancel}
        onDisconnect={onDisconnect}
        onStart={onStart}
      />
      {error ? <p className="authbar__error">{error}</p> : null}
    </div>
  );
}

export function ChannelPointsClaimAuth({
  compact = false,
}: {
  compact?: boolean;
}) {
  const { t } = useTranslation("common");
  const [status, setStatus] = useState<ChannelPointsClaimAuthStatus | null>(null);
  const [device, setDevice] = useState<TvDeviceCodeResponse | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  function clearPoll() {
    if (pollTimer.current) {
      clearTimeout(pollTimer.current);
      pollTimer.current = null;
    }
  }

  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    void invoke<ChannelPointsClaimAuthStatus>("channel_points_claim_auth_status")
      .then((next) => {
        if (alive) setStatus(next);
      })
      .catch((reason: unknown) => {
        if (alive) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      });
    return () => {
      alive = false;
      clearPoll();
    };
  }, []);

  async function startLogin() {
    if (busy || !isTauri()) return;
    clearPoll();
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<TvDeviceCodeResponse>(
        "channel_points_claim_auth_start_device_login",
      );
      setDevice(next);
      await openUrl(next.verificationUri);

      let intervalMs = Math.max(next.interval, 1) * 1000;
      const poll = async () => {
        try {
          const result = await invoke<TvDevicePoll>(
            "channel_points_claim_auth_poll_device_login",
            { deviceCode: next.deviceCode },
          );
          if (result.state === "done") {
            clearPoll();
            setStatus(result.status);
            setDevice(null);
            setBusy(false);
            setExpanded(false);
            return;
          }
          if (result.state === "slowDown") {
            intervalMs = Math.min(intervalMs + 5000, 30_000);
          }
          pollTimer.current = setTimeout(() => void poll(), intervalMs);
        } catch (reason) {
          clearPoll();
          setDevice(null);
          setBusy(false);
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      };
      pollTimer.current = setTimeout(() => void poll(), intervalMs);
    } catch (reason) {
      setBusy(false);
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function disconnect() {
    if (busy || !isTauri()) return;
    clearPoll();
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<ChannelPointsClaimAuthStatus>(
        "channel_points_claim_auth_clear",
      );
      setStatus(next);
      setDevice(null);
      setExpanded(false);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  const connected = Boolean(status?.configured);
  const className = compact
    ? "authbar__playback authbar__playback--compact"
    : "authbar__playback";

  return (
    <div className={className}>
      <button
        type="button"
        className="button-secondary authbar__playback-toggle"
        aria-expanded={expanded}
        onClick={() =>
          setExpanded(
            applyBonusClaimsChipClick({
              expanded,
              status,
              device,
            }).expanded,
          )
        }
      >
        <span
          className={`authbar__playback-dot${connected ? " authbar__playback-dot--connected" : ""}`}
          aria-hidden="true"
        />
        {connected ? t("bonusClaimsConnected") : t("bonusClaimsSetup")}
      </button>

      <ClaimAuthPanel
        visible={expanded}
        status={status}
        device={device}
        connected={connected}
        busy={busy}
        error={error}
        onCancel={() => {
          clearPoll();
          setDevice(null);
          setBusy(false);
        }}
        onDisconnect={() => void disconnect()}
        onStart={() => void startLogin()}
      />
    </div>
  );
}
