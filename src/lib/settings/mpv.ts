import { CHAT_WIDTH_FRACTION } from "../streaming/layout";

/** Wiki-oriented mpv preset toggles (upstream Streamlink Twitch GUI Recommendations). */
export interface MpvPresetSettings {
  /** --no-border */
  noBorder: boolean;
  /** --no-keepaspect-window */
  noKeepaspectWindow: boolean;
  /** --window-maximized=yes */
  windowMaximized: boolean;
  /** --loop-playlist=inf --loop-file=inf (Enter reloads) */
  loopReload: boolean;
  /** --cache=yes --demuxer-max-back-bytes=250M */
  cacheRewind: boolean;
  /**
   * --volume=N --volume-max=N. 100 leaves mpv at its default ceiling (130),
   * so nothing above 100 % happens unless the user opts in.
   */
  volumeBoost: number;
}

/**
 * Off by default. mpv clamps `volume` to `volume-max` (100-1000, default 130),
 * so a boost needs both flags; anything above 100 % is software gain and can
 * clip on already-loud streams.
 */
export const MPV_VOLUME_BOOSTS = [100, 130, 150, 200, 300] as const;

export const DEFAULT_MPV_VOLUME_BOOST = 100;

/** Old settings blobs / hand-edited JSON must not inject an arbitrary gain. */
export function normalizeMpvVolumeBoost(value: unknown): number {
  const parsed = typeof value === "number" ? value : Number(value);
  return (MPV_VOLUME_BOOSTS as readonly number[]).includes(parsed)
    ? parsed
    : DEFAULT_MPV_VOLUME_BOOST;
}

export const defaultMpvPresets = (): MpvPresetSettings => ({
  noBorder: true,
  noKeepaspectWindow: true,
  windowMaximized: true,
  loopReload: true,
  cacheRewind: true,
  volumeBoost: DEFAULT_MPV_VOLUME_BOOST,
});

export const MPV_WINGET = "winget install -e --id shinchiro.mpv";
export const MPV_SCOOP = "scoop install mpv";
export const MPV_PORTABLE_URL =
  "https://github.com/shinchiro/mpv-winbuild-cmake/releases";

function sanitizePlayerChannel(value: string): string {
  const cleaned = [...value]
    .filter((c) => /[a-z0-9_-]/i.test(c))
    .join("")
    .toLowerCase();
  return cleaned || "stream";
}

function sanitizePlayerFragment(value: string): string {
  const cleaned = value
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "_")
    .replace(/^[_-]+|[_-]+$/g, "")
    .slice(0, 80)
    .replace(/_+$/g, "");
  return cleaned || "stream";
}

/** Window title shown by mpv: `<channel>-<stream_title>`. */
export function mpvWindowTitle(channel: string, streamTitle: string): string {
  return `${sanitizePlayerChannel(channel)}-${sanitizePlayerFragment(streamTitle)}`;
}

/** Build Streamlink --player-args for mpv from toggles + optional extras. */
export function composeMpvPlayerArgs(
  presets: MpvPresetSettings,
  customExtras: string,
  meta: { channel: string; title: string; game: string },
  opts?: {
    sideBySideChat?: boolean;
    /** Full `--geometry=…` flag; overrides maximized / side chat defaults. */
    geometry?: string;
    /** Skip maximized/geometry — OS layout will position after the player is ready. */
    deferLayout?: boolean;
  },
): string {
  const label = mpvWindowTitle(meta.channel, meta.title || meta.channel);
  const parts: string[] = [
    "--force-window=yes",
    "--keep-open=no",
  ];
  if (presets.noBorder) parts.push("--no-border");
  if (presets.noKeepaspectWindow) parts.push("--no-keepaspect-window");
  if (opts?.geometry) {
    parts.push(opts.geometry);
  } else if (opts?.deferLayout) {
    // Leave positioning to layout_watching (work-area dock).
  } else if (opts?.sideBySideChat) {
    const videoPct = Math.round((1 - CHAT_WIDTH_FRACTION) * 100);
    parts.push(`--geometry=${videoPct}%x100%+0+0`);
  } else if (presets.windowMaximized) {
    parts.push("--window-maximized=yes");
  }
  if (presets.loopReload) {
    parts.push("--loop-playlist=inf");
    parts.push("--loop-file=inf");
  }
  if (presets.cacheRewind) {
    parts.push("--cache=yes");
    parts.push("--demuxer-max-back-bytes=250M");
  }
  if (presets.volumeBoost > 100) {
    // Order matters: mpv clamps volume to volume-max, so raise the ceiling
    // first (last-one-wins for repeated options).
    parts.push(`--volume-max=${presets.volumeBoost}`);
    parts.push(`--volume=${presets.volumeBoost}`);
  }
  parts.push(`--title="${label}"`);
  parts.push(`--force-media-title="${label}"`);
  const extras = customExtras.trim();
  if (extras) parts.push(extras);
  return parts.join(" ");
}

/** Plain-language summary of active recommended flags. */
export function describeMpvPresets(presets: MpvPresetSettings): string[] {
  const items: string[] = [];
  if (presets.noBorder) items.push("borderless window");
  if (presets.noKeepaspectWindow) items.push("letterboxing when resized");
  if (presets.windowMaximized) items.push("start maximized");
  if (presets.loopReload) items.push("Enter reloads the stream");
  if (presets.cacheRewind) items.push("cache + rewind buffer");
  if (presets.volumeBoost > 100) {
    items.push(`volume boost ${presets.volumeBoost}%`);
  }
  return items;
}
