import { create } from "zustand";
import { invoke, isTauri } from "../tauri";
import type { HelixStream } from "../twitch/helix";
import { getChannelStreams, getUsersByLogin } from "../twitch/helix";
import { useSettingsStore } from "../settings/store";
import { resolveChannelLaunch } from "../settings/types";
import { captureAppError } from "../sentryCapture";
import { debugRuntimeEvent } from "../diagnostics/runtimeDebug";
import { layoutCapacity } from "./layout";
import { nextSessionStatus, sessionStatusPatch } from "./status";
import { prunePresenceMetadata } from "./presence";
import type { StreamSession, WatchingState } from "./store/types";
import { ensurePresenceMetadata, syncViewerPresence } from "./store/presenceSync";
import { currentLayout, runningChannels, chatterinoChannels, syncIndependentChatterino } from "./store/layoutChannels";
import { syncSlotsFromSessions, syncChatterino } from "./store/chatterinoStoreSync";
import { scheduleLayoutAfterReady, afterSessionsChanged } from "./store/layoutSchedule";
import { syncEventSub, stubHelixStream } from "./store/listeners";
import { chatSyncState, presenceState } from "./store/state";


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
    presenceState.metadata = prunePresenceMetadata(presenceState.metadata, sessions);
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
    const besideChat = hasChatterino;

    const launch = resolveChannelLaunch(
      settings,
      stream.user_login,
      {
        title: stream.title,
        game: stream.game_name,
      },
      { sideBySideChat: besideChat },
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

      if (hasChatterino && mode === "independent") {
        void syncIndependentChatterino([
          ...running.map((session) => session.channel.toLowerCase()),
          channel,
        ]);
      } else if (hasChatterino) {
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
          autoCatchUp:
            settings.player.mpv.autoCatchUp && launch.playerId === "mpv",
          volumeMode: settings.player.mpv.volumeMode,
          volumeCustom: settings.player.mpv.volumeCustom,
          volumeMax: settings.player.mpv.volumeBoost,
          reserveChat: besideChat,
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
      { sideBySideChat: hasChatterino },
    );
    const plannedChannels = multi
      ? [...new Set([...get().slotChannels, toLogin])]
      : [toLogin];

    if (hasChatterino && mode === "independent") {
      void syncIndependentChatterino([...runningChannels(), toLogin]);
    } else if (hasChatterino) {
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
          autoCatchUp:
            settings.player.mpv.autoCatchUp && launch.playerId === "mpv",
          volumeMode: settings.player.mpv.volumeMode,
          volumeCustom: settings.player.mpv.volumeCustom,
          volumeMax: settings.player.mpv.volumeBoost,
          reserveChat: hasChatterino,
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
    delete presenceState.metadata[id];
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
    chatSyncState.lastKey = "";
    chatSyncState.inflight = "";
    chatSyncState.generation += 1;
    await get().refresh();
    afterSessionsChanged();
  },

  stopAll: async () => {
    debugRuntimeEvent("windows", "stream.stop_all.request");
    await invoke("stream_stop_all");
    debugRuntimeEvent("windows", "stream.stop_all.result", { ok: true });
    chatSyncState.lastKey = "";
    chatSyncState.inflight = "";
    chatSyncState.generation += 1;
    void invoke("close_owned_chatterino").catch(() => undefined);
    presenceState.metadata = {};
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

export type { StreamSession, StreamStatusEvent } from "./store/types";
export { syncViewerPresence } from "./store/presenceSync";
export { syncEventSub, bindStreamingListeners } from "./store/listeners";

