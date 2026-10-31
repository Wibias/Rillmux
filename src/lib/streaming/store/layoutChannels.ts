import { invoke, isTauri } from "../../tauri";
import { useSettingsStore } from "../../settings/store";
import { DEFAULT_MULTISTREAM_LAYOUT, isMultistreamLayout, type MultistreamLayout } from "../layout";
import { useWatchingStore } from "../store";
import { independentChatChannels } from "./state";

export function currentLayout(): MultistreamLayout {
  const raw = useSettingsStore.getState().settings.streaming.multistreamLayout;
  return isMultistreamLayout(raw) ? raw : DEFAULT_MULTISTREAM_LAYOUT;
}

export function runningChannels(): string[] {
  return useWatchingStore.getState().sessions.flatMap((session) => {
    if (!session.running) return [];
    const channel = session.channel.toLowerCase();
    return channel ? [channel] : [];
  });
}

/** Only channels owned by the coordinated native layout. */
export function layoutChannels(): string[] {
  const state = useWatchingStore.getState();
  const mode = useSettingsStore.getState().settings.streaming.streamOpenMode;
  if (mode === "independent") return [];
  if (mode === "seamless") return runningChannels();

  const running = new Set(runningChannels());
  return state.slotChannels.filter((channel) => running.has(channel));
}

export function chatterinoChannels(): string[] {
  const mode = useSettingsStore.getState().settings.streaming.streamOpenMode;
  return mode === "multistream" ? layoutChannels() : runningChannels();
}

export async function syncIndependentChatterino(channels: string[]) {
  if (!isTauri()) return;
  const settings = useSettingsStore.getState().settings;
  if (settings.chat.provider !== "chatterino") return;
  const desired = new Set(
    channels.flatMap((channel) => {
      const key = channel.trim().toLowerCase();
      return key ? [key] : [];
    }),
  );
  const opens: Promise<unknown>[] = [];
  for (const channel of desired) {
    if (independentChatChannels.has(channel)) continue;
    opens.push(
      invoke("open_independent_chatterino_chat", { channel })
        .then(() => independentChatChannels.add(channel))
        .catch((reason) => {
          useWatchingStore.setState({
            error:
              reason instanceof Error
                ? reason.message
                : `Chatterino7 failed to open: ${String(reason)}`,
          });
        }),
    );
  }
  const closes: Promise<unknown>[] = [];
  for (const channel of [...independentChatChannels]) {
    if (desired.has(channel)) continue;
    independentChatChannels.delete(channel);
    closes.push(
      invoke("close_independent_chatterino_chat", { channel }).catch(
        () => undefined,
      ),
    );
  }
  await Promise.allSettled([...opens, ...closes]);
}

