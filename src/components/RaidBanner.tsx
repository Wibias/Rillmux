import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke, isTauri } from "../lib/tauri";
import { debugRuntimeEvent } from "../lib/diagnostics/runtimeDebug";
import {
  enqueueRaid,
  raidDedupeKey,
  raidOverlayRect,
  type OverlayRect,
  type RaidOutgoingEvent,
} from "../lib/streaming/raid";
import { useWatchingStore } from "../lib/streaming/store";
import { useSettingsStore } from "../lib/settings/store";
import "./RaidBanner.css";

const COUNTDOWN_SECS = 15;
const OVERLAY_LABEL = "raid-overlay";

function isRaidOverlayWindow() {
  return new URLSearchParams(window.location.search).get("overlay") === "raid";
}

function raidFromSearch(): RaidOutgoingEvent | null {
  const params = new URLSearchParams(window.location.search);
  const fromChannel = params.get("from")?.trim() ?? "";
  const toChannel = params.get("to")?.trim() ?? "";
  const toUserId = params.get("toUserId")?.trim() ?? "";
  if (!fromChannel || !toChannel) return null;
  const viewers = Number(params.get("viewers"));
  return {
    fromChannel,
    toChannel,
    toUserId,
    viewers: Number.isFinite(viewers) ? viewers : undefined,
  };
}

function overlayUrl(raid: RaidOutgoingEvent): string {
  const params = new URLSearchParams({
    overlay: "raid",
    from: raid.fromChannel,
    to: raid.toChannel,
    toUserId: raid.toUserId,
  });
  if (raid.viewers != null) params.set("viewers", String(raid.viewers));
  return `/?${params.toString()}`;
}

async function placeOverlayWindow(raid: RaidOutgoingEvent) {
  if (!isTauri()) return;
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const placed = await invoke<OverlayRect | null>("raid_overlay_place", {
    fromChannel: raid.fromChannel,
  }).catch(() => null);
  let rect = placed;
  if (!rect) {
    const main = getCurrentWindow();
    const pos = await main.outerPosition().catch(() => null);
    const size = await main.outerSize().catch(() => null);
    if (pos && size) {
      rect = raidOverlayRect(null, null, {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
      });
    }
  }
  if (!rect) return;

  const existing = await WebviewWindow.getByLabel(OVERLAY_LABEL);
  await existing?.close().catch(() => undefined);
  const scale = await getCurrentWindow().scaleFactor().catch(() => 1);
  new WebviewWindow(OVERLAY_LABEL, {
    url: overlayUrl(raid),
    title: "Raid",
    decorations: false,
    transparent: true,
    shadow: false,
    skipTaskbar: true,
    alwaysOnTop: true,
    resizable: false,
    focus: true,
    x: Math.round(rect.x / scale),
    y: Math.round(rect.y / scale),
    width: Math.round(rect.width / scale),
    height: Math.round(rect.height / scale),
  });
}

async function closeOverlayWindow() {
  if (!isTauri()) return;
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const overlay = await WebviewWindow.getByLabel(OVERLAY_LABEL);
  await overlay?.close().catch(() => undefined);
}

function RaidOverlayPrompt() {
  const { t } = useTranslation("common");
  const raid = raidFromSearch();
  const [seconds, setSeconds] = useState(COUNTDOWN_SECS);
  const sentRef = useRef(false);

  useEffect(() => {
    const tick = window.setInterval(() => {
      setSeconds((value) => Math.max(0, value - 1));
    }, 1000);
    return () => window.clearInterval(tick);
  }, []);

  async function finish(action: "follow" | "stay") {
    if (!raid || sentRef.current) return;
    sentRef.current = true;
    if (isTauri()) {
      await emit(action === "follow" ? "raid-overlay-follow" : "raid-overlay-stay", raid);
    }
    await getCurrentWindow().close().catch(() => undefined);
  }

  useEffect(() => {
    if (seconds > 0) return;
    void finish("follow");
    // eslint-disable-next-line react-hooks/exhaustive-deps -- finish closes over raid
  }, [seconds]);

  if (!raid) return null;

  return (
    <div className="raid-banner raid-banner--overlay" role="status">
      <div className="raid-banner__text">
        <strong>
          {t("raidBannerTitle", {
            from: raid.fromChannel,
            to: raid.toChannel,
          })}
        </strong>
        <span className="muted">{t("raidBannerBody", { seconds })}</span>
      </div>
      <div className="raid-banner__actions">
        <button type="button" className="button-primary" onClick={() => void finish("follow")}>
          {t("raidFollowNow")}
        </button>
        <button type="button" className="button-secondary" onClick={() => void finish("stay")}>
          {t("raidStay")}
        </button>
      </div>
    </div>
  );
}

