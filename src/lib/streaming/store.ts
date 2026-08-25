import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { invoke, isTauri } from "../tauri";
import type { HelixStream } from "../twitch/helix";
import { getChannelStreams, getUsersByLogin } from "../twitch/helix";
import { useSettingsStore } from "../settings/store";
import { resolveChannelLaunch } from "../settings/types";
import { captureAppError } from "../sentry";
import { debugRuntimeEvent } from "../diagnostics/runtimeDebug";
import {
  DEFAULT_MULTISTREAM_LAYOUT,
  isMultistreamLayout,
  layoutCapacity,
  type MultistreamLayout,
} from "./layout";
import { nextSessionStatus, sessionStatusPatch } from "./status";
import type { RaidOutgoingEvent } from "./raid";
import {
  chatterinoShouldCloseOnEmpty,
  chatterinoShouldSkipSync,
  chatterinoSyncKey,
} from "./chatterinoSync";
import {
  buildPresenceTargets,
  presenceSourceFromStream,
  prunePresenceMetadata,
  type PresenceMetadata,
} from "./presence";

export interface StreamSession {
  id: string;
  channel: string;
  quality: string;
  title?: string | null;
  game?: string | null;
  running: boolean;
  status?: string;
  phase?: string;
  ready?: boolean;
  muted?: boolean;
}

export interface StreamStatusEvent {
  id: string;
  channel: string;
  line: string;
  status: string;
  phase: string;
  ready: boolean;
}

interface WatchingState {
  sessions: StreamSession[];
  /** Ordered multistream slots (lowercase logins). Ignored when seamless is on. */
  slotChannels: string[];
  activeChatChannel: string | null;
  error: string | null;
  refresh: () => Promise<void>;
  watchStream: (stream: HelixStream) => Promise<void>;
  /** Replace one watching slot after an outgoing raid (never kills other sessions). */
  followRaid: (raid: RaidOutgoingEvent) => Promise<void>;
  stopSession: (id: string) => Promise<void>;
  stopAll: () => Promise<void>;
  toggleMute: (id: string) => Promise<void>;
  moveSlot: (channel: string, direction: -1 | 1) => void;
  /** Drag & drop reorder: replace the slot order outright (same channels). */
  reorderSlots: (channels: string[]) => void;
  /** Retile + resync chat after layout preset changes. */
  applyLayout: () => void;
  setActiveChat: (channel: string | null) => void;
  applyStatus: (payload: StreamStatusEvent) => void;
}

let listenersBound = false;
let lastChatSyncKey = "";
let chatSyncInflight = "";
let chatSyncGeneration = 0;
let layoutTimer: ReturnType<typeof setTimeout> | null = null;
let presenceMetadata: PresenceMetadata = {};
let lastPresenceSyncKey = "";

function rememberPresence(sessionId: string, stream: HelixStream) {
  const source = presenceSourceFromStream(stream);
  if (!source) {
    delete presenceMetadata[sessionId];
    return;
  }
  presenceMetadata[sessionId] = source;
}

async function ensurePresenceMetadata(
  sessionId: string,
  stream: HelixStream,
) {
  rememberPresence(sessionId, stream);
  if (presenceMetadata[sessionId]) {
    debugRuntimeEvent("points-credit", "presence.metadata.ready", {
      channel: stream.user_login.toLowerCase(),
      session: sessionId,
      source: "launch",
    });
    syncViewerPresence(true);
    return;
  }
  debugRuntimeEvent("points-credit", "presence.metadata.lookup", {
    channel: stream.user_login.toLowerCase(),
    session: sessionId,
  });
  try {
    const page = await getChannelStreams(stream.user_login);
    const live = page.data[0];
    if (live) {
      rememberPresence(sessionId, live);
      debugRuntimeEvent("points-credit", "presence.metadata.ready", {
        channel: stream.user_login.toLowerCase(),
        session: sessionId,
        source: "helix",
      });
    } else {
      debugRuntimeEvent("points-credit", "presence.metadata.missing", {
        channel: stream.user_login.toLowerCase(),
        session: sessionId,
      });
    }
  } catch (reason) {
    debugRuntimeEvent("points-credit", "presence.metadata.failed", {
      channel: stream.user_login.toLowerCase(),
      session: sessionId,
      reason: reason instanceof Error ? reason.message : String(reason),
    });
    // Presence stays omitted until a later refresh can resolve IDs.
  }
  syncViewerPresence(true);
}

