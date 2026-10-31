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

type AuthState = ReturnType<typeof useAuthStore.getState>;
type AuthSession = AuthState["session"];
type AuthDevice = AuthState["device"];
type AuthErrorValue = AuthState["error"];

function authBarClassName(compact: boolean) {
  return compact ? "authbar authbar--compact" : "authbar";
}

function AuthError({ error }: { error: AuthErrorValue }) {
  const { t } = useTranslation("common");
  if (!error) return null;

  const transient = isTransientAuthNetworkError(error);
  return (
    <p className={transient ? "authbar__status" : "authbar__error"}>
      {transient ? t("authNetworkUnavailable") : error}
    </p>
  );
}

function DeviceLogin({
  device,
  onCancel,
}: {
  device: AuthDevice;
  onCancel: () => void;
}) {
  const { t } = useTranslation("common");
  if (!device) return null;

  return (
    <div className="authbar__device">
      <p>{t("authDevicePrompt")}</p>
      <CopyableDeviceCode code={device.userCode} />
      <button type="button" className="button-secondary" onClick={onCancel}>
        {t("cancel")}
      </button>
    </div>
  );
}

function AuthIntegrations({ compact }: { compact: boolean }) {
  return (
    <>
      <TwitchWebsiteAuth compact={compact} />
      <ChannelPointsClaimAuth compact={compact} />
      <ChannelPointsStatus compact={compact} />
    </>
  );
}

function LoggedInControls({
  session,
  compact,
  onLogout,
}: {
  session: AuthSession;
  compact: boolean;
  onLogout: () => void;
}) {
  const { t } = useTranslation("common");
  if (!session?.loggedIn) return null;

  if (compact) {
    return <AuthIntegrations compact />;
  }

  return (
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
        <button type="button" className="button-secondary" onClick={onLogout}>
          {t("logout")}
        </button>
      </div>
      <ChannelPointsClaimAuth compact={compact} />
      <TwitchWebsiteAuth compact={compact} />
      <ChannelPointsStatus compact={compact} />
    </>
  );
}

function LoginButton({
  visible,
  loading,
  onLogin,
}: {
  visible: boolean;
  loading: boolean;
  onLogin: () => void;
}) {
  const { t } = useTranslation("common");
  if (!visible) return null;

  return (
    <button type="button" onClick={onLogin} disabled={loading}>
      {t("login")}
    </button>
  );
}

export function AuthBar({ compact = false }: { compact?: boolean }) {
  const { t } = useTranslation("common");
  const session = useAuthStore((s) => s.session);
  const device = useAuthStore((s) => s.device);
  const error = useAuthStore((s) => s.error);
  const loading = useAuthStore((s) => s.loading);
  const startLogin = useAuthStore((s) => s.startLogin);
  const cancelLogin = useAuthStore((s) => s.cancelLogin);
  const logout = useAuthStore((s) => s.logout);

  if (loading && !session) {
    return (
      <div className={authBarClassName(compact)}>
        <span className="muted">{t("loading")}</span>
      </div>
    );
  }

  const offerLogin = shouldOfferTwitchLogin({
    loggedIn: Boolean(session?.loggedIn),
    deviceActive: Boolean(device),
    error,
  });
  const showLogin = offerLogin && !compact && !session?.loggedIn;

  return (
    <div className={authBarClassName(compact)}>
      <AuthError error={error} />
      <DeviceLogin device={device} onCancel={cancelLogin} />
      <LoggedInControls
        session={session}
        compact={compact}
        onLogout={() => void logout()}
      />
      <LoginButton
        visible={showLogin}
        loading={loading}
        onLogin={() => void startLogin()}
      />
    </div>
  );
}
