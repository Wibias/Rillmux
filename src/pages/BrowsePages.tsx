import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  useInfiniteQuery,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { DoctorPanel } from "../components/DoctorPanel";
import { ChangelogDialog } from "../components/ChangelogDialog";
import { EmbeddedChat } from "../components/EmbeddedChat";
import { LoadMore } from "../components/LoadMore";
import { LoadingGrid } from "../components/LoadingGrid";
import { PageRefreshButton } from "../components/PageRefreshButton";
import { PinnedFavourites } from "../components/PinnedFavourites";
import { StreamGrid } from "../components/StreamGrid";
import { StreamList } from "../components/StreamList";
import { PageSubbar } from "../components/AppShell";
import { useUpdaterCheck } from "../components/DeepLinkAndUpdaterBootstrap";
import { useAuthStore } from "../lib/auth/store";
import { useWatchingStore } from "../lib/streaming/store";
import {
  watchingPhase,
  watchingStatusText,
} from "../lib/streaming/status";
import { LanguageFilter } from "../components/LanguageFilter";
import {
  FilterIcon,
  GridViewIcon,
  GripIcon,
  ListViewIcon,
  SearchIcon,
} from "../components/FollowedIcons";
import {
  getFollowedStreams,
  getTopGames,
  getTopStreams,
  getUsersByLogin,
  LIVE_STREAM_QUERY,
} from "../lib/twitch/helix";
import { languagesQueryKey } from "../lib/twitch/languages";
import { useSettingsStore } from "../lib/settings/store";
import { isTauri } from "../lib/tauri";
import { useFollowedLiveStreams } from "../lib/browse/useFollowedLive";
import { followedStreamsQueryKey } from "../lib/notifications/followedLive";
import {
  filterFollowedStreams,
  followedVisibleCount,
  paginateItems,
  partitionPinned,
  sortFollowedStreams,
  togglePinnedLogin,
  type FollowedSort,
  type FollowedView,
} from "../lib/browse/followedList";
import "./FollowedPage.css";

