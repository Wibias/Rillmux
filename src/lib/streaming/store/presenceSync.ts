import { invoke, isTauri } from "../../tauri";
import type { HelixStream } from "../../twitch/helix";
import { getChannelStreams } from "../../twitch/helix";
import { useSettingsStore } from "../../settings/store";
import { debugRuntimeEvent } from "../../diagnostics/runtimeDebug";
import { buildPresenceTargets, presenceSourceFromStream } from "../presence";
import { useWatchingStore } from "../store";
import { presenceState } from "./state";

export function rememberPresence(sessionId: string, stream: HelixStream) {
  const source = presenceSourceFromStream(stream);
  if (!source) {
    delete presenceState.metadata[sessionId];
    return;
  }
  presenceState.metadata[sessionId] = source;
}

export async function ensurePresenceMetadata(
  sessionId: string,
  stream: HelixStream,
) {
  rememberPresence(sessionId, stream);
  if (presenceState.metadata[sessionId]) {
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
    presenceState.metadata,
    preferredSessionIds,
  );
  const key = JSON.stringify({ enabled, targets });
  if (!force && key === presenceState.lastSyncKey) return;
  presenceState.lastSyncKey = key;
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
      if (presenceState.lastSyncKey === key) {
        presenceState.lastSyncKey = "";
      }
    });
}

