import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { invoke, isTauri } from "../tauri";
import type { HelixStream } from "../twitch/helix";
import { getChannelStreams, getUsersByLogin } from "../twitch/helix";
import { useSettingsStore } from "../settings/store";
import { resolveChannelLaunch } from "../settings/types";
import { captureAppError } from "../sentryCapture";
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
  /** Ordered Multistream slots (lowercase logins); empty in other modes. */
  slotChannels: string[];
  activeChatChannel: string | null;
  error: string | null;
  refresh: () => Promise<void>;
  watchStream: (stream: HelixStream) => Promise<void>;
  /** Replace the raiding session without killing unrelated sessions. */
  followRaid: (raid: RaidOutgoingEvent) => Promise<void>;
  stopSession: (id: string) => Promise<void>;
  stopAll: () => Promise<void>;
  toggleMute: (id: string) => Promise<void>;
  moveSlot: (channel: string, direction: -1 | 1) => void;
  /** Drag & drop reorder: replace the slot order outright (same channels). */
  reorderSlots: (channels: string[]) => void;
  /** Retile + resync chat after mode/layout changes. */
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
  const preferredSessionIds =
    settings.streaming.streamOpenMode === "multistream"
      ? state.slotChannels
          .map(
            (channel) =>
              state.sessions.find(
                (session) => session.channel.toLowerCase() === channel,
              )?.id,
          )
          .filter((sessionId): sessionId is string => Boolean(sessionId))
      : [];
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

function runningChannels(): string[] {
  return useWatchingStore.getState().sessions.flatMap((session) => {
    if (!session.running) return [];
    const channel = session.channel.toLowerCase();
    return channel ? [channel] : [];
  });
}

/** Only channels owned by the coordinated native layout. */
function layoutChannels(): string[] {
  const state = useWatchingStore.getState();
  const mode = useSettingsStore.getState().settings.streaming.streamOpenMode;
  if (mode === "independent") return [];
  if (mode === "seamless") return runningChannels();

  const running = new Set(runningChannels());
  return state.slotChannels.filter((channel) => running.has(channel));
}

function chatterinoChannels(): string[] {
  const mode = useSettingsStore.getState().settings.streaming.streamOpenMode;
  return mode === "multistream" ? layoutChannels() : runningChannels();
}

