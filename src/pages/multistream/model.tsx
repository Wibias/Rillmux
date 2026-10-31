import { useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useAuthStore } from "../../lib/auth/store";
import { useFollowedLiveStreams } from "../../lib/browse/useFollowedLive";
import { useSettingsStore } from "../../lib/settings/store";
import { useWatchingStore, type StreamSession } from "../../lib/streaming/store";
import { watchingPhase, watchingStatusText } from "../../lib/streaming/status";
import { moveItemAt, slotHitY, slotIndexFromClientY } from "../../lib/streaming/reorder";
import { getFollowedChannelLogins, getUsersByLogin, searchChannels, type HelixChannel, type HelixStream, type HelixUser } from "../../lib/twitch/helix";
import { isMultistreamLayout, layoutCapacity } from "../../lib/streaming/layout";

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

export function useMultistreamPage() {
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

export type MultistreamPageModel = Extract<
  ReturnType<typeof useMultistreamPage>,
  { kind: "page" }
>;

