import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { streamThumbnail, type HelixStream, type HelixUser, isTwitchPartner } from "../lib/twitch/helix";
import { formatUptime, formatViewers } from "../lib/browse/format";
import type { FollowedView } from "../lib/browse/followedList";
import { StreamActionsMenu } from "./StreamActionsMenu";
import { ChevronIcon, PinIcon, PlayIcon, VerifiedBadge } from "./FollowedIcons";

export function PinnedFavourites({
  streams,
  view,
  collapsed,
  onToggleCollapsed,
  onWatch,
  onTogglePin,
  usersByLogin = {},
}: {
  streams: HelixStream[];
  view: FollowedView;
  collapsed: boolean;
  onToggleCollapsed: () => void;
  onWatch?: (stream: HelixStream) => void;
  onTogglePin: (login: string) => void;
  usersByLogin?: Record<string, HelixUser>;
}) {
  const { t } = useTranslation(["routes", "common"]);
  if (!streams.length) return null;

  return (
    <section className="followed-pins">
      <header className="followed-pins__head">
        <h2>
          <PinIcon />
          {t("pinnedFavourites")}
        </h2>
        <button
          type="button"
          className="button-secondary followed-pins__toggle"
          onClick={onToggleCollapsed}
          aria-expanded={!collapsed}
          aria-label={collapsed ? t("pinnedShow") : t("pinnedHide")}
        >
          <ChevronIcon up={!collapsed} />
        </button>
      </header>
      {collapsed ? null : view === "list" ? (
        <ul className="followed-pins__list">
          {streams.map((stream) => {
            const partner = isTwitchPartner(
              usersByLogin[stream.user_login.toLowerCase()],
            );
            return (
              <li key={stream.id} className="followed-pins__row">
                <button
                  type="button"
                  className="followed-pins__thumb"
                  onClick={() => onWatch?.(stream)}
                  aria-label={t("common:watch")}
                >
                  <img
                    src={streamThumbnail(stream.thumbnail_url, 128, 72)}
                    alt=""
                    loading="lazy"
                  />
                  <span className="badge badge--live">{t("liveBadge")}</span>
                </button>
                <Link
                  className="followed-pins__name"
                  to={`/channel/${stream.user_login}`}
                >
                  {stream.user_name}
                  {partner ? <VerifiedBadge /> : null}
                </Link>
                <span className="muted followed-pins__title" title={stream.title}>
                  {stream.title}
                </span>
                <span className="muted" title={stream.game_name}>
                  {stream.game_name}
                </span>
                <span className="muted">
                  {formatViewers(stream.viewer_count)}
                </span>
                <span className="muted">{formatUptime(stream.started_at)}</span>
                <span className="followed-pins__actions">
                  <button
                    type="button"
                    className="followed-pins__play"
                    aria-label={t("common:watch")}
                    onClick={() => onWatch?.(stream)}
                  >
                    <PlayIcon />
                  </button>
                  <StreamActionsMenu
                    stream={stream}
                    pinned
                    onWatch={onWatch}
                    onTogglePin={onTogglePin}
                  />
                </span>
              </li>
            );
          })}
        </ul>
      ) : (
        <div className="followed-pins__grid">
          {streams.map((stream) => {
            const partner = isTwitchPartner(
              usersByLogin[stream.user_login.toLowerCase()],
            );
            return (
              <article key={stream.id} className="followed-pins__card">
                <button
                  type="button"
                  className="followed-pins__thumb"
                  onClick={() => onWatch?.(stream)}
                  aria-label={t("common:watch")}
                >
                  <img
                    src={streamThumbnail(stream.thumbnail_url, 160, 90)}
                    alt=""
                    loading="lazy"
                  />
                  <span className="badge badge--live">{t("liveBadge")}</span>
                </button>
                <div className="followed-pins__copy">
                  <Link
                    className="followed-pins__name"
                    to={`/channel/${stream.user_login}`}
                  >
                    {stream.user_name}
                    {partner ? <VerifiedBadge /> : null}
                  </Link>
                  <p className="muted" title={stream.title}>
                    {stream.title}
                  </p>
                  <p className="muted">
                    {t("streamViewersUptime", {
                      viewers: formatViewers(stream.viewer_count),
                      uptime: formatUptime(stream.started_at),
                    })}
                  </p>
                </div>
                <StreamActionsMenu
                  stream={stream}
                  pinned
                  onWatch={onWatch}
                  onTogglePin={onTogglePin}
                />
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
