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

  return (
    <div
      className={`authbar__playback${compact ? " authbar__playback--compact" : ""}`}
    >
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

      {expanded ? (
        <div className="authbar__playback-panel">
          <strong>{t("bonusClaimsTitle")}</strong>
          {device ? (
            <>
              <p className="muted">{t("bonusClaimsDevicePrompt")}</p>
              <CopyableDeviceCode code={device.userCode} />
              <button
                type="button"
                className="button-secondary"
                onClick={() => {
                  clearPoll();
                  setDevice(null);
                  setBusy(false);
                }}
              >
                {t("cancel")}
              </button>
            </>
          ) : connected ? (
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
                onClick={() => void disconnect()}
              >
                {busy ? t("bonusClaimsRemoving") : t("bonusClaimsRemove")}
              </button>
            </>
          ) : (
            <>
              <p className="muted">{t("bonusClaimsExplanation")}</p>
              <button
                type="button"
                disabled={busy}
                onClick={() => void startLogin()}
              >
                {busy ? t("bonusClaimsConnecting") : t("bonusClaimsConnect")}
              </button>
            </>
          )}
          {error ? <p className="authbar__error">{error}</p> : null}
        </div>
      ) : null}
    </div>
  );
}
