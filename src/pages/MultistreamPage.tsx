import { useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { PageSubbar } from "../components/AppShell";
import {
  ChevronRightIcon,
  GripIcon,
  InfoIcon,
  SearchIcon,
  VerifiedBadge,
} from "../components/FollowedIcons";
import { useAuthStore } from "../lib/auth/store";
import { formatViewers } from "../lib/browse/format";
import { useFollowedLiveStreams } from "../lib/browse/useFollowedLive";
import { useSettingsStore } from "../lib/settings/store";
import { useWatchingStore, type StreamSession } from "../lib/streaming/store";
import {
  watchingPhase,
  watchingStatusText,
} from "../lib/streaming/status";
import { moveItemAt, slotHitY, slotIndexFromClientY, slotShiftY } from "../lib/streaming/reorder";
import {
  getFollowedChannelLogins,
  getUsersByLogin,
  isTwitchPartner,
  searchChannels,
  streamThumbnail,
  type HelixChannel,
  type HelixStream,
  type HelixUser,
} from "../lib/twitch/helix";
import {
  isMultistreamLayout,
  isUnevenLayout,
  isUnevenMainSide,
  layoutCapacity,
  MULTISTREAM_LAYOUTS,
  UNEVEN_MAIN_SIDES,
} from "../lib/streaming/layout";
import "./MultistreamPage.css";

/** watchStream only reads user_login/title/game_name — fill the rest. */
function streamLike(login: string, title = "", game = ""): HelixStream {
  return {
    id: "",
    user_id: "",
    user_login: login,
    user_name: login,
    game_id: "",
    game_name: game,
    type: "live",
    title,
    viewer_count: 0,
    started_at: "",
    language: "",
    thumbnail_url: "",
    is_mature: false,
  };
}

function useMultistreamPage() {
  const { t } = useTranslation(["multistream", "settings", "common", "routes"]);
  const session = useAuthStore((s) => s.session);
  const userId = session?.userId ?? null;
  const loggedIn = Boolean(session?.loggedIn);

  const sessions = useWatchingStore((s) => s.sessions);
  const slotChannels = useWatchingStore((s) => s.slotChannels);
  const activeChatChannel = useWatchingStore((s) => s.activeChatChannel);
  const launchError = useWatchingStore((s) => s.error);
  const watchStream = useWatchingStore((s) => s.watchStream);
  const stopSession = useWatchingStore((s) => s.stopSession);
  const stopAll = useWatchingStore((s) => s.stopAll);
  const toggleMute = useWatchingStore((s) => s.toggleMute);
  const reorderSlots = useWatchingStore((s) => s.reorderSlots);
  const setActiveChat = useWatchingStore((s) => s.setActiveChat);
  const applyLayout = useWatchingStore((s) => s.applyLayout);
  const refresh = useWatchingStore((s) => s.refresh);

  const settings = useSettingsStore((s) => s.settings);
  const setSettings = useSettingsStore((s) => s.setSettings);
  const chatProvider = settings.chat.provider;
  const multi = !settings.streaming.seamlessSwitch;

  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);
  const liveRef = useRef<HTMLUListElement>(null);
  const slotsRef = useRef<HTMLUListElement>(null);
  const ghostRef = useRef<HTMLDivElement>(null);
  const dragIndexRef = useRef<number | null>(null);
  const overIndexRef = useRef<number | null>(null);
  const dragStartYRef = useRef(0);
  const slotDraggingRef = useRef(false);
  const pointerRef = useRef({ x: 0, y: 0 });
  const dragMetricsRef = useRef<{
    grabX: number;
    grabY: number;
    width: number;
    height: number;
  } | null>(null);
  const dragStrideRef = useRef(0);
  const slotLayoutRef = useRef<Array<{ top: number; bottom: number }>>([]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(query.trim()), 300);
    return () => window.clearTimeout(id);
  }, [query]);

  const follows = useQuery({
    queryKey: ["followed-channel-logins", userId],
    enabled: loggedIn && Boolean(userId),
    queryFn: () => getFollowedChannelLogins(userId!),
    staleTime: 5 * 60_000,
  });
  const followedSet = useMemo(
    () => new Set(follows.data ?? []),
    [follows.data],
  );

  const search = useQuery({
    queryKey: ["multistream-search", debounced],
    enabled: loggedIn && debounced.length >= 2,
    queryFn: () => searchChannels(debounced),
  });

  const followedLive = useFollowedLiveStreams();

  const sessionByChannel = useMemo(() => {
    const map = new Map<string, StreamSession>();
    for (const s of sessions) {
      map.set(s.channel.toLowerCase(), s);
    }
    return map;
  }, [sessions]);

  const slotUsersQuery = useQuery({
    queryKey: ["multistream-slot-users", slotChannels],
    enabled: slotChannels.length > 0,
    queryFn: async () => {
      const page = await getUsersByLogin(slotChannels);
      const record: Record<string, HelixUser> = {};
      for (const user of page.data) {
        record[user.login.toLowerCase()] = user;
      }
      return record;
    },
  });
  const slotUsers = slotUsersQuery.data ?? {};

  const runningCount = sessions.filter((s) => s.running).length;
  const cap = layoutCapacity(
    isMultistreamLayout(settings.streaming.multistreamLayout)
      ? settings.streaming.multistreamLayout
      : "2x2",
  );
  const layoutFull = runningCount >= cap;

  const addChannel = (login: string, title = "", game = "") => {
    void watchStream(streamLike(login, title, game)).catch(() => undefined);
  };

  const results = search.data?.data ?? [];
  const followedResults = results.filter((ch) =>
    followedSet.has(ch.broadcaster_login.toLowerCase()),
  );
  const otherResults = results.filter(
    (ch) => !followedSet.has(ch.broadcaster_login.toLowerCase()),
  );

  const isAdded = (login: string) =>
    slotChannels.includes(login.toLowerCase());

  const positionGhost = () => {
    const ghost = ghostRef.current;
    const metrics = dragMetricsRef.current;
    if (!ghost || !metrics) return;
    ghost.style.width = `${metrics.width}px`;
    ghost.style.transform = `translate(${pointerRef.current.x - metrics.grabX}px, ${pointerRef.current.y - metrics.grabY}px)`;
  };

  useLayoutEffect(() => {
    if (dragIndex === null) return;
    positionGhost();
  }, [dragIndex]);

  const endSlotDrag = (pointerId: number, target: HTMLLIElement) => {
    if (dragIndexRef.current === null) return;
    if (target.hasPointerCapture(pointerId)) {
      target.releasePointerCapture(pointerId);
    }
    const from = dragIndexRef.current;
    const to = overIndexRef.current;
    const moved = slotDraggingRef.current;
    dragIndexRef.current = null;
    overIndexRef.current = null;
    slotDraggingRef.current = false;
    dragMetricsRef.current = null;
    dragStrideRef.current = 0;
    slotLayoutRef.current = [];
    setDragIndex(null);
    setOverIndex(null);
    if (moved && from !== null && to !== null && from !== to) {
      reorderSlots(moveItemAt(slotChannels, from, to));
    }
  };

  const onSlotPointerDown = (
    event: PointerEvent<HTMLLIElement>,
    index: number,
  ) => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    if (target.closest("button, a, input, select, textarea")) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    const rect = event.currentTarget.getBoundingClientRect();
    pointerRef.current = { x: event.clientX, y: event.clientY };
    dragMetricsRef.current = {
      grabX: event.clientX - rect.left,
      grabY: event.clientY - rect.top,
      width: rect.width,
      height: rect.height,
    };
    dragIndexRef.current = index;
    overIndexRef.current = index;
    dragStartYRef.current = event.clientY;
    slotDraggingRef.current = false;
  };

  const onSlotPointerMove = (event: PointerEvent<HTMLLIElement>) => {
    if (dragIndexRef.current === null) return;
    pointerRef.current = { x: event.clientX, y: event.clientY };
    if (!slotDraggingRef.current) {
      if (Math.abs(event.clientY - dragStartYRef.current) < 5) return;
      slotDraggingRef.current = true;
      const list = slotsRef.current;
      if (list) {
        const rects = [
          ...list.querySelectorAll<HTMLElement>("[data-ms-slot]"),
        ].map((el) => {
          const row = el.getBoundingClientRect();
          return { top: row.top, bottom: row.bottom };
        });
        slotLayoutRef.current = rects;
        dragStrideRef.current =
          rects.length >= 2
            ? rects[1]!.top - rects[0]!.top
            : (rects[0]?.bottom ?? 0) - (rects[0]?.top ?? 0);
      }
      setDragIndex(dragIndexRef.current);
      setOverIndex(overIndexRef.current);
    }
    positionGhost();
    const metrics = dragMetricsRef.current;
    const layout = slotLayoutRef.current;
    if (!metrics || !layout.length) return;
    const next = slotIndexFromClientY(
      layout,
      slotHitY(event.clientY, metrics.grabY, metrics.height),
    );
    if (next === null || next === overIndexRef.current) return;
    overIndexRef.current = next;
    setOverIndex(next);
  };

  const renderResult = (ch: HelixChannel) => {
    const added = isAdded(ch.broadcaster_login);
    return (
      <li key={ch.id} className="ms-result">
        {ch.thumbnail_url ? (
          <img
            src={ch.thumbnail_url}
            alt=""
            className="ms-result__thumb"
            loading="lazy"
          />
        ) : null}
        <div className="ms-result__body">
          <strong>
            {ch.display_name}
            {ch.is_live ? (
              <span className="badge badge--live ms-result__live">
                {t("multistream:live")}
              </span>
            ) : (
              <span className="muted ms-result__live">
                {t("multistream:offline")}
              </span>
            )}
          </strong>
          <span className="ms-result__title">
            {ch.game_name ? `${ch.game_name} · ` : ""}
            {ch.title}
          </span>
        </div>
        <button
          type="button"
          className="button-secondary"
          disabled={added || layoutFull}
          onClick={() => addChannel(ch.broadcaster_login, ch.title, ch.game_name)}
        >
          {added ? t("multistream:added") : t("multistream:add")}
        </button>
      </li>
    );
  };


  const draggingChannel =
    dragIndex !== null ? slotChannels[dragIndex] : undefined;
  const draggingSession = draggingChannel
    ? sessionByChannel.get(draggingChannel)
    : undefined;
  const draggingUser = draggingChannel
    ? slotUsers[draggingChannel]
    : undefined;
  const draggingPhase = watchingPhase(draggingSession?.phase);
  const draggingStatus = draggingSession
    ? watchingStatusText(draggingPhase, draggingSession.status, t)
    : "";

  if (!multi) return { kind: "seamless" as const, t };

  return {
    kind: "page" as const,
    t,
    sessions,
    stopAll,
    settings,
    setSettings,
    applyLayout,
    runningCount,
    cap,
    layoutFull,
    launchError,
    slotChannels,
    sessionByChannel,
    slotUsers,
    dragIndex,
    overIndex,
    dragStrideRef,
    slotsRef,
    ghostRef,
    onSlotPointerDown,
    onSlotPointerMove,
    endSlotDrag,
    activeChatChannel,
    chatProvider,
    setActiveChat,
    toggleMute,
    stopSession,
    draggingChannel,
    draggingUser,
    draggingSession,
    draggingPhase,
    draggingStatus,
    loggedIn,
    query,
    setQuery,
    debounced,
    followedLive,
    liveRef,
    isAdded,
    addChannel,
    followedResults,
    otherResults,
    search,
    renderResult,
  };
}

