import { useTranslation } from "react-i18next";
import { useAuthStore } from "../lib/auth/store";
import {
  isTransientAuthNetworkError,
  shouldOfferTwitchLogin,
} from "../lib/auth/sessionRestore";
import { CopyableDeviceCode } from "./CopyableDeviceCode";
import { ChannelPointsClaimAuth } from "./ChannelPointsClaimAuth";
import { ChannelPointsStatus } from "./ChannelPointsStatus";
import { TwitchWebsiteAuth } from "./TwitchWebsiteAuth";
import "./AuthBar.css";

export function AuthBar({ compact = false }: { compact?: boolean }) {
  const { t } = useTranslation("common");
  const session = useAuthStore((s) => s.session);
  const device = useAuthStore((s) => s.device);
  const error = useAuthStore((s) => s.error);
  const loading = useAuthStore((s) => s.loading);
  const startLogin = useAuthStore((s) => s.startLogin);
  const cancelLogin = useAuthStore((s) => s.cancelLogin);
  const logout = useAuthStore((s) => s.logout);
  const offerLogin = shouldOfferTwitchLogin({
    loggedIn: Boolean(session?.loggedIn),
    deviceActive: Boolean(device),
    error,
  });
  const errorText = error
    ? isTransientAuthNetworkError(error)
      ? t("authNetworkUnavailable")
      : error
    : null;

  if (loading && !session) {
    return (
      <div className={`authbar${compact ? " authbar--compact" : ""}`}>
        <span className="muted">{t("loading")}</span>
      </div>
    );
  }

  return (
    <div className={`authbar${compact ? " authbar--compact" : ""}`}>
      {errorText ? (
        <p
          className={
            isTransientAuthNetworkError(error)
              ? "authbar__status"
              : "authbar__error"
          }
        >
          {errorText}
        </p>
      ) : null}
      {device ? (
        <div className="authbar__device">
          <p>{t("authDevicePrompt")}</p>
          <CopyableDeviceCode code={device.userCode} />
          <button
            type="button"
            className="button-secondary"
            onClick={cancelLogin}
          >
            {t("cancel")}
          </button>
        </div>
      ) : null}
      {session?.loggedIn && !compact ? (
        <>
          <div className="authbar__user">
            {session.profileImageUrl ? (
              <img
                src={session.profileImageUrl}
                alt=""
                className="authbar__avatar"
                width={28}
                height={28}
              />
            ) : null}
            <span className="authbar__name">
              {session.displayName ?? session.login}
            </span>
            <button
              type="button"
              className="button-secondary"
              onClick={() => void logout()}
            >
              {t("logout")}
            </button>
          </div>
          <ChannelPointsClaimAuth compact={compact} />
          <TwitchWebsiteAuth compact={compact} />
          <ChannelPointsStatus compact={compact} />
        </>
      ) : session?.loggedIn ? (
        <>
          <TwitchWebsiteAuth compact={compact} />
          <ChannelPointsClaimAuth compact={compact} />
          <ChannelPointsStatus compact={compact} />
        </>
      ) : offerLogin && !compact ? (
        <button
          type="button"
          onClick={() => void startLogin()}
          disabled={loading}
        >
          {t("login")}
        </button>
      ) : null}
    </div>
  );
}
