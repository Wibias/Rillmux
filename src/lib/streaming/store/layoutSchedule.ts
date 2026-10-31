import { invoke, isTauri } from "../../tauri";
import { useSettingsStore } from "../../settings/store";
import { debugRuntimeEvent } from "../../diagnostics/runtimeDebug";
import { syncViewerPresence } from "./presenceSync";
import { currentLayout, runningChannels, layoutChannels, chatterinoChannels, syncIndependentChatterino } from "./layoutChannels";
import { syncChatterino } from "./chatterinoStoreSync";
import { syncEventSub } from "./listeners";
import { layoutState } from "./state";

export function scheduleLayoutAfterReady() {
  if (!isTauri()) return;
  if (layoutState.timer) clearTimeout(layoutState.timer);
  layoutState.timer = setTimeout(() => {
    layoutState.timer = null;
    const settings = useSettingsStore.getState().settings;
    if (settings.streaming.streamOpenMode === "independent") {
      const channels =
        settings.chat.provider === "chatterino" ? runningChannels() : [];
      debugRuntimeEvent("windows", "layout.request", {
        channels: channels.join(","),
        reserveChat: channels.length > 0,
        layout: channels.length ? "independent" : "",
        linkedDock: channels.length > 0,
        chatFraction: settings.streaming.chatWidthFraction,
      });
      void invoke("layout_watching", {
        channels,
        reserveChat: false,
        layout: channels.length ? "independent" : undefined,
        linkedDock: false,
        chatFraction: settings.streaming.chatWidthFraction,
      }).catch(() => undefined);
      return;
    }
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
    const reserveChat = settings.chat.provider === "chatterino";
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

export function afterSessionsChanged() {
  const channels = runningChannels();
  debugRuntimeEvent("windows", "sessions.changed", {
    channels: channels.join(","),
    count: channels.length,
    layoutChannels: layoutChannels().join(","),
  });
  if (useSettingsStore.getState().settings.streaming.streamOpenMode === "independent") {
    void syncIndependentChatterino(chatterinoChannels());
  } else {
    void syncChatterino(chatterinoChannels());
  }
  syncEventSub();
  syncViewerPresence();
  scheduleLayoutAfterReady();
}

