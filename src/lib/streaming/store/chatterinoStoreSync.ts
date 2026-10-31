import { invoke, isTauri } from "../../tauri";
import { useSettingsStore } from "../../settings/store";
import { debugRuntimeEvent } from "../../diagnostics/runtimeDebug";
import { chatterinoShouldCloseOnEmpty, chatterinoShouldSkipSync, chatterinoSyncKey } from "../chatterinoSync";
import { useWatchingStore } from "../store";
import type { StreamSession } from "./types";
import { chatSyncState } from "./state";

export function syncSlotsFromSessions(sessions: StreamSession[]) {
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

export async function syncChatterino(channels: string[]) {
  if (!isTauri()) return;
  const settings = useSettingsStore.getState().settings;
  if (settings.chat.provider !== "chatterino") return;
  const key = chatterinoSyncKey(channels);
  if (!key) {
    const hasRunningSessions = useWatchingStore
      .getState()
      .sessions.some((session) => session.running);
    if (!chatterinoShouldCloseOnEmpty(chatSyncState.inflight, hasRunningSessions)) {
      debugRuntimeEvent("windows", "chatterino.close.skipped", {
        generation: chatSyncState.generation,
        running: hasRunningSessions,
        inflight: Boolean(chatSyncState.inflight),
      });
      return;
    }
    chatSyncState.generation += 1;
    chatSyncState.lastKey = "";
    chatSyncState.inflight = "";
    debugRuntimeEvent("windows", "chatterino.close.request", {
      generation: chatSyncState.generation,
    });
    void invoke("close_owned_chatterino")
      .then(() =>
        debugRuntimeEvent("windows", "chatterino.close.result", {
          generation: chatSyncState.generation,
          ok: true,
        }),
      )
      .catch((reason) =>
        debugRuntimeEvent("windows", "chatterino.close.result", {
          generation: chatSyncState.generation,
          ok: false,
          reason: reason instanceof Error ? reason.message : String(reason),
        }),
      );
    return;
  }
  // Duplicate Watching refresh + stream start used to fire two opens; the
  // second kill+spawn hits Chatterino's single-instance mutex and the new
  // process exits immediately.
  if (chatterinoShouldSkipSync(key, chatSyncState.lastKey, chatSyncState.inflight)) {
    debugRuntimeEvent("windows", "chatterino.open.skipped", {
      key,
      generation: chatSyncState.generation,
      last: chatSyncState.lastKey,
      inflight: chatSyncState.inflight,
    });
    return;
  }
  chatSyncState.inflight = key;
  const generation = chatSyncState.generation;
  debugRuntimeEvent("windows", "chatterino.open.request", {
    key,
    generation,
    channels: channels.join(","),
  });
  void invoke<string>("open_chatterino_chat", { channels })
    .then(() => {
      if (generation === chatSyncState.generation) {
        chatSyncState.lastKey = key;
      }
      debugRuntimeEvent("windows", "chatterino.open.result", {
        key,
        generation,
        currentGeneration: chatSyncState.generation,
        accepted: generation === chatSyncState.generation,
        ok: true,
      });
    })
    .catch((err: unknown) => {
      chatSyncState.lastKey = "";
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
      if (chatSyncState.inflight === key) chatSyncState.inflight = "";
    });
}