function syncSlotsFromSessions(sessions: StreamSession[]) {
  const mode = useSettingsStore.getState().settings.streaming.streamOpenMode;
  if (mode !== "multistream") {
    useWatchingStore.setState({ slotChannels: [] });
    return;
  }
  const running = [
    ...new Set(
      sessions.flatMap((session) => {
        if (!session.running) return [];
        const channel = session.channel.toLowerCase();
        return channel ? [channel] : [];
      }),
    ),
  ];
  const prev = useWatchingStore.getState().slotChannels;
  const runningSet = new Set(running);
  const kept = prev.filter((channel) => runningSet.has(channel));
  const keptSet = new Set(kept);
  const added = running.filter((channel) => !keptSet.has(channel));
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
      .sessions.some((session) => session.running);
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
    const channels = layoutChannels();
    if (!channels.length) {
      debugRuntimeEvent("windows", "layout.request", {
        channels: "",
        reserveChat: false,
        reason: "layout-inactive",
      });
      void invoke("layout_watching", {
        channels: [],
        reserveChat: false,
      }).catch(() => undefined);
      return;
    }
    const mode = settings.streaming.streamOpenMode;
    const reserveChat =
      settings.chat.provider === "chatterino" && mode !== "independent";
    const linkedDock =
      mode === "multistream" && settings.streaming.linkedDock;
    const layout = currentLayout();
    debugRuntimeEvent("windows", "layout.request", {
      channels: channels.join(","),
      reserveChat,
      layout,
      linkedDock,
      chatFraction: settings.streaming.chatWidthFraction,
      mainSide: settings.streaming.unevenMainSide,
    });
    void invoke("layout_watching", {
      channels,
      reserveChat,
      layout,
      linkedDock,
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
  const channels = runningChannels();
  debugRuntimeEvent("windows", "sessions.changed", {
    channels: channels.join(","),
    count: channels.length,
    layoutChannels: layoutChannels().join(","),
  });
  void syncChatterino(chatterinoChannels());
  syncEventSub();
  syncViewerPresence();
  scheduleLayoutAfterReady();
}

/** Keep Rust EventSub subscriptions aligned with every running stream. */
export function syncEventSub() {
  if (!isTauri()) return;
  const settings = useSettingsStore.getState().settings;
  const channels = runningChannels();
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
  const [unStatus, unChanged, unFraction] = await Promise.all([
    listen<StreamStatusEvent>("stream-status", (event) => {
      useWatchingStore.getState().applyStatus(event.payload);
    }),
    listen("stream-sessions-changed", () => {
      void useWatchingStore.getState().refresh().then(() => {
        afterSessionsChanged();
      });
    }),
    listen<number>("dock-chat-fraction", (event) => {
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
    }),
  ]);
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
    if (active && !sessions.some((session) => session.channel === active)) {
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
      syncViewerPresence(true);
    }
  },

  watchStream: async (stream) => {
    set({ error: null });
    const settings = useSettingsStore.getState().settings;
    const mode = settings.streaming.streamOpenMode;
    const multi = mode === "multistream";
    const seamless = mode === "seamless";
    const channel = stream.user_login.toLowerCase();
    const running = get().sessions.filter((session) => session.running);
    const already = running.some(
      (session) => session.channel.toLowerCase() === channel,
    );

    if (multi && !already) {
      const cap = layoutCapacity(currentLayout());
      const slots = get().slotChannels.filter((slot) =>
        running.some((session) => session.channel.toLowerCase() === slot),
      );
      if (slots.length >= cap) {
        const msg = `Layout holds ${cap} streams. Stop one or pick a larger layout.`;
        set({ error: msg });
        throw new Error(msg);
      }
    }

    const replaceExisting = seamless && running.length > 0;
    const hasChatterino = settings.chat.provider === "chatterino";
    const reserveChat = hasChatterino && mode !== "independent";

    const launch = resolveChannelLaunch(
      settings,
      stream.user_login,
      {
        title: stream.title,
        game: stream.game_name,
      },
      { sideBySideChat: reserveChat },
    );
    debugRuntimeEvent("windows", "watch.start", {
      channel,
      mode,
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
        set({ slotChannels: [] });
      }

      if (hasChatterino) {
        const existingChannels = running.map((session) =>
          session.channel.toLowerCase(),
        );
        const chatChannels = replaceExisting
          ? [channel]
          : [...new Set([...existingChannels, channel])];
        void syncChatterino(chatChannels);
      }

      // Independent/Seamless launches are standalone. Only Multistream gives
      // Streamlink a coordinated slot geometry before the later native retile.
      const plannedChannels = multi
        ? [...new Set([...get().slotChannels, channel])]
        : [channel];

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
          ...state.sessions.filter((item) => item.id !== session.id),
          session,
        ],
        activeChatChannel:
          settings.chat.provider === "embedded"
            ? stream.user_login
            : state.activeChatChannel,
      }));
      syncViewerPresence();
      // Ready status re-triggers this later. In Independent mode this only
      // clears stale native dock state and never tiles the standalone players.
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
    const mode = settings.streaming.streamOpenMode;
    const multi = mode === "multistream";
    const hasChatterino = settings.chat.provider === "chatterino";
    const reserveChat = hasChatterino && mode !== "independent";
    const previousSlotIndex = get().slotChannels.indexOf(from);
    debugRuntimeEvent("raids", "raid.follow", {
      from,
      to: toLogin,
      viewers: raid.viewers ?? 0,
      mode,
      reserveChat,
    });

    const session = get().sessions.find(
      (item) => item.channel.toLowerCase() === from,
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

    if (session?.running) {
      debugRuntimeEvent("raids", "raid.stop_old", {
        channel: from,
        session: session.id,
      });
      await invoke("stream_stop", { id: session.id });
      await get().refresh();
    }

    if (multi) {
      const nextSlots = get().slotChannels.filter(
        (slot) => slot !== from && slot !== toLogin,
      );
      const index =
        previousSlotIndex >= 0
          ? Math.min(previousSlotIndex, nextSlots.length)
          : nextSlots.length;
      nextSlots.splice(index, 0, toLogin);
      set({ slotChannels: nextSlots });
    } else {
      set({ slotChannels: [] });
    }
    set({
      activeChatChannel:
        settings.chat.provider === "embedded" &&
        (get().activeChatChannel?.toLowerCase() === from ||
          !get().activeChatChannel)
          ? toLogin
          : get().activeChatChannel,
    });

    const launch = resolveChannelLaunch(
      settings,
      toLogin,
      {
        title: target.title,
        game: target.game_name,
      },
      { sideBySideChat: reserveChat },
    );
    const plannedChannels = multi
      ? [...new Set([...get().slotChannels, toLogin])]
      : [toLogin];

    if (hasChatterino) {
      void syncChatterino([...new Set([...runningChannels(), toLogin])]);
    }

    try {
      debugRuntimeEvent("raids", "raid.start_new", {
        channel: toLogin,
        from,
        mode,
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
          ...state.sessions.filter(
            (item) => item.id !== started.id && item.id !== session?.id,
          ),
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
    const session = get().sessions.find((item) => item.id === id);
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
        slotChannels: state.slotChannels.filter((slot) => slot !== channel),
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
    scheduleLayoutAfterReady();
  },

  toggleMute: async (id) => {
    if (!isTauri()) return;
    try {
      const muted = await invoke<boolean>("stream_toggle_mute", { id });
      set((state) => ({
        sessions: state.sessions.map((session) =>
          session.id === id ? { ...session, muted } : session,
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
    void syncChatterino(chatterinoChannels());
  },

  reorderSlots: (channels) => {
    const current = new Set(get().slotChannels);
    const next = channels.flatMap((channel) => {
      const login = channel.toLowerCase();
      return current.has(login) ? [login] : [];
    });
    if (next.length !== current.size) return;
    set({ slotChannels: next });
    scheduleLayoutAfterReady();
    void syncChatterino(chatterinoChannels());
  },

  applyLayout: () => {
    scheduleLayoutAfterReady();
    void syncChatterino(chatterinoChannels());
  },

  setActiveChat: (channel) => set({ activeChatChannel: channel }),
}));