type MultistreamPageModel = Extract<
  ReturnType<typeof useMultistreamPage>,
  { kind: "page" }
>;

function MultistreamSeamlessNote({
  t,
}: {
  t: ReturnType<typeof useMultistreamPage>["t"];
}) {
  return (
    <section className="page">
      <PageSubbar
        title={t("multistream:title")}
        lede={t("multistream:lede")}
      />
      <p className="muted">{t("multistream:seamlessNote")}</p>
    </section>
  );
}

function MultistreamLayoutControls({
  t,
  settings,
  setSettings,
  applyLayout,
  runningCount,
  cap,
  layoutFull,
  launchError,
}: MultistreamPageModel) {
  return (
      <div className="ms-section">
        <label className="ms-layout-row">
          <span>{t("multistream:layoutLabel")}</span>
          <select
            value={settings.streaming.multistreamLayout}
            onChange={(e) => {
              const value = e.target.value;
              if (!isMultistreamLayout(value)) return;
              if (runningCount > layoutCapacity(value)) {
                useWatchingStore.setState({
                  error: `Layout holds ${layoutCapacity(value)} streams. Stop extras first.`,
                });
                return;
              }
              setSettings({
                streaming: { ...settings.streaming, multistreamLayout: value },
              });
              applyLayout();
            }}
          >
            {MULTISTREAM_LAYOUTS.map((layout) => (
              <option key={layout} value={layout}>
                {t(`settings:layout${layout}`)}
              </option>
            ))}
          </select>
        </label>
        {isUnevenLayout(settings.streaming.multistreamLayout) ? (
          <label className="ms-layout-row">
            <span>{t("settings:unevenMainSide")}</span>
            <select
              value={settings.streaming.unevenMainSide}
              onChange={(e) => {
                const value = e.target.value;
                if (!isUnevenMainSide(value)) return;
                setSettings({
                  streaming: { ...settings.streaming, unevenMainSide: value },
                });
                applyLayout();
              }}
            >
              {UNEVEN_MAIN_SIDES.map((side) => (
                <option key={side} value={side}>
                  {t(
                    `settings:mainSide${side[0]!.toUpperCase()}${side.slice(1)}`,
                  )}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <p className="muted ms-slots-meta">
          {t("multistream:slotsUsed", { used: runningCount, cap })}
          {layoutFull ? ` — ${t("multistream:layoutFull")}` : ""}
        </p>
        {launchError ? <p className="muted">{launchError}</p> : null}
      </div>
  );
}

function MultistreamSlotList({
  t,
  slotChannels,
  sessionByChannel,
  slotUsers,
  dragIndex,
  overIndex,
  dragStrideRef,
  slotsRef,
  ghostRef,
  onSlotPointerDown,
  onSlotPointerMove,
  endSlotDrag,
  activeChatChannel,
  chatProvider,
  setActiveChat,
  toggleMute,
  stopSession,
  draggingChannel,
  draggingUser,
  draggingSession,
  draggingPhase,
  draggingStatus,
}: MultistreamPageModel) {
  return (
      <div className="ms-section">
        <div className="ms-section__head">
          <h2>{t("multistream:currentStreams")}</h2>
          <span className="muted">{t("multistream:dragHint")}</span>
        </div>
        {!slotChannels.length ? (
          <p className="muted">{t("multistream:currentEmpty")}</p>
        ) : (
          <>
          <ul
            className={["ms-slots", dragIndex !== null ? "ms-slots--dragging" : ""]
              .filter(Boolean)
              .join(" ")}
            ref={slotsRef}
          >
            {slotChannels.map((channel, index) => {
              const s = sessionByChannel.get(channel);
              const user = slotUsers[channel];
              const phase = watchingPhase(s?.phase);
              const status = watchingStatusText(phase, s?.status, t);
              const chatActive =
                (activeChatChannel ?? slotChannels[0]) === channel;
              const shift =
                dragIndex !== null && overIndex !== null
                  ? slotShiftY(
                      index,
                      dragIndex,
                      overIndex,
                      dragStrideRef.current,
                    )
                  : 0;
              return (
                <li
                  key={channel}
                  data-ms-slot={index}
                  className={[
                    "ms-slot",
                    dragIndex === index ? "ms-slot--placeholder" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  style={
                    dragIndex !== null
                      ? { transform: `translateY(${shift}px)` }
                      : undefined
                  }
                  onPointerDown={(event) => onSlotPointerDown(event, index)}
                  onPointerMove={onSlotPointerMove}
                  onPointerUp={(event) =>
                    endSlotDrag(event.pointerId, event.currentTarget)
                  }
                  onPointerCancel={(event) =>
                    endSlotDrag(event.pointerId, event.currentTarget)
                  }
                  onDragStart={(event) => event.preventDefault()}
                >
                  <span className="ms-slot__handle" aria-hidden>
                    <GripIcon />
                  </span>
                  {user?.profile_image_url ? (
                    <img
                      src={user.profile_image_url}
                      alt=""
                      className="ms-slot__avatar"
                      draggable={false}
                    />
                  ) : (
                    <span className="ms-slot__avatar ms-slot__avatar--empty" />
                  )}
                  <div className="ms-slot__meta">
                    <div className="ms-slot__title">
                      <strong>{user?.display_name ?? s?.channel ?? channel}</strong>
                      {isTwitchPartner(user) ? <VerifiedBadge /> : null}
                      {s?.game ? (
                        <span className="muted"> • {s.game}</span>
                      ) : null}
                    </div>
                    {status ? (
                      <p
                        className={`ms-slot__status ms-slot__status--${phase}`}
                        title={s?.status}
                      >
                        {status}
                      </p>
                    ) : null}
                  </div>
                  <div className="ms-slot__actions">
                    {chatProvider === "embedded" ? (
                      <button
                        type="button"
                        className={`button-secondary${chatActive ? " ms-chat-active" : ""}`}
                        aria-pressed={chatActive}
                        onClick={() => setActiveChat(channel)}
                      >
                        {chatActive
                          ? t("multistream:chatActive")
                          : t("multistream:chatPick")}
                      </button>
                    ) : null}
                    {s ? (
                      <>
                        <button
                          type="button"
                          className={`button-secondary${s.muted ? " ms-muted" : ""}`}
                          aria-pressed={Boolean(s.muted)}
                          title={
                            s.muted
                              ? t("multistream:unmute")
                              : t("multistream:mute")
                          }
                          onClick={() => void toggleMute(s.id)}
                        >
                          {s.muted
                            ? t("multistream:unmute")
                            : t("multistream:mute")}
                        </button>
                        <button
                          type="button"
                          onClick={() => void stopSession(s.id)}
                        >
                          {t("multistream:stop")}
                        </button>
                      </>
                    ) : null}
                  </div>
                </li>
              );
            })}
          </ul>
          {dragIndex !== null && draggingChannel ? (
            <div
              ref={ghostRef}
              className="ms-slot ms-slot-ghost"
              aria-hidden
            >
              <span className="ms-slot__handle">
                <GripIcon />
              </span>
              {draggingUser?.profile_image_url ? (
                <img
                  src={draggingUser.profile_image_url}
                  alt=""
                  className="ms-slot__avatar"
                  draggable={false}
                />
              ) : (
                <span className="ms-slot__avatar ms-slot__avatar--empty" />
              )}
              <div className="ms-slot__meta">
                <div className="ms-slot__title">
                  <strong>
                    {draggingUser?.display_name ??
                      draggingSession?.channel ??
                      draggingChannel}
                  </strong>
                  {isTwitchPartner(draggingUser) ? <VerifiedBadge /> : null}
                  {draggingSession?.game ? (
                    <span className="muted"> • {draggingSession.game}</span>
                  ) : null}
                </div>
                {draggingStatus ? (
                  <p
                    className={`ms-slot__status ms-slot__status--${draggingPhase}`}
                  >
                    {draggingStatus}
                  </p>
                ) : null}
              </div>
            </div>
          ) : null}
          </>
        )}
        {chatProvider === "chatterino" && slotChannels.length ? (
          <p className="muted ms-chatterino-note">
            {t("multistream:chatterinoNote")}
          </p>
        ) : null}
      </div>
  );
}

function MultistreamSearchPanel({
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

export function MultistreamPage() {
  const model = useMultistreamPage();
  if (model.kind === "seamless") {
    return <MultistreamSeamlessNote t={model.t} />;
  }
  return (
    <section className="page">
      <PageSubbar
        title={model.t("multistream:title")}
        lede={model.t("multistream:lede")}
        actions={
          model.sessions.length ? (
            <button
              type="button"
              className="button-secondary"
              onClick={() => void model.stopAll()}
            >
              {model.t("multistream:stopAll")}
            </button>
          ) : undefined
        }
      />
      <MultistreamLayoutControls {...model} />
      <MultistreamSlotList {...model} />
      <MultistreamSearchPanel {...model} />
      <p className="ms-footnote">
        <InfoIcon />
        <span>{model.t("multistream:layoutHint")}</span>
      </p>
    </section>
  );
}
