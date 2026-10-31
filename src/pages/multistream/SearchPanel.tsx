import type { MultistreamPageModel } from "./model";
import {
  ChevronRightIcon,
  SearchIcon,
} from "../../components/FollowedIcons";
import { formatViewers } from "../../lib/browse/format";
import { streamThumbnail } from "../../lib/twitch/helix";

function SearchInput({
  t,
  query,
  setQuery,
}: Pick<MultistreamPageModel, "t" | "query" | "setQuery">) {
  return (
    <label className="ms-search">
      <span className="ms-search__icon">
        <SearchIcon />
      </span>
      <input
        type="search"
        className="search-hero__input"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder={t("multistream:searchPlaceholder")}
        aria-label={t("multistream:searchTitle")}
      />
    </label>
  );
}

function FollowedLiveSection({
  t,
  followedLive,
  liveRef,
  isAdded,
  layoutFull,
  addChannel,
}: Pick<
  MultistreamPageModel,
  | "t"
  | "followedLive"
  | "liveRef"
  | "isAdded"
  | "layoutFull"
  | "addChannel"
>) {
  if (!followedLive.streams.length) {
    return (
      <p className="muted">
        {followedLive.query.isLoading
          ? t("common:loading")
          : t("multistream:followedEmpty")}
      </p>
    );
  }

  return (
    <div className="ms-live-scroller">
      <ul className="ms-live-grid" ref={liveRef}>
        {followedLive.streams.map((stream) => {
          const added = isAdded(stream.user_login);
          return (
            <li key={stream.id} className="ms-live-card">
              <div className="ms-live-card__media">
                <img
                  src={streamThumbnail(stream.thumbnail_url, 320, 180)}
                  alt=""
                  className="ms-live-card__thumb"
                  loading="lazy"
                />
                <span className="badge badge--live ms-live-card__live">
                  {t("routes:liveBadge")}
                </span>
              </div>
              <div className="ms-live-card__row">
                <span className="ms-live-card__name">{stream.user_name}</span>
                <button
                  type="button"
                  className="button-secondary"
                  disabled={added || layoutFull}
                  onClick={() =>
                    addChannel(
                      stream.user_login,
                      stream.title,
                      stream.game_name,
                    )
                  }
                >
                  {added ? t("multistream:added") : t("multistream:add")}
                </button>
              </div>
              <span className="ms-result__title">{stream.game_name}</span>
              <span className="ms-live-card__viewers">
                {formatViewers(stream.viewer_count)} viewers
              </span>
            </li>
          );
        })}
      </ul>
      <button
        type="button"
        className="ms-live-scroller__next"
        aria-label={t("routes:followedPageNext")}
        onClick={() =>
          liveRef.current?.scrollBy({ left: 280, behavior: "smooth" })
        }
      >
        <ChevronRightIcon />
      </button>
    </div>
  );
}

function FollowedDiscovery(props: MultistreamPageModel) {
  const { t } = props;
  return (
    <>
      <p className="muted">{t("multistream:searchMinChars")}</p>
      <div className="ms-divider">{t("multistream:followedLiveTitle")}</div>
      <FollowedLiveSection {...props} />
    </>
  );
}

function SearchResults({
  t,
  followedResults,
  otherResults,
  search,
  renderResult,
}: Pick<
  MultistreamPageModel,
  "t" | "followedResults" | "otherResults" | "search" | "renderResult"
>) {
  const showEmpty = !search.isFetching && !followedResults.length;
  return (
    <>
      {followedResults.length ? (
        <>
          <div className="ms-divider">{t("multistream:followedSection")}</div>
          <ul className="ms-results">{followedResults.map(renderResult)}</ul>
        </>
      ) : null}
      <div className="ms-divider">{t("multistream:allSection")}</div>
      {otherResults.length ? (
        <ul className="ms-results">{otherResults.map(renderResult)}</ul>
      ) : (
        <p className="muted">
          {search.isFetching
            ? t("common:loading")
            : showEmpty
              ? t("routes:searchEmpty")
              : ""}
        </p>
      )}
    </>
  );
}

function LoggedInSearch(props: MultistreamPageModel) {
  const { debounced } = props;
  return (
    <>
      <SearchInput {...props} />
      {debounced.length < 2 ? (
        <FollowedDiscovery {...props} />
      ) : (
        <SearchResults {...props} />
      )}
    </>
  );
}

export function MultistreamSearchPanel(props: MultistreamPageModel) {
  const { t, loggedIn } = props;
  return (
    <div className="ms-section">
      <div className="ms-section__head">
        <h2>{t("multistream:searchTitle")}</h2>
      </div>
      {loggedIn ? (
        <LoggedInSearch {...props} />
      ) : (
        <p className="muted">{t("multistream:loginRequired")}</p>
      )}
    </div>
  );
}
