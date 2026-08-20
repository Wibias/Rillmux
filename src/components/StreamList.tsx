import { useState } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  getChannelFollowerCount,
  isTwitchPartner,
  streamThumbnail,
  type HelixStream,
  type HelixUser,
} from "../lib/twitch/helix";
import {
  formatStartedAt,
  formatUptime,
  formatViewers,
  twitchChannelUrl,
} from "../lib/browse/format";
import {
  followedDetailTags,
  type FollowedSort,
} from "../lib/browse/followedList";
import { isTauri } from "../lib/tauri";
import { StreamActionsMenu } from "./StreamActionsMenu";
import {
  ExternalLinkIcon,
  LinkIcon,
  PlayIcon,
  VerifiedBadge,
} from "./FollowedIcons";
import "./StreamList.css";

async function openExternal(url: string) {
  if (isTauri()) {
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

interface StreamListProps {
  streams: HelixStream[];
  startIndex?: number;
  sort?: FollowedSort;
  onWatch?: (stream: HelixStream) => void;
  pinnedLogins?: string[];
  onTogglePin?: (login: string) => void;
  usersByLogin?: Record<string, HelixUser>;
}

function isRowControl(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    Boolean(target.closest("a, button, .stream-menu"))
  );
}

function ExpandedStreamDetail({
  stream,
  user,
  onWatch,
}: {
  stream: HelixStream;
  user?: HelixUser;
  onWatch?: (stream: HelixStream) => void;
}) {
  const { t } = useTranslation(["common", "routes"]);
  const tags = followedDetailTags(stream);
  const partner = isTwitchPartner(user);
  const followers = useQuery({
    queryKey: ["channel-followers", stream.user_id],
    queryFn: () => getChannelFollowerCount(stream.user_id),
  });

  return (
    <div className="stream-list__detail">
      <div className="stream-list__detail-thumb-wrap">
        <img
          className="stream-list__detail-thumb"
          src={streamThumbnail(stream.thumbnail_url, 440, 248)}
          alt=""
        />
        <span className="badge badge--live">{t("routes:liveBadge")}</span>
      </div>
      <div className="stream-list__detail-copy">
        <Link className="stream-card__name" to={`/channel/${stream.user_login}`}>
          {stream.user_name}
          {partner ? <VerifiedBadge /> : null}
        </Link>
        <p className="stream-list__detail-title">{stream.title}</p>
        {tags.length ? (
          <ul className="stream-list__tags">
            {tags.map((tag) => (
              <li key={tag}>{tag}</li>
            ))}
          </ul>
        ) : null}
        {user?.description ? (
          <p className="stream-list__detail-bio muted">{user.description}</p>
        ) : null}
      </div>
      <dl className="stream-list__stats">
        <div>
          <dt>{t("routes:streamColViewers")}</dt>
          <dd>{formatViewers(stream.viewer_count)}</dd>
        </div>
        <div>
          <dt>{t("routes:streamColFollowers")}</dt>
          <dd>
            {followers.isSuccess
              ? formatViewers(followers.data)
              : "—"}
          </dd>
        </div>
        <div>
          <dt>{t("routes:streamColUptime")}</dt>
          <dd>{formatUptime(stream.started_at)}</dd>
        </div>
        <div>
          <dt>{t("routes:streamColStarted")}</dt>
          <dd>{formatStartedAt(stream.started_at)}</dd>
        </div>
      </dl>
      <div className="stream-list__detail-actions">
        <button type="button" onClick={() => onWatch?.(stream)}>
          <PlayIcon />
          {t("routes:streamPlay")}
        </button>
        <button
          type="button"
          className="button-secondary"
          onClick={() => void openExternal(twitchChannelUrl(stream.user_login))}
        >
          <ExternalLinkIcon />
          {t("routes:streamOpenTwitch")}
        </button>
        <button
          type="button"
          className="button-secondary"
          onClick={() =>
            void navigator.clipboard.writeText(
              twitchChannelUrl(stream.user_login),
            )
          }
        >
          <LinkIcon />
          {t("routes:streamCopyUrl")}
        </button>
      </div>
    </div>
  );
}

export function StreamList({
  streams,
  startIndex = 1,
  sort,
  onWatch,
  pinnedLogins = [],
  onTogglePin,
  usersByLogin = {},
}: StreamListProps) {
  const { t } = useTranslation(["common", "routes"]);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const pins = new Set(pinnedLogins.map((login) => login.toLowerCase()));

  return (
    <div className="stream-list">
      <div className="stream-list__head" aria-hidden="true">
        <span className="stream-list__idx">#</span>
        <span>{t("routes:streamColStreamer")}</span>
        <span>{t("routes:streamColTitle")}</span>
        <span>{t("routes:streamColGame")}</span>
        <span>
          {t("routes:streamColViewers")}
          {sort === "viewers-desc" ? " ↓" : null}
          {sort === "viewers-asc" ? " ↑" : null}
        </span>
        <span>{t("routes:streamColRuntime")}</span>
        <span>{t("routes:streamColActions")}</span>
      </div>
      {streams.map((stream, index) => {
        const expanded = expandedId === stream.id;
        const pinned = pins.has(stream.user_login.toLowerCase());
        const user = usersByLogin[stream.user_login.toLowerCase()];
        return (
          <article
            key={stream.id}
            className={
              expanded ? "stream-list__row is-expanded" : "stream-list__row"
            }
          >
            <div
              className="stream-list__main"
              onClick={(event) => {
                if (isRowControl(event.target)) return;
                setExpandedId((current) =>
                  current === stream.id ? null : stream.id,
                );
              }}
            >
              <span className="stream-list__idx muted">
                {startIndex + index}
              </span>
              <span className="stream-list__streamer">
                <span className="stream-list__thumb">
                  <img
                    src={streamThumbnail(stream.thumbnail_url, 96, 54)}
                    alt=""
                    loading="lazy"
                  />
                  <span className="badge badge--live">
                    {t("routes:liveBadge")}
                  </span>
                </span>
                <Link
                  className="stream-list__name"
                  to={`/channel/${stream.user_login}`}
                >
                  {stream.user_name}
                  {isTwitchPartner(user) ? <VerifiedBadge /> : null}
                </Link>
              </span>
              <span className="stream-list__title" title={stream.title}>
                {stream.title}
              </span>
              <span className="stream-list__game" title={stream.game_name}>
                {stream.game_name || "—"}
              </span>
              <span className="stream-list__num">
                {formatViewers(stream.viewer_count)}
              </span>
              <span className="stream-list__num">
                {formatUptime(stream.started_at)}
              </span>
              <span className="stream-list__actions">
                <button
                  type="button"
                  className="stream-list__play"
                  aria-label={t("common:watch")}
                  onClick={() => onWatch?.(stream)}
                >
                  <PlayIcon />
                </button>
                <StreamActionsMenu
                  stream={stream}
                  pinned={pinned}
                  onWatch={onWatch}
                  onTogglePin={onTogglePin}
                />
              </span>
            </div>
            {expanded ? (
              <ExpandedStreamDetail
                stream={stream}
                user={user}
                onWatch={onWatch}
              />
            ) : null}
          </article>
        );
      })}
    </div>
  );
}
