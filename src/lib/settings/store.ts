import { create } from "zustand";
import {
  type AppSettings,
  defaultDebugCategories,
  defaultHotkeys,
  defaultSettings,
  isStreamOpenMode,
  SETTINGS_SCHEMA_VERSION,
} from "./types";
import {
  DEFAULT_MULTISTREAM_LAYOUT,
  DEFAULT_UNEVEN_MAIN_SIDE,
  isMultistreamLayout,
  isUnevenMainSide,
} from "../streaming/layout";
import {
  isFollowedPageSize,
  isFollowedSort,
  isFollowedView,
} from "../browse/followedList";

interface SettingsState {
  settings: AppSettings;
  hydrated: boolean;
  setSettings: (patch: Partial<AppSettings>) => void;
  replaceSettings: (next: AppSettings) => void;
  hydrate: (next: AppSettings) => void;
  setChannelOverride: (
    login: string,
    patch: Partial<AppSettings["channels"][string]> | null,
  ) => void;
}

function parseChannelPointsHudOffset(
  raw: unknown,
): { x: number; y: number } | null {
  if (!raw || typeof raw !== "object") return null;
  const value = raw as { x?: unknown; y?: unknown };
  if (typeof value.x !== "number" || typeof value.y !== "number") return null;
  if (!Number.isFinite(value.x) || !Number.isFinite(value.y)) return null;
  return {
    x: Math.min(1, Math.max(0, value.x)),
    y: Math.min(1, Math.max(0, value.y)),
  };
}

