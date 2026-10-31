import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { LoadingGrid } from "../../components/LoadingGrid";
import { PageRefreshButton } from "../../components/PageRefreshButton";
import { PinnedFavourites } from "../../components/PinnedFavourites";
import { StreamGrid } from "../../components/StreamGrid";
import { StreamList } from "../../components/StreamList";
import { PageSubbar } from "../../components/AppShell";
import { useAuthStore } from "../../lib/auth/store";
import { useWatchingStore } from "../../lib/streaming/store";
import {
  FilterIcon,
  GridViewIcon,
  ListViewIcon,
  SearchIcon,
} from "../../components/FollowedIcons";
import { getUsersByLogin } from "../../lib/twitch/helix";
import { useSettingsStore } from "../../lib/settings/store";
import { useFollowedLiveStreams } from "../../lib/browse/useFollowedLive";
import {
  filterFollowedStreams,
  followedVisibleCount,
  paginateItems,
  partitionPinned,
  sortFollowedStreams,
  togglePinnedLogin,
  type FollowedSort,
  type FollowedView,
} from "../../lib/browse/followedList";
import "../FollowedPage.css";

function useSearchShortcut(searchRef: React.RefObject<HTMLInputElement | null>) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== "k" || !(event.ctrlKey || event.metaKey)) {
        return;
      }
      event.preventDefault();
      searchRef.current?.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [searchRef]);
}

function useDismissFilters(
  filtersOpen: boolean,
  filtersRef: React.RefObject<HTMLDivElement | null>,
  closeFilters: () => void,
) {
  useEffect(() => {
    if (!filtersOpen) return;
    const onPointer = (event: PointerEvent) => {
      if (!filtersRef.current?.contains(event.target as Node)) {
        closeFilters();
      }
    };
    document.addEventListener("pointerdown", onPointer);
    return () => document.removeEventListener("pointerdown", onPointer);
  }, [filtersOpen, filtersRef, closeFilters]);
}

function useFollowedFitSize({
  fitRef,
  view,
  pinsCollapsed,
  pinnedCount,
  loggedIn,
  restCount,
}: {
  fitRef: React.RefObject<HTMLDivElement | null>;
  view: FollowedView;
  pinsCollapsed: boolean;
  pinnedCount: number;
  loggedIn: boolean;
  restCount: number;
}) {
  const [fitSize, setFitSize] = useState(12);

  useEffect(() => {
    const node = fitRef.current;
    if (!node) return;
    const apply = () => {
      const rect = node.getBoundingClientRect();
      if (rect.width < 80 || rect.height < 80) return;
      const next = followedVisibleCount({
        view,
        width: rect.width,
        height: rect.height,
      });
      setFitSize((current) => (current === next ? current : next));
    };
    apply();
    const observer = new ResizeObserver(apply);
    observer.observe(node);
    return () => observer.disconnect();
  }, [fitRef, view, pinsCollapsed, pinnedCount, loggedIn, restCount]);

  return fitSize;
}

function pageButtonNumbers(page: number, pageCount: number) {
  if (pageCount <= 9) {
    return Array.from({ length: pageCount }, (_, index) => index + 1);
  }
  const start = Math.max(1, Math.min(page - 3, pageCount - 8));
  return Array.from({ length: 9 }, (_, index) => start + index);
}

function useFollowedUsers(
  loggedIn: boolean,
  userLogins: string[],
) {
  return useQuery({
    queryKey: ["followed-users", userLogins],
    enabled: loggedIn && userLogins.length > 0,
    queryFn: async () => {
      const page = await getUsersByLogin(userLogins);
      const record: Record<string, (typeof page.data)[number]> = {};
      for (const user of page.data) {
        record[user.login.toLowerCase()] = user;
      }
      return record;
    },
  });
}

