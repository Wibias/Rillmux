import type { MultistreamPageModel } from "./model";
import { ChevronRightIcon, SearchIcon } from "../../components/FollowedIcons";
import { formatViewers } from "../../lib/browse/format";
import { streamThumbnail } from "../../lib/twitch/helix";

export function MultistreamSearchPanel({
  t,
  loggedIn,
  query,
  setQuery,
  debounced,
  followedLive,
  liveRef,
  isAdded,
  layoutFull,
  addChannel,
  followedResults,
  otherResults,
  search,
  renderResult,
}: MultistreamPageModel) {
  return (
      <div className="ms-section">
        <div className="ms-section__head">
          <h2>{t("multistream:searchTitle")}</h2>
        </div>
        {!loggedIn ? (
          <p className="muted">{t("multistream:loginRequired")}</p>
        ) : (
          <>
            <label className="ms-search">
              <span className="ms-search__icon">
                <SearchIcon />
              </span>
              <input
                type="search"
                className="search-hero__input"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("multistream:searchPlaceholder")}
                aria-label={t("multistream:searchTitle")}
              />
            </label>
            {debounced.length < 2 ? (
              <>
                <p className="muted">{t("multistream:searchMinChars")}</p>
                <div className="ms-divider">
                  {t("multistream:followedLiveTitle")}
                </div>
                {followedLive.streams.length ? (
                  <div className="ms-live-scroller">
                    <ul className="ms-live-grid" ref={liveRef}>
                    {followedLive.streams.map((s) => {
                      const added = isAdded(s.user_login);
                      return (
                        <li key={s.id} className="ms-live-card">
                          <div className="ms-live-card__media">
                            <img
                              src={streamThumbnail(s.thumbnail_url, 320, 180)}
                              alt=""
                              className="ms-live-card__thumb"
                              loading="lazy"
                            />
                            <span className="badge badge--live ms-live-card__live">
                              {t("routes:liveBadge")}
                            </span>
                          </div>
                          <div className="ms-live-card__row">
                            <span className="ms-live-card__name">
                              {s.user_name}
                            </span>
                            <button
                              type="button"
                              className="button-secondary"
                              disabled={added || layoutFull}
                              onClick={() =>
                                addChannel(s.user_login, s.title, s.game_name)
                              }
                            >
                              {added
                                ? t("multistream:added")
                                : t("multistream:add")}
                            </button>
                          </div>
                          <span className="ms-result__title">
                            {s.game_name}
                          </span>
                          <span className="ms-live-card__viewers">
                            {formatViewers(s.viewer_count)} viewers
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
                        liveRef.current?.scrollBy({
                          left: 280,
                          behavior: "smooth",
                        })
                      }
                    >
                      <ChevronRightIcon />
                    </button>
                  </div>
                ) : (
                  <p className="muted">
                    {followedLive.query.isLoading
                      ? t("common:loading")
                      : t("multistream:followedEmpty")}
                  </p>
                )}
              </>
            ) : (
              <>
                {followedResults.length ? (
                  <>
                    <div className="ms-divider">
                      {t("multistream:followedSection")}
                    </div>
                    <ul className="ms-results">
                      {followedResults.map(renderResult)}
                    </ul>
                  </>
                ) : null}
                <div className="ms-divider">
                  {t("multistream:allSection")}
                </div>
                {otherResults.length ? (
                  <ul className="ms-results">{otherResults.map(renderResult)}</ul>
                ) : (
                  <p className="muted">
                    {search.isFetching
                      ? t("common:loading")
                      : followedResults.length
                        ? ""
                        : t("routes:searchEmpty")}
                  </p>
                )}
              </>
            )}
          </>
        )}
      </div>
  );
}