/** Migrate older settings blobs toward the current schema. */
export function migrateSettings(raw: unknown): AppSettings {
  const base = defaultSettings();
  if (!raw || typeof raw !== "object") {
    return base;
  }
  const input = raw as Omit<Partial<AppSettings>, "streaming"> & {
    schemaVersion?: number;
    quality?: string;
    closeToTray?: boolean;
    streaming?: Partial<AppSettings["streaming"]> & {
      /** v20 and older. */
      seamlessSwitch?: boolean;
    };
  };
  const prevSchema = input.schemaVersion ?? 0;

  const merged: AppSettings = {
    ...base,
    ...input,
    streamlink: { ...base.streamlink, ...input.streamlink },
    player: {
      ...base.player,
      ...input.player,
      input: input.player?.input ?? base.player.input,
      mpv: { ...base.player.mpv, ...input.player?.mpv },
    },
    chat: { ...base.chat, ...input.chat },
    streaming: {
      ...base.streaming,
      ...input.streaming,
      quality: input.streaming?.quality ?? input.quality ?? base.streaming.quality,
      disableAds: input.streaming?.disableAds ?? base.streaming.disableAds,
      streamOpenMode: (() => {
        const mode = input.streaming?.streamOpenMode;
        if (isStreamOpenMode(mode)) return mode;
        if (input.streaming?.seamlessSwitch === false) return "multistream";
        if (input.streaming?.seamlessSwitch === true) return "seamless";
        return base.streaming.streamOpenMode;
      })(),
      multistreamLayout: (() => {
        const raw = input.streaming?.multistreamLayout;
        return raw && isMultistreamLayout(raw)
          ? raw
          : DEFAULT_MULTISTREAM_LAYOUT;
      })(),
      unevenMainSide: (() => {
        const raw = input.streaming?.unevenMainSide;
        return raw && isUnevenMainSide(raw) ? raw : DEFAULT_UNEVEN_MAIN_SIDE;
      })(),
      linkedDock: input.streaming?.linkedDock ?? base.streaming.linkedDock,
      followRaids: input.streaming?.followRaids ?? base.streaming.followRaids,
      channelPointsPolls:
        input.streaming?.channelPointsPolls ?? base.streaming.channelPointsPolls,
      channelPointsHud:
        input.streaming?.channelPointsHud ?? base.streaming.channelPointsHud,
      channelPointsHudOffset: parseChannelPointsHudOffset(
        input.streaming?.channelPointsHudOffset,
      ),
      streamLanguages: (() => {
        const raw = input.streaming?.streamLanguages;
        if (!Array.isArray(raw)) return base.streaming.streamLanguages;
        return [
          ...new Set(
            raw.flatMap((c) => {
              if (typeof c !== "string") return [];
              const n = c.trim().toLowerCase();
              return n === "other" || /^[a-z]{2}$/.test(n) ? [n] : [];
            }),
          ),
        ].slice(0, 100);
      })(),
      chatWidthFraction: (() => {
        const f = input.streaming?.chatWidthFraction;
        if (typeof f !== "number" || Number.isNaN(f)) {
          return base.streaming.chatWidthFraction;
        }
        return Math.min(0.45, Math.max(0.12, f));
      })(),
    },
    gui: {
      ...base.gui,
      ...input.gui,
      closeToTray:
        input.gui?.closeToTray ?? input.closeToTray ?? base.gui.closeToTray,
      onboardingDone: input.gui?.onboardingDone ?? base.gui.onboardingDone,
      followedView: isFollowedView(input.gui?.followedView)
        ? input.gui.followedView
        : base.gui.followedView,
      followedSort: isFollowedSort(input.gui?.followedSort)
        ? input.gui.followedSort
        : base.gui.followedSort,
      followedPageSize: isFollowedPageSize(input.gui?.followedPageSize)
        ? input.gui.followedPageSize
        : base.gui.followedPageSize,
      hideMatureFollowed: Boolean(
        input.gui?.hideMatureFollowed ?? base.gui.hideMatureFollowed,
      ),
      debugMode: Boolean(input.gui?.debugMode ?? base.gui.debugMode),
      debugCategories: (() => {
        const defaults = defaultDebugCategories();
        const raw = input.gui?.debugCategories;
        return {
          windows:
            typeof raw?.windows === "boolean" ? raw.windows : defaults.windows,
          pointsCredit:
            typeof raw?.pointsCredit === "boolean"
              ? raw.pointsCredit
              : defaults.pointsCredit,
          pointsClaim:
            typeof raw?.pointsClaim === "boolean"
              ? raw.pointsClaim
              : defaults.pointsClaim,
          rewards:
            typeof raw?.rewards === "boolean" ? raw.rewards : defaults.rewards,
          polls: typeof raw?.polls === "boolean" ? raw.polls : defaults.polls,
          raids: typeof raw?.raids === "boolean" ? raw.raids : defaults.raids,
        };
      })(),
      pinnedFollowed: (() => {
        const raw = input.gui?.pinnedFollowed;
        if (!Array.isArray(raw)) return base.gui.pinnedFollowed;
        return [
          ...new Set(
            raw.flatMap((c) => {
              if (typeof c !== "string") return [];
              const n = c.trim().toLowerCase();
              return n ? [n] : [];
            }),
          ),
        ].slice(0, 50);
      })(),
    },
    notifications: {
      ...base.notifications,
      ...input.notifications,
      mutedFollowed: (() => {
        const raw = input.notifications?.mutedFollowed;
        if (!Array.isArray(raw)) return base.notifications.mutedFollowed;
        return [
          ...new Set(
            raw.flatMap((c) => {
              if (typeof c !== "string") return [];
              const n = c.trim().toLowerCase();
              return n ? [n] : [];
            }),
          ),
        ];
      })(),
    },
    hotkeys: { ...defaultHotkeys(), ...input.hotkeys },
    channels: { ...base.channels, ...input.channels },
    schemaVersion: SETTINGS_SCHEMA_VERSION,
  };

  // Only an actual Multistream session may use the linked dock.
  if (merged.streaming.streamOpenMode !== "multistream") {
    merged.streaming.linkedDock = false;
  }
  // Older browse/multistream views still consume this derived compatibility
  // bit. Launch/session behavior uses streamOpenMode exclusively.
  merged.streaming.seamlessSwitch =
    merged.streaming.streamOpenMode !== "multistream";

  // v8: webbrowser default flipped off — it made first stream starts very slow.
  if (prevSchema < 8) {
    merged.streaming.webbrowser = false;
  }

  delete (merged as { quality?: string }).quality;
  delete (merged as { closeToTray?: boolean }).closeToTray;
  return merged;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  settings: defaultSettings(),
  hydrated: false,
  setSettings: (patch) =>
    set((state) => ({
      settings: migrateSettings({
        ...state.settings,
        ...patch,
        streamlink: { ...state.settings.streamlink, ...patch.streamlink },
        player: {
          ...state.settings.player,
          ...patch.player,
          mpv: {
            ...state.settings.player.mpv,
            ...patch.player?.mpv,
          },
        },
        chat: { ...state.settings.chat, ...patch.chat },
        streaming: { ...state.settings.streaming, ...patch.streaming },
        gui: { ...state.settings.gui, ...patch.gui },
        notifications: {
          ...state.settings.notifications,
          ...patch.notifications,
        },
        hotkeys: { ...state.settings.hotkeys, ...patch.hotkeys },
        channels: patch.channels
          ? { ...state.settings.channels, ...patch.channels }
          : state.settings.channels,
      }),
    })),
  replaceSettings: (next) => set({ settings: migrateSettings(next) }),
  hydrate: (next) => set({ settings: migrateSettings(next), hydrated: true }),
  setChannelOverride: (login, patch) =>
    set((state) => {
      const key = login.trim().toLowerCase();
      if (!key) return state;
      const channels = { ...state.settings.channels };
      if (patch === null) {
        delete channels[key];
      } else {
        channels[key] = { ...channels[key], ...patch };
      }
      return {
        settings: migrateSettings({ ...state.settings, channels }),
      };
    }),
}));
