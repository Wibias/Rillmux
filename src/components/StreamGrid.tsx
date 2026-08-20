import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import {
  isTwitchPartner,
  streamThumbnail,
  type HelixStream,
  type HelixUser,
} from "../lib/twitch/helix";
import { formatUptime, formatViewers } from "../lib/browse/format";
import { StreamActionsMenu } from "./StreamActionsMenu";
import { VerifiedBadge } from "./FollowedIcons";
import "./StreamGrid.css";

interface StreamGridProps {
  streams: HelixStream[];
  onWatch?: (stream: HelixStream) => void;
  pinnedLogins?: string[];
  onTogglePin?: (login: string) => void;
  usersByLogin?: Record<string, HelixUser>;
}

export function StreamGrid({
  streams,
  onWatch,
  pinnedLogins = [],
  onTogglePin,
  usersByLogin = {},
}: StreamGridProps) {
  const { t } = useTranslation(["common", "routes"]);
  const pins = new Set(pinnedLogins.map((login) => login.toLowerCase()));

  return (
    <div className="stream-grid">
      {streams.map((stream) => {
        const pinned = pins.has(stream.user_login.toLowerCase());
        const partner = isTwitchPartner(
          usersByLogin[stream.user_login.toLowerCase()],
        );
        return (
          <article key={stream.id} className="stream-card">
            <button
              type="button"
              className="stream-card__thumb-btn"
              onClick={() => onWatch?.(stream)}
              aria-label={t("common:watch")}
            >
              <img
                className="stream-card__thumb"
                src={streamThumbnail(stream.thumbnail_url)}
                alt=""
                loading="lazy"
              />
              <span className="badge badge--live stream-card__live">
                {t("routes:liveBadge")}
              </span>
            </button>
            <div className="stream-card__body">
              <div className="stream-card__row">
                <Link
                  className="stream-card__name"
                  to={`/channel/${stream.user_login}`}
                >
                  {stream.user_name}
                  {partner ? <VerifiedBadge /> : null}
                </Link>
                <StreamActionsMenu
                  stream={stream}
                  pinned={pinned}
                  onWatch={onWatch}
                  onTogglePin={onTogglePin}
                />
              </div>
              <p className="stream-card__title" title={stream.title}>
                {stream.title}
              </p>
              <p className="stream-card__meta muted">
                {t("routes:streamViewersUptime", {
                  viewers: formatViewers(stream.viewer_count),
                  uptime: formatUptime(stream.started_at),
                })}
              </p>
            </div>
          </article>
        );
      })}
    </div>
  );
}