function useFollowedPageModel() {
  const { t } = useTranslation(["routes", "common"]);
  const session = useAuthStore((s) => s.session);
  const watchStream = useWatchingStore((s) => s.watchStream);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const loggedIn = Boolean(session?.loggedIn && session.userId);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const [pinsCollapsed, setPinsCollapsed] = useState(false);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const filtersRef = useRef<HTMLDivElement>(null);
  const fitRef = useRef<HTMLDivElement>(null);
  const { query, streams } = useFollowedLiveStreams();

  useSearchShortcut(searchRef);
  useDismissFilters(filtersOpen, filtersRef, () => setFiltersOpen(false));

  const refreshing = query.isFetching && !query.isFetchingNextPage;
  const filtered = useMemo(
    () =>
      filterFollowedStreams(streams, {
        query: search,
        hideMature: settings.gui.hideMatureFollowed,
      }),
    [streams, search, settings.gui.hideMatureFollowed],
  );
  const sorted = useMemo(
    () => sortFollowedStreams(filtered, settings.gui.followedSort),
    [filtered, settings.gui.followedSort],
  );
  const { pinned, rest } = useMemo(
    () => partitionPinned(sorted, settings.gui.pinnedFollowed),
    [sorted, settings.gui.pinnedFollowed],
  );
  const fitSize = useFollowedFitSize({
    fitRef,
    view: settings.gui.followedView,
    pinsCollapsed,
    pinnedCount: pinned.length,
    loggedIn,
    restCount: rest.length,
  });
  const paged = paginateItems(rest, page, fitSize);
  const userLogins = useMemo(
    () => [
      ...new Set(
        [...pinned, ...paged.pageItems].map((stream) =>
          stream.user_login.toLowerCase(),
        ),
      ),
    ],
    [pinned, paged.pageItems],
  );
  const usersQuery = useFollowedUsers(loggedIn, userLogins);
  const usersByLogin = usersQuery.data ?? {};

  useEffect(() => {
    setPage(1);
  }, [
    search,
    settings.gui.followedSort,
    settings.gui.followedView,
    settings.gui.hideMatureFollowed,
  ]);

  function patchGui(
    patch: Partial<{
      followedView: FollowedView;
      followedSort: FollowedSort;
      hideMatureFollowed: boolean;
      pinnedFollowed: string[];
    }>,
  ) {
    setSettings({ gui: { ...settings.gui, ...patch } });
  }

  const onWatch = (stream: (typeof streams)[number]) => {
    void watchStream(stream);
  };
  const onTogglePin = (login: string) => {
    patchGui({
      pinnedFollowed: togglePinnedLogin(settings.gui.pinnedFollowed, login),
    });
  };

  return {
    t,
    loggedIn,
    search,
    setSearch,
    searchRef,
    filtersRef,
    filtersOpen,
    setFiltersOpen,
    settings,
    patchGui,
    refreshing,
    query,
    streams,
    pinned,
    pinsCollapsed,
    setPinsCollapsed,
    onWatch,
    onTogglePin,
    usersByLogin,
    fitRef,
    paged,
    rest,
    pageButtons: pageButtonNumbers(paged.page, paged.pageCount),
    setPage,
  };
}

type FollowedPageModel = ReturnType<typeof useFollowedPageModel>;

function FollowedToolbar({ model }: { model: FollowedPageModel }) {
  const {
    t,
    search,
    setSearch,
    searchRef,
    filtersRef,
    filtersOpen,
    setFiltersOpen,
    settings,
    patchGui,
    refreshing,
    query,
  } = model;

  return (
    <div className="followed-toolbar">
      <label className="followed-search">
        <span className="followed-search__icon">
          <SearchIcon />
        </span>
        <input
          ref={searchRef}
          className="input"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder={t("routes:followedSearch")}
          aria-label={t("routes:followedSearch")}
        />
        <span className="followed-search__hint">Ctrl+K</span>
      </label>
      <div className="followed-filters" ref={filtersRef}>
        <button
          type="button"
          className="button-secondary followed-toolbar__action"
          aria-expanded={filtersOpen}
          onClick={() => setFiltersOpen((value) => !value)}
        >
          <FilterIcon />
          {t("routes:followedFilters")}
        </button>
        {filtersOpen ? (
          <div className="followed-filters__panel">
            <label>
              <input
                type="checkbox"
                checked={settings.gui.hideMatureFollowed}
                onChange={(event) =>
                  patchGui({ hideMatureFollowed: event.target.checked })
                }
              />
              {t("routes:followedHideMature")}
            </label>
          </div>
        ) : null}
      </div>
      <select
        className="followed-toolbar__sort"
        value={settings.gui.followedSort}
        onChange={(event) =>
          patchGui({ followedSort: event.target.value as FollowedSort })
        }
        aria-label={t("routes:followedSortViewersDesc")}
      >
        <option value="viewers-desc">{t("routes:followedSortViewersDesc")}</option>
        <option value="viewers-asc">{t("routes:followedSortViewersAsc")}</option>
        <option value="uptime-desc">{t("routes:followedSortUptime")}</option>
        <option value="name">{t("routes:followedSortName")}</option>
      </select>
      <div
        className="followed-views"
        role="group"
        aria-label={t("routes:followedViewGrid")}
      >
        <button
          type="button"
          className={settings.gui.followedView === "list" ? "is-active" : ""}
          aria-pressed={settings.gui.followedView === "list"}
          aria-label={t("routes:followedViewList")}
          title={t("routes:followedViewList")}
          onClick={() => patchGui({ followedView: "list" })}
        >
          <ListViewIcon />
        </button>
        <button
          type="button"
          className={settings.gui.followedView === "grid" ? "is-active" : ""}
          aria-pressed={settings.gui.followedView === "grid"}
          aria-label={t("routes:followedViewGrid")}
          title={t("routes:followedViewGrid")}
          onClick={() => patchGui({ followedView: "grid" })}
        >
          <GridViewIcon />
        </button>
      </div>
      <PageRefreshButton
        iconOnly
        refreshing={refreshing}
        onRefresh={() => void query.refetch()}
      />
    </div>
  );
}

