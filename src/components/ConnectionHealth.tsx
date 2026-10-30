import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAuthStore } from "../lib/auth/store";
import type { ChannelPointsClaimAuthStatus } from "../lib/auth/claimAuth";
import {
  getTwitchWebsiteAuthStatus,
  type TwitchWebsiteAuthStatus,
} from "../lib/auth/website";
import { invoke, isTauri } from "../lib/tauri";
import { AuthBar } from "./AuthBar";
import "./ConnectionHealth.css";

interface EventSubStatus {
  enabled: boolean;
  watchedChannels: number;
  state: "idle" | "connecting" | "connected" | "retrying";
  lastError: string | null;
}

type LoadState<T> =
  | { kind: "checking" }
  | { kind: "ready"; value: T }
  | { kind: "error"; message: string };

type Tone = "ok" | "pending" | "warning" | "off" | "error";

function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

export function ConnectionHealth() {
  const { t } = useTranslation("routes");
  const authLoading = useAuthStore((state) => state.loading);
  const authError = useAuthStore((state) => state.error);
  const session = useAuthStore((state) => state.session);
  const [refreshId, setRefreshId] = useState(0);
  const [manualRefresh, setManualRefresh] = useState(false);
  const manualRefreshRef = useRef(false);
  const [website, setWebsite] = useState<LoadState<TwitchWebsiteAuthStatus>>({
    kind: "checking",
  });
  const [claims, setClaims] = useState<
    LoadState<ChannelPointsClaimAuthStatus>
  >({ kind: "checking" });
  const [eventSub, setEventSub] = useState<LoadState<EventSubStatus>>({
    kind: "checking",
  });
  const checking =
    manualRefresh ||
    (refreshId === 0 &&
      (website.kind === "checking" ||
        claims.kind === "checking" ||
        eventSub.kind === "checking"));

  useEffect(() => {
    if (!isTauri()) {
      const message = t("connectionsDesktopRequired");
      setWebsite({ kind: "error", message });
      setClaims({ kind: "error", message });
      setEventSub({ kind: "error", message });
      return;
    }

    let active = true;
    const showChecking = refreshId === 0 || manualRefreshRef.current;
    manualRefreshRef.current = false;
    if (showChecking) {
      setWebsite({ kind: "checking" });
      setClaims({ kind: "checking" });
      setEventSub({ kind: "checking" });
    }

    void Promise.allSettled([
      getTwitchWebsiteAuthStatus(),
      invoke<ChannelPointsClaimAuthStatus>(
        "channel_points_claim_auth_status",
      ),
      invoke<EventSubStatus>("eventsub_status"),
    ]).then(([websiteResult, claimsResult, eventSubResult]) => {
      if (!active) return;

      setWebsite(
        websiteResult.status === "fulfilled"
          ? { kind: "ready", value: websiteResult.value }
          : { kind: "error", message: errorMessage(websiteResult.reason) },
      );
      setClaims(
        claimsResult.status === "fulfilled"
          ? { kind: "ready", value: claimsResult.value }
          : { kind: "error", message: errorMessage(claimsResult.reason) },
      );
      setEventSub(
        eventSubResult.status === "fulfilled"
          ? { kind: "ready", value: eventSubResult.value }
          : { kind: "error", message: errorMessage(eventSubResult.reason) },
      );
      setManualRefresh(false);
    });

    return () => {
      active = false;
    };
  }, [refreshId, t]);

  useEffect(() => {
    if (!isTauri()) return;
    const refresh = () => setRefreshId((value) => value + 1);
    const timer = window.setInterval(refresh, 30_000);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", refresh);
    };
  }, []);

  const accountTone: Tone = authLoading
    ? "pending"
    : session?.loggedIn
      ? "ok"
      : authError
        ? "error"
        : "off";
  const accountText = authLoading
    ? t("connectionsChecking")
    : session?.loggedIn
      ? t("connectionsAccountConnected", {
          name: session.displayName ?? session.login,
        })
      : authError
        ? t("connectionsAccountError")
        : t("connectionsNotConnected");

  function playbackSummary(): { tone: Tone; text: string } {
    if (website.kind === "checking") {
      return { tone: "pending", text: t("connectionsChecking") };
    }
    if (website.kind === "error") {
      return { tone: "error", text: website.message };
    }
    if (!website.value.configured) {
      return { tone: "off", text: t("connectionsNotConnected") };
    }
    return website.value.playbackReady
      ? {
          tone: "ok",
          text: t("connectionsPlaybackReady", {
            account: website.value.login ?? t("connectionsCurrentAccount"),
          }),
        }
      : { tone: "warning", text: t("connectionsNeedsRepair") };
  }

  function claimSummary(): { tone: Tone; text: string } {
    if (claims.kind === "checking") {
      return { tone: "pending", text: t("connectionsChecking") };
    }
    if (claims.kind === "error") {
      return { tone: "error", text: claims.message };
    }
    return claims.value.configured
      ? {
          tone: "ok",
          text: t("connectionsClaimConnected", {
            account: claims.value.login ?? t("connectionsCurrentAccount"),
          }),
        }
      : { tone: "off", text: t("connectionsNotConnected") };
  }

  function eventSubSummary(): { tone: Tone; text: string } {
    if (eventSub.kind === "checking") {
      return { tone: "pending", text: t("connectionsChecking") };
    }
    if (eventSub.kind === "error") {
      return { tone: "error", text: eventSub.message };
    }
    const status = eventSub.value;
    if (!status.enabled) {
      return { tone: "off", text: t("connectionsRaidsDisabled") };
    }
    if (status.state === "connected") {
      return {
        tone: "ok",
        text: t("connectionsRaidsConnected", {
          count: status.watchedChannels,
        }),
      };
    }
    if (status.state === "retrying") {
      return {
        tone: "warning",
        text: t("connectionsRaidsRetrying", {
          reason: status.lastError ?? "other",
        }),
      };
    }
    if (status.state === "connecting") {
      return { tone: "pending", text: t("connectionsRaidsConnecting") };
    }
    return { tone: "off", text: t("connectionsRaidsIdle") };
  }

  const playback = playbackSummary();
  const claim = claimSummary();
  const raids = eventSubSummary();

  return (
    <section
      id="connections"
      className="connections"
      aria-labelledby="connections-title"
    >
      <div className="connections__header">
        <div>
          <h2 id="connections-title">{t("connectionsTitle")}</h2>
          <p className="muted">{t("connectionsLede")}</p>
        </div>
        <button
          type="button"
          className="button-secondary button-with-spinner"
          onClick={() => {
            manualRefreshRef.current = true;
            setManualRefresh(true);
            setRefreshId((value) => value + 1);
          }}
          disabled={checking}
        >
          {checking ? <span className="spinner" aria-hidden /> : null}
          {checking ? t("connectionsChecking") : t("connectionsRefresh")}
        </button>
      </div>
      <ul className="connections__list" aria-live="polite">
        <li className="connections__row">
          <span
            className="connections__dot"
            data-tone={accountTone}
            aria-hidden="true"
          />
          <div>
            <strong>{t("connectionsTwitch")}</strong>
            <span>{accountText}</span>
          </div>
        </li>
        <li className="connections__row">
          <span
            className="connections__dot"
            data-tone={playback.tone}
            aria-hidden="true"
          />
          <div>
            <strong>{t("connectionsPlayback")}</strong>
            <span>{playback.text}</span>
          </div>
        </li>
        <li className="connections__row">
          <span
            className="connections__dot"
            data-tone={claim.tone}
            aria-hidden="true"
          />
          <div>
            <strong>{t("connectionsClaims")}</strong>
            <span>{claim.text}</span>
          </div>
        </li>
        <li className="connections__row">
          <span
            className="connections__dot"
            data-tone={raids.tone}
            aria-hidden="true"
          />
          <div>
            <strong>{t("connectionsRaids")}</strong>
            <span>{raids.text}</span>
          </div>
        </li>
      </ul>
      <div className="connections__manage">
        <h3>{t("connectionsManage")}</h3>
        <p className="muted">{t("connectionsManageHint")}</p>
        <AuthBar />
      </div>
    </section>
  );
}
