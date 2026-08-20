import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useTranslation } from "react-i18next";
import { useAuthStore } from "../lib/auth/store";
import { isTauri } from "../lib/tauri";
import { ChevronIcon } from "./FollowedIcons";

export function NavAccount() {
  const { t } = useTranslation("common");
  const session = useAuthStore((s) => s.session);
  const loading = useAuthStore((s) => s.loading);
  const startLogin = useAuthStore((s) => s.startLogin);
  const logout = useAuthStore((s) => s.logout);
  const [open, setOpen] = useState(false);
  const [version, setVersion] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const loggedIn = Boolean(session?.loggedIn);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        if (isTauri()) {
          const next = await getVersion();
          if (!cancelled) setVersion(next);
          return;
        }
      } catch {
        // fall through to package version
      }
      if (!cancelled) {
        setVersion(import.meta.env.VITE_APP_VERSION ?? null);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const name = session?.displayName ?? session?.login ?? "";

  return (
    <div className="shell__account" ref={rootRef}>
      {loggedIn ? (
        <>
          <button
            type="button"
            className="shell__account-btn"
            aria-expanded={open}
            aria-haspopup="menu"
            onClick={() => setOpen((value) => !value)}
          >
            {session?.profileImageUrl ? (
              <img
                src={session.profileImageUrl}
                alt=""
                className="shell__account-avatar"
                width={28}
                height={28}
              />
            ) : (
              <span className="shell__account-avatar shell__account-avatar--empty">
                {name.slice(0, 1).toUpperCase()}
              </span>
            )}
            <span className="shell__account-name">{name}</span>
            <ChevronIcon up={open} />
          </button>
          {open ? (
            <div className="shell__account-menu" role="menu">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setOpen(false);
                  void logout();
                }}
              >
                {t("logout")}
              </button>
            </div>
          ) : null}
        </>
      ) : (
        <button
          type="button"
          className="shell__account-login"
          onClick={() => void startLogin()}
          disabled={loading}
        >
          {t("login")}
        </button>
      )}
      {version ? <span className="shell__version">v{version}</span> : null}
    </div>
  );
}