/**
 * Host window listens for EventSub raids and opens an always-on-top overlay
 * over mpv or Chatterino. The overlay itself only renders the prompt.
 */
export function RaidBanner() {
  const [queue, setQueue] = useState<RaidOutgoingEvent[]>([]);
  const cooldownRef = useRef<Set<string>>(new Set());
  const followingRef = useRef(false);
  const followRaids = useSettingsStore((s) => s.settings.streaming.followRaids);
  const active = queue[0] ?? null;

  useEffect(() => {
    if (!isTauri() || isRaidOverlayWindow()) return;
    let unlisten: (() => void) | undefined;
    void listen<RaidOutgoingEvent>("raid-outgoing", (event) => {
      if (!useSettingsStore.getState().settings.streaming.followRaids) return;
      const payload = event.payload;
      if (!payload?.fromChannel || !payload?.toChannel) return;
      debugRuntimeEvent("raids", "raid.received", {
        from: payload.fromChannel.toLowerCase(),
        to: payload.toChannel.toLowerCase(),
        viewers: payload.viewers ?? 0,
      });
      const key = raidDedupeKey(payload);
      if (cooldownRef.current.has(key)) {
        debugRuntimeEvent("raids", "raid.received.duplicate", {
          from: payload.fromChannel.toLowerCase(),
          to: payload.toChannel.toLowerCase(),
        });
        return;
      }
      setQueue((q) => enqueueRaid(q, payload));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (!isTauri() || isRaidOverlayWindow()) return;
    let unFollow: (() => void) | undefined;
    let unStay: (() => void) | undefined;
    void listen<RaidOutgoingEvent>("raid-overlay-follow", (event) => {
      if (!event.payload?.fromChannel) return;
      void accept(event.payload);
    }).then((fn) => {
      unFollow = fn;
    });
    void listen<RaidOutgoingEvent>("raid-overlay-stay", (event) => {
      if (!event.payload?.fromChannel) return;
      stay(event.payload);
    }).then((fn) => {
      unStay = fn;
    });
    return () => {
      unFollow?.();
      unStay?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- accept/stay close over refs
  }, []);

  useEffect(() => {
    if (isRaidOverlayWindow()) return;
    if (!active || !followRaids) {
      void closeOverlayWindow();
      return;
    }
    void placeOverlayWindow(active);
  }, [active?.fromChannel, active?.toChannel, active?.toUserId, followRaids]);

  async function accept(raid: RaidOutgoingEvent) {
    if (followingRef.current) return;
    followingRef.current = true;
    const key = raidDedupeKey(raid);
    cooldownRef.current.add(key);
    window.setTimeout(() => cooldownRef.current.delete(key), 60_000);
    try {
      await useWatchingStore.getState().followRaid(raid);
    } catch {
      // error already in store
    } finally {
      followingRef.current = false;
      setQueue((q) => q.filter((item) => raidDedupeKey(item) !== key));
    }
  }

  function stay(raid: RaidOutgoingEvent) {
    const key = raidDedupeKey(raid);
    cooldownRef.current.add(key);
    window.setTimeout(() => cooldownRef.current.delete(key), 60_000);
    setQueue((q) => q.filter((item) => raidDedupeKey(item) !== key));
  }

  if (isRaidOverlayWindow()) {
    return <RaidOverlayPrompt />;
  }
  return null;
}