export function syncViewerPresence(force = false) {
  if (!isTauri()) return;
  const state = useWatchingStore.getState();
  const settings = useSettingsStore.getState().settings;
  const preferredSessionIds = settings.streaming.seamlessSwitch
    ? []
    : state.slotChannels
        .map(
          (channel) =>
            state.sessions.find(
              (session) => session.channel.toLowerCase() === channel,
            )?.id,
        )
        .filter((sessionId): sessionId is string => Boolean(sessionId));
  const enabled = settings.streaming.channelPoints;
  const targets = buildPresenceTargets(
    state.sessions,
    presenceMetadata,
    preferredSessionIds,
  );
  const key = JSON.stringify({ enabled, targets });
  if (!force && key === lastPresenceSyncKey) return;
  lastPresenceSyncKey = key;
  debugRuntimeEvent("points-credit", "presence.sync", {
    enabled,
    force,
    targetCount: targets.length,
    channels: targets.map((target) => target.channelLogin).join(","),
    sessions: targets.map((target) => target.sessionId).join(","),
  });
  void invoke("viewer_presence_sync", { enabled, targets })
    .then(() => {
      debugRuntimeEvent("points-credit", "presence.sync.result", {
        enabled,
        targetCount: targets.length,
        ok: true,
      });
    })
    .catch((reason) => {
      debugRuntimeEvent("points-credit", "presence.sync.result", {
        enabled,
        targetCount: targets.length,
        ok: false,
        reason: reason instanceof Error ? reason.message : String(reason),
      });
      if (lastPresenceSyncKey === key) {
        lastPresenceSyncKey = "";
      }
    });
}

function currentLayout(): MultistreamLayout {
  const raw = useSettingsStore.getState().settings.streaming.multistreamLayout;
  return isMultistreamLayout(raw) ? raw : DEFAULT_MULTISTREAM_LAYOUT;
}

function orderedChannels(): string[] {
  const state = useWatchingStore.getState();
  const settings = useSettingsStore.getState().settings;
  if (settings.streaming.seamlessSwitch) {
    return state.sessions
      .filter((s) => s.running)
      .map((s) => s.channel.toLowerCase())
      .filter(Boolean);
  }
  const running = new Set(
    state.sessions
      .filter((s) => s.running)
      .map((s) => s.channel.toLowerCase()),
  );
  return state.slotChannels.filter((c) => running.has(c));
}

function syncSlotsFromSessions(sessions: StreamSession[]) {
  const settings = useSettingsStore.getState().settings;
  if (settings.streaming.seamlessSwitch) {
    useWatchingStore.setState({ slotChannels: [] });
    return;
  }
  const running = sessions
    .filter((s) => s.running)
    .map((s) => s.channel.toLowerCase())
    .filter(Boolean);
  const prev = useWatchingStore.getState().slotChannels;
  const kept = prev.filter((c) => running.includes(c));
  const added = running.filter((c) => !kept.includes(c));
  useWatchingStore.setState({ slotChannels: [...kept, ...added] });
}

