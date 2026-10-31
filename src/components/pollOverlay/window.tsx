import { getCurrentWindow } from "@tauri-apps/api/window";
import { overlayRectMoved, pollOverlayRect, type OverlayRect } from "../../lib/streaming/pollOverlay";
import { invoke, isTauri } from "../../lib/tauri";
import { OVERLAY_LABEL } from "./types";

export function pollChannelFromSearch() {
  return new URLSearchParams(window.location.search).get("channel")?.trim() ?? null;
}

export function overlayUrl(channel: string) {
  const params = new URLSearchParams({ overlay: "poll", channel });
  return `/?${params.toString()}`;
}

export async function measureEmbeddedChat(): Promise<OverlayRect | null> {
  const el = document.querySelector(".embedded-chat:not(.embedded-chat--empty)");
  if (!(el instanceof HTMLElement)) return null;
  const box = el.getBoundingClientRect();
  if (box.width < 80 || box.height < 80) return null;
  const win = getCurrentWindow();
  const [pos, scale] = await Promise.all([
    win.innerPosition().catch(() => null),
    win.scaleFactor().catch(() => 1),
  ]);
  if (!pos) return null;
  return {
    x: pos.x + box.left * scale,
    y: pos.y + box.top * scale,
    width: box.width * scale,
    height: box.height * scale,
  };
}

export async function resolvePollOverlayRect(): Promise<OverlayRect | null> {
  const chatterino = await invoke<OverlayRect | null>("poll_overlay_place").catch(
    () => null,
  );
  const chat = chatterino ?? (await measureEmbeddedChat());
  let main: OverlayRect | null = null;
  if (!chat) {
    const win = getCurrentWindow();
    const pos = await win.outerPosition().catch(() => null);
    const size = await win.outerSize().catch(() => null);
    if (pos && size) {
      main = { x: pos.x, y: pos.y, width: size.width, height: size.height };
    }
  }
  return pollOverlayRect(chat, main);
}

let placedChannel = "";
let overlayOpen = false;
let placing = false;
let lastPlacedRect: OverlayRect | null = null;

export async function placeOverlayWindow(channel: string) {
  if (!isTauri() || placing) return;
  placing = true;
  try {
    const rect = await resolvePollOverlayRect();
    if (!rect) {
      await closeOverlayWindow();
      return;
    }
    const [{ WebviewWindow }, { LogicalPosition, LogicalSize }] =
      await Promise.all([
        import("@tauri-apps/api/webviewWindow"),
        import("@tauri-apps/api/dpi"),
      ]);
    const scale = await getCurrentWindow().scaleFactor().catch(() => 1);
    const x = Math.round(rect.x / scale);
    const y = Math.round(rect.y / scale);
    const width = Math.round(rect.width / scale);
    const height = Math.round(rect.height / scale);
    const existing = overlayOpen
      ? await WebviewWindow.getByLabel(OVERLAY_LABEL)
      : null;
    if (existing && placedChannel === channel) {
      if (lastPlacedRect && !overlayRectMoved(lastPlacedRect, rect)) {
        return;
      }
      lastPlacedRect = rect;
      await existing.setAlwaysOnTop(true).catch(() => undefined);
      await existing.setPosition(new LogicalPosition(x, y)).catch(() => undefined);
      await existing.setSize(new LogicalSize(width, height)).catch(() => undefined);
      await invoke("poll_overlay_raise").catch(() => undefined);
      return;
    }
    placedChannel = channel;
    overlayOpen = true;
    lastPlacedRect = rect;
    await existing?.close().catch(() => undefined);
    new WebviewWindow(OVERLAY_LABEL, {
      url: overlayUrl(channel),
      title: "Poll",
      decorations: false,
      transparent: true,
      shadow: false,
      skipTaskbar: true,
      alwaysOnTop: true,
      resizable: false,
      visible: false,
      focus: false,
      x,
      y,
      width,
      height,
    });
  } finally {
    placing = false;
  }
}

export async function closeOverlayWindow() {
  if (!isTauri() || !overlayOpen) {
    placedChannel = "";
    overlayOpen = false;
    lastPlacedRect = null;
    return;
  }
  overlayOpen = false;
  placedChannel = "";
  lastPlacedRect = null;
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const overlay = await WebviewWindow.getByLabel(OVERLAY_LABEL);
  await overlay?.close().catch(() => undefined);
}

