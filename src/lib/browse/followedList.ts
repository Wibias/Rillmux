import type { HelixStream } from "../twitch/helix";
import { languageLabel } from "../twitch/languages";

export const FOLLOWED_SORTS = [
  "viewers-desc",
  "viewers-asc",
  "uptime-desc",
  "name",
] as const;

export type FollowedSort = (typeof FOLLOWED_SORTS)[number];

export const FOLLOWED_VIEWS = ["grid", "list"] as const;

export type FollowedView = (typeof FOLLOWED_VIEWS)[number];

export const FOLLOWED_PAGE_SIZES = [12, 24, 48] as const;

export type FollowedPageSize = (typeof FOLLOWED_PAGE_SIZES)[number];

export function isFollowedSort(value: unknown): value is FollowedSort {
  return (
    typeof value === "string" &&
    (FOLLOWED_SORTS as readonly string[]).includes(value)
  );
}

export function isFollowedView(value: unknown): value is FollowedView {
  return (
    typeof value === "string" &&
    (FOLLOWED_VIEWS as readonly string[]).includes(value)
  );
}

export function isFollowedPageSize(value: unknown): value is FollowedPageSize {
  return (
    typeof value === "number" &&
    (FOLLOWED_PAGE_SIZES as readonly number[]).includes(value)
  );
}

export function filterFollowedStreams(
  streams: HelixStream[],
  opts: { query: string; hideMature: boolean },
): HelixStream[] {
  const needle = opts.query.trim().toLowerCase();
  return streams.filter((stream) => {
    if (opts.hideMature && stream.is_mature) return false;
    if (!needle) return true;
    return (
      stream.user_login.toLowerCase().includes(needle) ||
      stream.user_name.toLowerCase().includes(needle) ||
      stream.game_name.toLowerCase().includes(needle) ||
      stream.title.toLowerCase().includes(needle)
    );
  });
}

export function sortFollowedStreams(
  streams: HelixStream[],
  sort: FollowedSort,
): HelixStream[] {
  const copy = [...streams];
  copy.sort((a, b) => {
    switch (sort) {
      case "viewers-desc":
        return b.viewer_count - a.viewer_count;
      case "viewers-asc":
        return a.viewer_count - b.viewer_count;
      case "uptime-desc":
        return Date.parse(a.started_at) - Date.parse(b.started_at);
      case "name":
        return a.user_name.localeCompare(b.user_name, undefined, {
          sensitivity: "base",
        });
      default: {
        const _never: never = sort;
        return _never;
      }
    }
  });
  return copy;
}

export function partitionPinned(
  streams: HelixStream[],
  pinnedLogins: string[],
): { pinned: HelixStream[]; rest: HelixStream[] } {
  const order = new Map(
    pinnedLogins.map((login, index) => [login.toLowerCase(), index]),
  );
  const pinned: HelixStream[] = [];
  const rest: HelixStream[] = [];
  for (const stream of streams) {
    if (order.has(stream.user_login.toLowerCase())) {
      pinned.push(stream);
    } else {
      rest.push(stream);
    }
  }
  pinned.sort(
    (a, b) =>
      (order.get(a.user_login.toLowerCase()) ?? 0) -
      (order.get(b.user_login.toLowerCase()) ?? 0),
  );
  return { pinned, rest };
}

export function paginateItems<T>(
  items: T[],
  page: number,
  pageSize: number,
): { pageItems: T[]; page: number; pageCount: number; start: number; end: number } {
  const pageCount = Math.max(1, Math.ceil(items.length / pageSize) || 1);
  const safePage = Math.min(Math.max(1, page), pageCount);
  const startIndex = (safePage - 1) * pageSize;
  const pageItems = items.slice(startIndex, startIndex + pageSize);
  return {
    pageItems,
    page: safePage,
    pageCount,
    start: items.length === 0 ? 0 : startIndex + 1,
    end: startIndex + pageItems.length,
  };
}

const LIST_HEAD_PX = 40;
const LIST_ROW_PX = 56;
const GRID_MIN_CARD_PX = 200;
const GRID_COL_GAP_PX = 16;
const GRID_ROW_GAP_PX = 18;
const GRID_CARD_BODY_PX = 72;

/** How many followed streams fit in the visible list/grid without scrolling. */
export function followedVisibleCount(opts: {
  view: FollowedView;
  width: number;
  height: number;
}): number {
  const width = Math.max(0, opts.width);
  const height = Math.max(0, opts.height);
  if (opts.view === "list") {
    return Math.max(1, Math.floor((height - LIST_HEAD_PX) / LIST_ROW_PX));
  }
  const cols = Math.max(
    1,
    Math.floor((width + GRID_COL_GAP_PX) / (GRID_MIN_CARD_PX + GRID_COL_GAP_PX)),
  );
  const cardWidth = Math.max(
    GRID_MIN_CARD_PX,
    (width - GRID_COL_GAP_PX * (cols - 1)) / cols,
  );
  const cardHeight = cardWidth * (9 / 16) + GRID_CARD_BODY_PX;
  const rows = Math.max(
    1,
    Math.floor((height + GRID_ROW_GAP_PX) / (cardHeight + GRID_ROW_GAP_PX)),
  );
  return cols * rows;
}

export function followedDetailTags(stream: HelixStream): string[] {
  const tags: string[] = [];
  const seen = new Set<string>();
  const push = (value: string) => {
    const label = value.trim();
    const key = label.toLowerCase();
    if (!label || seen.has(key)) return;
    seen.add(key);
    tags.push(label);
  };
  if (stream.language) push(languageLabel(stream.language));
  for (const tag of stream.tags ?? []) push(tag);
  if (stream.is_mature) push("18+");
  return tags.slice(0, 6);
}

export function togglePinnedLogin(pins: string[], login: string): string[] {
  const key = login.trim().toLowerCase();
  if (!key) return pins;
  return pins.includes(key)
    ? pins.filter((item) => item !== key)
    : [...pins, key];
}