async function syncChatterino(channels: string[]) {
  if (!isTauri()) return;
  const settings = useSettingsStore.getState().settings;
  if (settings.chat.provider !== "chatterino") return;
  const key = chatterinoSyncKey(channels);
  if (!key) {
    const hasRunningSessions = useWatchingStore
      .getState()
      .sessions.some((s) => s.running);
    if (!chatterinoShouldCloseOnEmpty(chatSyncInflight, hasRunningSessions)) {
      debugRuntimeEvent("windows", "chatterino.close.skipped", {
        generation: chatSyncGeneration,
        running: hasRunningSessions,
        inflight: Boolean(chatSyncInflight),
      });
      return;
    }
    chatSyncGeneration += 1;
    lastChatSyncKey = "";
    chatSyncInflight = "";
    debugRuntimeEvent("windows", "chatterino.close.request", {
      generation: chatSyncGeneration,
    });
    void invoke("close_owned_chatterino")
      .then(() =>
        debugRuntimeEvent("windows", "chatterino.close.result", {
          generation: chatSyncGeneration,
          ok: true,
        }),
      )
      .catch((reason) =>
        debugRuntimeEvent("windows", "chatterino.close.result", {
          generation: chatSyncGeneration,
          ok: false,
          reason: reason instanceof Error ? reason.message : String(reason),
        }),
      );
    return;
  }
  // Duplicate Watching refresh + stream start used to fire two opens; the
  // second kill+spawn hits Chatterino's single-instance mutex and the new
  // process exits immediately.
  if (chatterinoShouldSkipSync(key, lastChatSyncKey, chatSyncInflight)) {
    debugRuntimeEvent("windows", "chatterino.open.skipped", {
      key,
      generation: chatSyncGeneration,
      last: lastChatSyncKey,
      inflight: chatSyncInflight,
    });
    return;
  }
  chatSyncInflight = key;
  const generation = chatSyncGeneration;
  debugRuntimeEvent("windows", "chatterino.open.request", {
    key,
    generation,
    channels: channels.join(","),
  });
  void invoke<string>("open_chatterino_chat", { channels })
    .then(() => {
      if (generation === chatSyncGeneration) {
        lastChatSyncKey = key;
      }
      debugRuntimeEvent("windows", "chatterino.open.result", {
        key,
        generation,
        currentGeneration: chatSyncGeneration,
        accepted: generation === chatSyncGeneration,
        ok: true,
      });
    })
    .catch((err: unknown) => {
      lastChatSyncKey = "";
      debugRuntimeEvent("windows", "chatterino.open.result", {
        key,
        generation,
        ok: false,
        reason: err instanceof Error ? err.message : String(err),
      });
      useWatchingStore.setState({
        error:
          err instanceof Error
            ? err.message
            : `Chatterino7 failed to open: ${String(err)}`,
      });
    })
    .finally(() => {
      if (chatSyncInflight === key) chatSyncInflight = "";
    });
}

function scheduleLayoutAfterReady() {
  if (!isTauri()) return;
  if (layoutTimer) clearTimeout(layoutTimer);
  layoutTimer = setTimeout(() => {
    layoutTimer = null;
    const settings = useSettingsStore.getState().settings;
    const reserveChat = settings.chat.provider === "chatterino";
    const channels = orderedChannels();
    if (!channels.length) return;
    const layout = currentLayout();
    debugRuntimeEvent("windows", "layout.request", {
      channels: channels.join(","),
      reserveChat,
      layout,
      linkedDock: settings.streaming.linkedDock,
      chatFraction: settings.streaming.chatWidthFraction,
      mainSide: settings.streaming.unevenMainSide,
    });
    void invoke("layout_watching", {
      channels,
      reserveChat,
      layout,
      linkedDock: settings.streaming.linkedDock,
      chatFraction: settings.streaming.chatWidthFraction,
      mainSide: settings.streaming.unevenMainSide,
    })
      .then(() =>
        debugRuntimeEvent("windows", "layout.result", {
          channels: channels.join(","),
          ok: true,
        }),
      )
      .catch((reason) =>
        debugRuntimeEvent("windows", "layout.result", {
          channels: channels.join(","),
          ok: false,
          reason: reason instanceof Error ? reason.message : String(reason),
        }),
      );
  }, 100);
}

function afterSessionsChanged() {
  const channels = orderedChannels();
  debugRuntimeEvent("windows", "sessions.changed", {
    channels: channels.join(","),
    count: channels.length,
  });
  void syncChatterino(channels);
  syncEventSub();
  syncViewerPresence();
  if (channels.length) {
    scheduleLayoutAfterReady();
  } else if (isTauri()) {
    debugRuntimeEvent("windows", "layout.request", {
      channels: "",
      reserveChat: false,
      reason: "sessions-empty",
    });
    // Tear down dock grips when the last stream ends (natural close or stop).
    void invoke("layout_watching", {
      channels: [],
      reserveChat: false,
    }).catch(() => undefined);
  }
}

