import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { NavLink } from "react-router";
import { useTranslation } from "react-i18next";
import { AuthBar } from "./AuthBar";
import { NavAccount } from "./NavAccount";
import { TitlebarControls } from "./TitlebarControls";
import { HeartIcon, NAV_ICONS } from "./NavIcons";
import { useFollowedLiveStreams } from "../lib/browse/useFollowedLive";
import { invoke, isTauri } from "../lib/tauri";
import "./AppShell.css";

const ShellSubbarContext = createContext<HTMLDivElement | null>(null);

export function ShellSubbarPortal({ children }: { children: ReactNode }) {
  const host = useContext(ShellSubbarContext);
  if (!host) return null;
  return createPortal(children, host);
}

export function PageSubbar({
  title,
  lede,
  actions,
}: {
  title: string;
  lede?: string;
  actions?: ReactNode;
}) {
  return (
    <ShellSubbarPortal>
      <div className="page-subbar">
        <div className="page-subbar__copy">
          <h1 className="page-subbar__title">{title}</h1>
          {lede ? <p className="page-subbar__lede">{lede}</p> : null}
        </div>
        {actions ? <div className="page-subbar__actions">{actions}</div> : null}
      </div>
    </ShellSubbarPortal>
  );
}

const primaryLinks = [
  { to: "/", key: "followed" as const },
  { to: "/streams", key: "streams" as const },
  { to: "/games", key: "games" as const },
  { to: "/search", key: "search" as const },
  { to: "/teams", key: "teams" as const },
  { to: "/watching", key: "watching" as const },
  { to: "/multistream", key: "multistream" as const },
];

const secondaryLinks = [
  { to: "/settings", key: "settings" as const },
  { to: "/about", key: "about" as const },
];

export function AppShell({ children }: { children: React.ReactNode }) {
  const { t } = useTranslation("nav");
  const { t: tc } = useTranslation("common");
  const { streams, loggedIn } = useFollowedLiveStreams();
  const [subbar, setSubbar] = useState<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    void invoke("enable_title_bar_overlay").catch(() => {
      // Overlay is Windows-only; HTML caption buttons remain as fallback.
    });
  }, []);

  return (
    <div className="shell">
      <header className="shell__titlebar">
        <div className="shell__titlebar-drag" data-tauri-drag-region>
          <img
            className="shell__titlebar-icon"
            src="/app-icon.png"
            width={19}
            height={19}
            alt=""
          />
          <span className="shell__titlebar-title">{tc("appName")}</span>
        </div>
        <AuthBar compact />
        <TitlebarControls />
      </header>
      <div className="shell__brand">
        <img
          className="shell__brand-mark"
          src="/app-icon.png"
          width={28}
          height={28}
          alt=""
        />
        <div>
          <div className="shell__brand-title">{tc("appNameShort")}</div>
          <div className="shell__brand-sub">{tc("appTagline")}</div>
        </div>
      </div>
      <div className="shell__subbar" ref={setSubbar} />
      <aside className="shell__nav" aria-label={tc("appName")}>
        <div className="shell__section shell__browse">
          <h2 className="shell__section-label">{t("browse")}</h2>
          <nav className="shell__links" aria-label={t("browse")}>
            {primaryLinks.map((link) => {
              const Icon = NAV_ICONS[link.key];
              return (
                <NavLink
                  key={link.to}
                  to={link.to}
                  className={({ isActive }) =>
                    isActive ? "shell__link shell__link--active" : "shell__link"
                  }
                  end={link.to === "/"}
                >
                  {({ isActive }) => (
                    <>
                      {link.key === "followed" ? (
                        <HeartIcon filled={isActive} />
                      ) : (
                        <Icon />
                      )}
                      <span>{t(link.key)}</span>
                      {link.key === "followed" && loggedIn ? (
                        <span className="shell__link-count">{streams.length}</span>
                      ) : null}
                    </>
                  )}
                </NavLink>
              );
            })}
          </nav>
        </div>

        <div className="shell__bottom">
          <div className="shell__section">
            <h2 className="shell__section-label">{t("system")}</h2>
            <nav className="shell__links" aria-label={t("system")}>
              {secondaryLinks.map((link) => {
                const Icon = NAV_ICONS[link.key];
                return (
                  <NavLink
                    key={link.to}
                    to={link.to}
                    className={({ isActive }) =>
                      isActive ? "shell__link shell__link--active" : "shell__link"
                    }
                  >
                    <Icon />
                    <span>{t(link.key)}</span>
                  </NavLink>
                );
              })}
            </nav>
          </div>
          <NavAccount />
        </div>
      </aside>
      <div className="shell__content">
        <main className="shell__main">
          <ShellSubbarContext.Provider value={subbar}>
            {children}
          </ShellSubbarContext.Provider>
        </main>
      </div>
    </div>
  );
}