export function FollowedPage() {
  const { t } = useTranslation(["routes", "common"]);
  const session = useAuthStore((s) => s.session);
  const watchStream = useWatchingStore((s) => s.watchStream);
  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const loggedIn = Boolean(session?.loggedIn && session.userId);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const [fitSize, setFitSize] = useState(12);
  const [pinsCollapsed, setPinsCollapsed] = useState(false);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const filtersRef = useRef<HTMLDivElement>(null);
  const fitRef = useRef<HTMLDivElement>(null);

  const { query, streams } = useFollowedLiveStreams();

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
  }, []);

  useEffect(() => {
    if (!filtersOpen) return;
    const onPointer = (event: PointerEvent) => {
      if (!filtersRef.current?.contains(event.target as Node)) {
        setFiltersOpen(false);
      }
    };
    document.addEventListener("pointerdown", onPointer);
    return () => document.removeEventListener("pointerdown", onPointer);
  }, [filtersOpen]);

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
  const usersQuery = useQuery({
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
  const usersByLogin = usersQuery.data ?? {};

  useEffect(() => {
    const node = fitRef.current;
    if (!node) return;
    const apply = () => {
      const rect = node.getBoundingClientRect();
      if (rect.width < 80 || rect.height < 80) return;
      const next = followedVisibleCount({
        view: settings.gui.followedView,
        width: rect.width,
        height: rect.height,
      });
      setFitSize((current) => (current === next ? current : next));
    };
    apply();
    const observer = new ResizeObserver(apply);
    observer.observe(node);
    return () => observer.disconnect();
  }, [
    settings.gui.followedView,
    pinsCollapsed,
    pinned.length,
    loggedIn,
    rest.length,
  ]);

  useEffect(() => {
    setPage(1);
  }, [
    search,
    settings.gui.followedSort,
    settings.gui.followedView,
    settings.gui.hideMatureFollowed,
  ]);

  useEffect(() => {
    if (page !== paged.page) setPage(paged.page);
  }, [page, paged.page]);

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

  const pageButtons = (() => {
    const total = paged.pageCount;
    if (total <= 9) {
      return Array.from({ length: total }, (_, i) => i + 1);
    }
    const start = Math.max(1, Math.min(paged.page - 3, total - 8));
    return Array.from({ length: 9 }, (_, i) => start + i);
  })();

  return (
    <section className="page page--followed">
      <PageSubbar
        title={t("routes:followedTitle")}
        lede={t("routes:followedLede")}
        actions={
          loggedIn ? (
          <div className="followed-toolbar">
          <label className="followed-search">
            <span className="followed-search__icon">
              <SearchIcon />
            </span>
            <input
              ref={searchRef}
              className="input"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
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
                    onChange={(e) =>
                      patchGui({ hideMatureFollowed: e.target.checked })
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
            onChange={(e) =>
              patchGui({ followedSort: e.target.value as FollowedSort })
            }
            aria-label={t("routes:followedSortViewersDesc")}
          >
            <option value="viewers-desc">
              {t("routes:followedSortViewersDesc")}
            </option>
            <option value="viewers-asc">
              {t("routes:followedSortViewersAsc")}
            </option>
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
          ) : undefined
        }
      />

      <div className="followed-body">
      {!loggedIn ? <p className="muted">{t("routes:followedLoginRequired")}</p> : null}
      {query.isLoading ? <LoadingGrid /> : null}
      {query.isError ? (
        <p className="muted">{(query.error as Error).message}</p>
      ) : null}
      {loggedIn && !query.isLoading && streams.length === 0 ? (
        <div className="empty-panel">
          <strong>{t("routes:followedEmpty")}</strong>
        </div>
      ) : null}

      {streams.length ? (
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
      ) : null}
      </div>
      {streams.length > 0 && rest.length > 0 ? (
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
          ) : null}
    </section>
  );
}

export function StreamsPage() {
  const { t } = useTranslation(["routes", "common"]);
  const session = useAuthStore((s) => s.session);
  const authLoading = useAuthStore((s) => s.loading);
  const watchStream = useWatchingStore((s) => s.watchStream);
  const loggedIn = Boolean(session?.loggedIn);
  const streamLanguages = useSettingsStore(
    (s) => s.settings.streaming.streamLanguages,
  );
  const langKey = languagesQueryKey(streamLanguages);

  const query = useInfiniteQuery({
    queryKey: ["top-streams", langKey],
    enabled: loggedIn,
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) => getTopStreams(pageParam, streamLanguages),
    getNextPageParam: (last) => last.pagination?.cursor,
    ...LIVE_STREAM_QUERY,
  });

  const streams = query.data?.pages.flatMap((p) => p.data) ?? [];
  const refreshing = query.isFetching && !query.isFetchingNextPage;
  const userLogins = useMemo(
    () => [...new Set(streams.map((stream) => stream.user_login.toLowerCase()))],
    [streams],
  );
  const usersQuery = useQuery({
    queryKey: ["top-stream-users", userLogins],
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

  return (
    <section className="page">
      <PageSubbar
        title={t("routes:streamsTitle")}
        lede={t("routes:streamsLede")}
        actions={
          loggedIn ? (
            <>
              <LanguageFilter />
              <PageRefreshButton
                refreshing={refreshing}
                onRefresh={() => void query.refetch()}
              />
            </>
          ) : undefined
        }
      />
      {!loggedIn && !authLoading ? (
        <p className="muted">{t("routes:followedLoginRequired")}</p>
      ) : null}
      {(authLoading || query.isLoading) && !streams.length ? (
        <LoadingGrid />
      ) : null}
      {query.isError ? (
        <p className="muted">{(query.error as Error).message}</p>
      ) : null}
      {streams.length ? (
        <>
          <StreamGrid
            streams={streams}
            onWatch={(stream) => {
              void watchStream(stream);
            }}
            usersByLogin={usersQuery.data ?? {}}
          />
          <LoadMore
            hasMore={Boolean(query.hasNextPage)}
            isFetching={query.isFetchingNextPage}
            onLoadMore={() => void query.fetchNextPage()}
          />
        </>
      ) : null}
    </section>
  );
}

export function WatchingPage() {
  const { t } = useTranslation(["routes", "common"]);
  const sessions = useWatchingStore((s) => s.sessions);
  const slotChannels = useWatchingStore((s) => s.slotChannels);
  const refresh = useWatchingStore((s) => s.refresh);
  const stopSession = useWatchingStore((s) => s.stopSession);
  const stopAll = useWatchingStore((s) => s.stopAll);
  const toggleMute = useWatchingStore((s) => s.toggleMute);
  const moveSlot = useWatchingStore((s) => s.moveSlot);
  const activeChatChannel = useWatchingStore((s) => s.activeChatChannel);
  const setActiveChat = useWatchingStore((s) => s.setActiveChat);
  const settings = useSettingsStore((s) => s.settings);
  const chatProvider = settings.chat.provider;
  const multi = !settings.streaming.seamlessSwitch;
  const launchError = useWatchingStore((s) => s.error);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const orderedSessions = multi
    ? slotChannels
        .map((ch) =>
          sessions.find((s) => s.channel.toLowerCase() === ch && s.running),
        )
        .filter((s): s is NonNullable<typeof s> => Boolean(s))
    : sessions;

  return (
    <section className="watching-layout">
      <div className="watching-layout__main">
        <PageSubbar
          title={t("routes:watchingTitle")}
          lede={t("routes:watchingLede")}
          actions={
            sessions.length ? (
              <button
                type="button"
                className="button-secondary"
                onClick={() => void stopAll()}
              >
                {t("routes:watchingStopAll")}
              </button>
            ) : undefined
          }
        />
        {launchError ? <p className="muted">{launchError}</p> : null}
        {!sessions.length ? (
          <div className="empty-panel">
            <strong>{t("routes:watchingEmpty")}</strong>
          </div>
        ) : null}
        <ul className="watching-list">
          {orderedSessions.map((session, index) => {
            const phase = watchingPhase(session.phase);
            const status = watchingStatusText(phase, session.status, t);
            return (
            <li key={session.id} className="watching-list__item">
              <span className="watching-list__handle" aria-hidden>
                <GripIcon />
              </span>
              <div className="watching-list__meta">
                <div className="watching-list__title">
                  {multi ? <span className="muted">#{index + 1} </span> : null}
                  <strong>{session.channel}</strong>
                  {session.game ? (
                    <span className="muted"> • {session.game}</span>
                  ) : null}
                </div>
                {status ? (
                  <p
                    className={`watching-list__status watching-list__status--${phase}`}
                    title={session.status}
                  >
                    {status}
                  </p>
                ) : null}
              </div>
              <div className="watching-list__actions">
                {multi ? (
                  <>
                    <button
                      type="button"
                      className="button-secondary"
                      disabled={index === 0}
                      onClick={() => moveSlot(session.channel, -1)}
                      aria-label="Move up"
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      className="button-secondary"
                      disabled={index >= orderedSessions.length - 1}
                      onClick={() => moveSlot(session.channel, 1)}
                      aria-label="Move down"
                    >
                      ↓
                    </button>
                  </>
                ) : null}
                {chatProvider === "embedded" ? (
                  <button
                    type="button"
                    className="button-secondary"
                    onClick={() => setActiveChat(session.channel)}
                  >
                    {t("routes:chatTitle", { channel: session.channel })}
                  </button>
                ) : null}
                <button
                  type="button"
                  className="button-secondary"
                  aria-pressed={Boolean(session.muted)}
                  onClick={() => void toggleMute(session.id)}
                >
                  {session.muted ? "Unmute" : "Mute"}
                </button>
                <button type="button" onClick={() => void stopSession(session.id)}>
                  {t("common:stop")}
                </button>
              </div>
            </li>
            );
          })}
        </ul>
      </div>
      {chatProvider === "embedded" ? (
        <EmbeddedChat channel={activeChatChannel ?? sessions[0]?.channel ?? null} />
      ) : null}
    </section>
  );
}

export function AboutPage() {
  const { t } = useTranslation("routes");
  const { status, version, error, check, install } = useUpdaterCheck();
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [showChangelog, setShowChangelog] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        if (isTauri()) {
          const { getVersion } = await import("@tauri-apps/api/app");
          const v = await getVersion();
          if (!cancelled) setAppVersion(v);
          return;
        }
      } catch {
        // fall through to package version
      }
      if (!cancelled) {
        setAppVersion(import.meta.env.VITE_APP_VERSION ?? null);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <section className="page">
      <header className="page__header">
        <div>
          <h1>{t("aboutTitle")}</h1>
          <p className="page__lede">{t("aboutBlurb")}</p>
          {appVersion ? (
            <p className="muted about-version" style={{ marginTop: "0.35rem" }}>
              {t("aboutVersion", { version: appVersion })}
              <button
                type="button"
                className="button-secondary"
                onClick={() => setShowChangelog(true)}
              >
                {t("viewChangelog")}
              </button>
            </p>
          ) : null}
        </div>
      </header>
      <div className="channel-header__actions" style={{ marginBottom: "1rem" }}>
        <button
          type="button"
          className="button-secondary"
          disabled={status === "checking"}
          onClick={() => void check()}
        >
          {status === "checking" ? t("updateChecking") : t("checkUpdates")}
        </button>
        {status === "available" && version ? (
          <button type="button" onClick={() => void install()}>
            {t("updateInstall")} ({version})
          </button>
        ) : null}
      </div>
      {status === "available" && version ? (
        <p>{t("updateAvailable", { version })}</p>
      ) : null}
      {status === "none" ? <p className="muted">{t("updateNone")}</p> : null}
      {status === "error" ? (
        <p className="authbar__error">
          {t("updateError")}
          {error ? ` — ${error}` : ""}
        </p>
      ) : null}
      <DoctorPanel />
      {showChangelog ? (
        <ChangelogDialog onClose={() => setShowChangelog(false)} />
      ) : null}
    </section>
  );
}

export function AuthBootstrap({ children }: { children: React.ReactNode }) {
  const refreshSession = useAuthStore((s) => s.refreshSession);
  const session = useAuthStore((s) => s.session);
  const queryClient = useQueryClient();

  useEffect(() => {
    void refreshSession();
  }, [refreshSession]);

  useEffect(() => {
    if (!session?.loggedIn) {
      return;
    }
    void queryClient.prefetchInfiniteQuery({
      queryKey: ["top-streams"],
      initialPageParam: undefined as string | undefined,
      queryFn: ({ pageParam }) => getTopStreams(pageParam),
      getNextPageParam: (last) => last.pagination?.cursor,
      staleTime: 20_000,
      pages: 1,
    });
    void queryClient.prefetchInfiniteQuery({
      queryKey: ["top-games"],
      initialPageParam: undefined as string | undefined,
      queryFn: ({ pageParam }) => getTopGames(pageParam),
      getNextPageParam: (last) => last.pagination?.cursor,
      staleTime: 60_000,
      pages: 1,
    });
    if (session.userId) {
      void queryClient.prefetchInfiniteQuery({
        queryKey: followedStreamsQueryKey(session.userId),
        initialPageParam: undefined as string | undefined,
        queryFn: ({ pageParam }) =>
          getFollowedStreams(session.userId!, pageParam),
        getNextPageParam: (last) => last.pagination?.cursor,
        staleTime: 20_000,
        pages: 1,
      });
    }
  }, [session?.loggedIn, session?.userId, queryClient]);

  return children;
}