/** Keep Rust EventSub subscriptions aligned with watching + settings. */
export function syncEventSub() {
  if (!isTauri()) return;
  const settings = useSettingsStore.getState().settings;
  const channels = orderedChannels();
  debugRuntimeEvent("raids", "eventsub.sync.frontend", {
    enabled: settings.streaming.followRaids,
    channels: channels.join(","),
  });
  void invoke("eventsub_sync", {
    enabled: settings.streaming.followRaids,
    channels,
  }).catch((reason) =>
    debugRuntimeEvent("raids", "eventsub.sync.frontend.result", {
      ok: false,
      reason: reason instanceof Error ? reason.message : String(reason),
    }),
  );
}

function stubHelixStream(opts: {
  login: string;
  userId: string;
  displayName?: string;
  title?: string;
  gameName?: string;
}): HelixStream {
  return {
    id: "",
    user_id: opts.userId,
    user_login: opts.login,
    user_name: opts.displayName ?? opts.login,
    game_id: "",
    game_name: opts.gameName ?? "",
    type: "live",
    title: opts.title ?? "",
    viewer_count: 0,
    started_at: new Date().toISOString(),
    language: "",
    thumbnail_url: "",
    is_mature: false,
  };
}

export async function bindStreamingListeners(): Promise<() => void> {
  if (!isTauri() || listenersBound) {
    return () => undefined;
  }
  listenersBound = true;
  const unStatus = await listen<StreamStatusEvent>("stream-status", (event) => {
    useWatchingStore.getState().applyStatus(event.payload);
  });
  const unChanged = await listen("stream-sessions-changed", () => {
    void useWatchingStore.getState().refresh().then(() => {
      afterSessionsChanged();
    });
  });
  const unFraction = await listen<number>("dock-chat-fraction", (event) => {
    const fraction = event.payload;
    if (typeof fraction !== "number" || Number.isNaN(fraction)) return;
    const settings = useSettingsStore.getState().settings;
    const clamped = Math.min(0.45, Math.max(0.12, fraction));
    if (Math.abs(settings.streaming.chatWidthFraction - clamped) < 0.001) return;
    useSettingsStore.getState().setSettings({
      streaming: {
        ...settings.streaming,
        chatWidthFraction: clamped,
      },
    });
  });
  syncEventSub();
  syncViewerPresence();
  return () => {
    listenersBound = false;
    unStatus();
    unChanged();
    unFraction();
  };
}