function FollowedStreams({ model }: { model: FollowedPageModel }) {
  const {
    settings,
    pinned,
    pinsCollapsed,
    setPinsCollapsed,
    onWatch,
    onTogglePin,
    usersByLogin,
    fitRef,
    paged,
  } = model;

  return (
    <>
      <PinnedFavourites
        streams={pinned}
        view={settings.gui.followedView}
        collapsed={pinsCollapsed}
        onToggleCollapsed={() => setPinsCollapsed((value) => !value)}
        onWatch={onWatch}
        onTogglePin={onTogglePin}
        usersByLogin={usersByLogin}
      />
      <div className="followed-fit" ref={fitRef}>
        {settings.gui.followedView === "list" ? (
          <StreamList
            streams={paged.pageItems}
            startIndex={paged.start || 1}
            sort={settings.gui.followedSort}
            onWatch={onWatch}
            pinnedLogins={settings.gui.pinnedFollowed}
            onTogglePin={onTogglePin}
            usersByLogin={usersByLogin}
          />
        ) : (
          <StreamGrid
            streams={paged.pageItems}
            onWatch={onWatch}
            pinnedLogins={settings.gui.pinnedFollowed}
            onTogglePin={onTogglePin}
            usersByLogin={usersByLogin}
          />
        )}
      </div>
    </>
  );
}

function FollowedBody({ model }: { model: FollowedPageModel }) {
  const { t, loggedIn, query, streams } = model;
  return (
    <div className="followed-body">
      {!loggedIn ? (
        <p className="muted">{t("routes:followedLoginRequired")}</p>
      ) : null}
      {query.isLoading ? <LoadingGrid /> : null}
      {query.isError ? (
        <p className="muted">{(query.error as Error).message}</p>
      ) : null}
      {loggedIn && !query.isLoading && streams.length === 0 ? (
        <div className="empty-panel">
          <strong>{t("routes:followedEmpty")}</strong>
        </div>
      ) : null}
      {streams.length ? <FollowedStreams model={model} /> : null}
    </div>
  );
}

function FollowedPager({ model }: { model: FollowedPageModel }) {
  const { t, streams, rest, paged, pageButtons, setPage } = model;
  if (!streams.length || !rest.length) return null;

  return (
    <div className="followed-pager">
      <p className="muted">
        {t("routes:followedShowing", {
          start: paged.start,
          end: paged.end,
          total: rest.length,
        })}
      </p>
      <div className="followed-pager__pages">
        <button
          type="button"
          className="button-secondary"
          disabled={paged.page <= 1}
          onClick={() => setPage(1)}
          aria-label={t("routes:followedPageFirst")}
        >
          «
        </button>
        <button
          type="button"
          className="button-secondary"
          disabled={paged.page <= 1}
          onClick={() => setPage(paged.page - 1)}
          aria-label={t("routes:followedPagePrev")}
        >
          ‹
        </button>
        {pageButtons.map((number) => (
          <button
            key={number}
            type="button"
            className={number === paged.page ? "is-active" : ""}
            onClick={() => setPage(number)}
          >
            {number}
          </button>
        ))}
        <button
          type="button"
          className="button-secondary"
          disabled={paged.page >= paged.pageCount}
          onClick={() => setPage(paged.page + 1)}
          aria-label={t("routes:followedPageNext")}
        >
          ›
        </button>
        <button
          type="button"
          className="button-secondary"
          disabled={paged.page >= paged.pageCount}
          onClick={() => setPage(paged.pageCount)}
          aria-label={t("routes:followedPageLast")}
        >
          »
        </button>
      </div>
    </div>
  );
}

function FollowedPageView({ model }: { model: FollowedPageModel }) {
  const { t, loggedIn } = model;
  return (
    <section className="page page--followed">
      <PageSubbar
        title={t("routes:followedTitle")}
        lede={t("routes:followedLede")}
        actions={loggedIn ? <FollowedToolbar model={model} /> : undefined}
      />
      <FollowedBody model={model} />
      <FollowedPager model={model} />
    </section>
  );
}

export function FollowedPage() {
  const model = useFollowedPageModel();
  return <FollowedPageView model={model} />;
}