export const useWatchingStore = create<WatchingState>((set, get) => ({
  sessions: [],
  slotChannels: [],
  activeChatChannel: null,
  error: null,

  refresh: async () => {
    const sessions = await invoke<StreamSession[]>("stream_list");
    syncSlotsFromSessions(sessions);
    const hadSessions = get().sessions.length > 0;
    const previous = get().sessions;
    set({
      sessions: sessions.map((session) => {
        const prev = previous.find((item) => item.id === session.id);
        if (!prev) return session;
        return {
          ...session,
          ...nextSessionStatus(prev, {
            phase: session.phase ?? "info",
            ready: session.ready ?? false,
            status: session.status ?? "",
          }),
        };
      }),
    });
    presenceMetadata = prunePresenceMetadata(presenceMetadata, sessions);
    const active = get().activeChatChannel;
    if (active && !sessions.some((s) => s.channel === active)) {
      set({ activeChatChannel: sessions[0]?.channel ?? null });
    }
    // minimizeOnWatch hid the app while watching — bring it back once the
    // last stream ended (e.g. the user closed the player window).
    if (
      hadSessions &&
      sessions.length === 0 &&
      useSettingsStore.getState().settings.gui.minimizeOnWatch &&
      isTauri()
    ) {
      void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
        const win = getCurrentWindow();
        void win.unminimize().then(() => win.setFocus());
      });
    }
    syncViewerPresence();
  },

  applyStatus: (payload) => {
    const session = get().sessions.find((item) => item.id === payload.id);
    if (!session) return;
    const { next, changed, becameReady } = sessionStatusPatch(session, {
      phase: payload.phase,
      ready: payload.ready,
      status: payload.status,
    });
    if (!changed) return;
    debugRuntimeEvent("windows", "stream.status", {
      channel: payload.channel.toLowerCase(),
      session: payload.id,
      phase: payload.phase,
      ready: payload.ready,
      becameReady,
      status: payload.status,
    });
    set((state) => ({
      sessions: state.sessions.map((item) =>
        item.id === payload.id ? { ...item, ...next } : item,
      ),
    }));
    if (becameReady) {
      scheduleLayoutAfterReady();
    }
  },

  watchStream: async (stream) => {
    set({ error: null });
    const settings = useSettingsStore.getState().settings;
    const multi = !settings.streaming.seamlessSwitch;
    const channel = stream.user_login.toLowerCase();
    const running = get().sessions.filter((s) => s.running);
    const already = running.some((s) => s.channel.toLowerCase() === channel);

    if (multi && !already) {
      const cap = layoutCapacity(currentLayout());
      const slots = get().slotChannels.filter((c) =>
        running.some((s) => s.channel.toLowerCase() === c),
      );
      if (slots.length >= cap) {
        const msg = `Layout holds ${cap} streams. Stop one or pick a larger layout.`;
        set({ error: msg });
        throw new Error(msg);
      }
    }

    const replaceExisting =
      settings.streaming.seamlessSwitch && running.length > 0;
    const reserveChat = settings.chat.provider === "chatterino";

    const launch = resolveChannelLaunch(settings, stream.user_login, {
      title: stream.title,
      game: stream.game_name,
    });
    debugRuntimeEvent("windows", "watch.start", {
      channel,
      multi,
      already,
      replaceExisting,
      reserveChat,
      player: launch.playerId,
      quality: launch.quality,
      layout: currentLayout(),
    });

    try {
      if (multi && !already) {
        set((state) => ({
          slotChannels: state.slotChannels.includes(channel)
            ? state.slotChannels
            : [...state.slotChannels, channel],
        }));
      } else if (!multi) {
        set({ slotChannels: [channel] });
      }

      if (reserveChat) {
        const chatChannels = multi
          ? [
              ...new Set(
                [
                  ...get().slotChannels,
                  channel,
                ].filter(Boolean),
              ),
            ]
          : [channel];
        void syncChatterino(chatChannels);
      }

      // Planned dock position for the launch geometry, so mpv opens already
      // snapped to its tile instead of resizing visibly after "ready".
      const plannedChannels = replaceExisting
        ? [channel]
        : [...new Set([...orderedChannels(), channel])];

      const session = await invoke<StreamSession>("stream_start", {
        request: {
          channel: stream.user_login,
          quality: launch.quality,
          title: stream.title,
          game: stream.game_name,
          streamlinkSource: settings.streamlink.source,
          streamlinkCustomPath: settings.streamlink.customPath,
          playerId: launch.playerId,
          playerCustomPath: settings.player.customPath,
          playerCustomArgs: launch.playerCustomArgs,
          lowLatency: launch.lowLatency,
          disableAds: launch.disableAds,
          playerInput: settings.player.input,
          webbrowser: settings.streaming.webbrowser,
          webbrowserHeadless: settings.streaming.webbrowserHeadless,
          webbrowserExecutable: settings.streaming.webbrowserExecutable,
          retryStreams: 0,
          retryMax: 0,
          playerNoClose: settings.streaming.playerNoClose,
          reserveChat,
          replaceExisting,
          slotIndex: Math.max(0, plannedChannels.indexOf(channel)),
          slotCount: plannedChannels.length,
          layout: currentLayout(),
        },
      });
      debugRuntimeEvent("windows", "watch.started", {
        channel,
        session: session.id,
        running: session.running,
        ready: session.ready ?? false,
        phase: session.phase ?? "",
      });
      void ensurePresenceMetadata(session.id, stream);
      if (settings.gui.minimizeOnWatch && isTauri()) {
        void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
          void getCurrentWindow().minimize();
        });
      }
      set((state) => ({
        sessions: [
          ...state.sessions.filter((s) => s.id !== session.id),
          session,
        ],
        activeChatChannel:
          settings.chat.provider === "embedded"
            ? stream.user_login
            : state.activeChatChannel,
      }));
      syncViewerPresence();
      // Kick the debounced layout once the session is registered (orderedChannels
      // reads the store). The "ready" status event re-triggers it later; the
      // backend retries until every player window is actually tiled.
      scheduleLayoutAfterReady();
      void get().refresh();
      syncEventSub();
    } catch (err) {
      debugRuntimeEvent("windows", "watch.failed", {
        channel,
        reason: err instanceof Error ? err.message : String(err),
      });
      captureAppError(err, "stream_start");
      set({
        error: err instanceof Error ? err.message : String(err),
      });
      throw err;
    }
  },

  followRaid: async (raid) => {
    set({ error: null });
    const from = raid.fromChannel.toLowerCase();
    const toLogin = raid.toChannel.toLowerCase();
    const settings = useSettingsStore.getState().settings;
    const reserveChat = settings.chat.provider === "chatterino";
    debugRuntimeEvent("raids", "raid.follow", {
      from,
      to: toLogin,
      viewers: raid.viewers ?? 0,
      reserveChat,
    });

    const session = get().sessions.find(
      (s) => s.channel.toLowerCase() === from,
    );

    // Resolve live Helix data when possible; fall back to a stub so we still jump.
    let target: HelixStream | null = null;
    try {
      const page = await getChannelStreams(toLogin);
      target = page.data[0] ?? null;
      if (!target) {
        const users = await getUsersByLogin([toLogin]);
        const user = users.data[0];
        target = stubHelixStream({
          login: toLogin,
          userId: user?.id ?? raid.toUserId,
          displayName: user?.display_name,
        });
      }
    } catch {
      target = stubHelixStream({
        login: toLogin,
        userId: raid.toUserId,
      });
    }

    // Replace slot in-place before stop so layout/chat see the new set.
    const slots = [...get().slotChannels];
    const idx = slots.indexOf(from);
    if (idx >= 0) {
      slots[idx] = toLogin;
    } else {
      slots.push(toLogin);
    }
    const nextSlots = [...new Set(slots.filter(Boolean))];
    set({
      slotChannels: nextSlots,
      activeChatChannel:
        settings.chat.provider === "embedded" &&
        (get().activeChatChannel?.toLowerCase() === from ||
          !get().activeChatChannel)
          ? toLogin
          : get().activeChatChannel,
    });

    if (session?.running) {
      debugRuntimeEvent("raids", "raid.stop_old", {
        channel: from,
        session: session.id,
      });
      await invoke("stream_stop", { id: session.id });
      await get().refresh();
    }

    const launch = resolveChannelLaunch(settings, toLogin, {
      title: target.title,
      game: target.game_name,
    });
    const plannedChannels = [...new Set([...orderedChannels(), toLogin])];

    if (reserveChat) {
      void syncChatterino(plannedChannels);
    }

    try {
      debugRuntimeEvent("raids", "raid.start_new", {
        channel: toLogin,
        from,
        slotCount: plannedChannels.length,
      });
      const started = await invoke<StreamSession>("stream_start", {
        request: {
          channel: toLogin,
          quality: launch.quality,
          title: target.title,
          game: target.game_name,
          streamlinkSource: settings.streamlink.source,
          streamlinkCustomPath: settings.streamlink.customPath,
          playerId: launch.playerId,
          playerCustomPath: settings.player.customPath,
          playerCustomArgs: launch.playerCustomArgs,
          lowLatency: launch.lowLatency,
          disableAds: launch.disableAds,
          playerInput: settings.player.input,
          webbrowser: settings.streaming.webbrowser,
          webbrowserHeadless: settings.streaming.webbrowserHeadless,
          webbrowserExecutable: settings.streaming.webbrowserExecutable,
          retryStreams: 0,
          retryMax: 0,
          playerNoClose: settings.streaming.playerNoClose,
          reserveChat,
          replaceExisting: false,
          slotIndex: Math.max(0, plannedChannels.indexOf(toLogin)),
          slotCount: plannedChannels.length,
          layout: currentLayout(),
        },
      });
      debugRuntimeEvent("raids", "raid.started", {
        channel: toLogin,
        session: started.id,
        ready: started.ready ?? false,
      });
      void ensurePresenceMetadata(started.id, target);
      set((state) => ({
        sessions: [
          ...state.sessions.filter((s) => s.id !== started.id && s.id !== session?.id),
          started,
        ],
      }));
      scheduleLayoutAfterReady();
      void get().refresh();
      syncEventSub();
    } catch (err) {
      debugRuntimeEvent("raids", "raid.follow.failed", {
        from,
        to: toLogin,
        reason: err instanceof Error ? err.message : String(err),
      });
      captureAppError(err, "follow_raid");
      set({
        error: err instanceof Error ? err.message : String(err),
      });
      throw err;
    }
  },

  stopSession: async (id) => {
    const session = get().sessions.find((s) => s.id === id);
    delete presenceMetadata[id];
    const channel = session?.channel.toLowerCase();
    debugRuntimeEvent("windows", "stream.stop.request", {
      channel: channel ?? "",
      session: id,
    });
    try {
      await invoke("stream_stop", { id });
      debugRuntimeEvent("windows", "stream.stop.result", {
        channel: channel ?? "",
        session: id,
        ok: true,
      });
    } catch (reason) {
      debugRuntimeEvent("windows", "stream.stop.result", {
        channel: channel ?? "",
        session: id,
        ok: false,
        reason: reason instanceof Error ? reason.message : String(reason),
      });
      throw reason;
    }
    if (channel) {
      set((state) => ({
        slotChannels: state.slotChannels.filter((c) => c !== channel),
        activeChatChannel:
          state.activeChatChannel?.toLowerCase() === channel
            ? null
            : state.activeChatChannel,
      }));
    }
    lastChatSyncKey = "";
    chatSyncInflight = "";
    chatSyncGeneration += 1;
    await get().refresh();
    afterSessionsChanged();
  },

  stopAll: async () => {
    debugRuntimeEvent("windows", "stream.stop_all.request");
    await invoke("stream_stop_all");
    debugRuntimeEvent("windows", "stream.stop_all.result", { ok: true });
    lastChatSyncKey = "";
    chatSyncInflight = "";
    chatSyncGeneration += 1;
    void invoke("close_owned_chatterino").catch(() => undefined);
    presenceMetadata = {};
    set({ sessions: [], slotChannels: [], activeChatChannel: null });
    syncEventSub();
    syncViewerPresence();
  },

  toggleMute: async (id) => {
    if (!isTauri()) return;
    try {
      const muted = await invoke<boolean>("stream_toggle_mute", { id });
      set((state) => ({
        sessions: state.sessions.map((s) =>
          s.id === id ? { ...s, muted } : s,
        ),
      }));
    } catch (err) {
      useWatchingStore.setState({
        error:
          err instanceof Error ? err.message : `Mute failed: ${String(err)}`,
      });
    }
  },

  moveSlot: (channel, direction) => {
    const login = channel.toLowerCase();
    const slots = [...get().slotChannels];
    const i = slots.indexOf(login);
    if (i < 0) return;
    const j = i + direction;
    if (j < 0 || j >= slots.length) return;
    const tmp = slots[i]!;
    slots[i] = slots[j]!;
    slots[j] = tmp;
    set({ slotChannels: slots });
    scheduleLayoutAfterReady();
    void syncChatterino(orderedChannels());
  },

  reorderSlots: (channels) => {
    const current = new Set(get().slotChannels);
    const next = channels
      .map((c) => c.toLowerCase())
      .filter((c) => current.has(c));
    if (next.length !== current.size) return;
    set({ slotChannels: next });
    scheduleLayoutAfterReady();
    void syncChatterino(orderedChannels());
  },

  applyLayout: () => {
    scheduleLayoutAfterReady();
    void syncChatterino(orderedChannels());
  },

  setActiveChat: (channel) => set({ activeChatChannel: channel }),
}));