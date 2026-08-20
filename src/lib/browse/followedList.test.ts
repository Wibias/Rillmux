import { describe, expect, it } from "vitest";
import type { HelixStream } from "../twitch/helix";
import {
  filterFollowedStreams,
  followedDetailTags,
  followedVisibleCount,
  paginateItems,
  partitionPinned,
  sortFollowedStreams,
} from "./followedList";

function stream(
  patch: Partial<HelixStream> & Pick<HelixStream, "user_login">,
): HelixStream {
  return {
    id: patch.id ?? patch.user_login,
    user_id: patch.user_id ?? "1",
    user_login: patch.user_login,
    user_name: patch.user_name ?? patch.user_login,
    game_id: "1",
    game_name: patch.game_name ?? "Just Chatting",
    type: "live",
    title: patch.title ?? "Hello",
    viewer_count: patch.viewer_count ?? 10,
    started_at: patch.started_at ?? "2026-08-20T10:00:00Z",
    language: patch.language ?? "en",
    thumbnail_url: "",
    is_mature: patch.is_mature ?? false,
    tags: patch.tags,
  };
}

describe("filterFollowedStreams", () => {
  const streams = [
    stream({ user_login: "forsen", title: "Minecraft", game_name: "Minecraft" }),
    stream({ user_login: "xqc", title: "slots", is_mature: true }),
  ];

  it("matches channel, game, or title and can hide mature", () => {
    expect(
      filterFollowedStreams(streams, { query: "mine", hideMature: false }).map(
        (s) => s.user_login,
      ),
    ).toEqual(["forsen"]);
    expect(
      filterFollowedStreams(streams, { query: "", hideMature: true }).map(
        (s) => s.user_login,
      ),
    ).toEqual(["forsen"]);
  });
});

describe("sortFollowedStreams", () => {
  it("sorts by viewers descending", () => {
    const sorted = sortFollowedStreams(
      [
        stream({ user_login: "a", viewer_count: 10 }),
        stream({ user_login: "b", viewer_count: 50 }),
      ],
      "viewers-desc",
    );
    expect(sorted.map((s) => s.user_login)).toEqual(["b", "a"]);
  });
});

describe("partitionPinned", () => {
  it("keeps pin order and removes them from the rest", () => {
    const { pinned, rest } = partitionPinned(
      [
        stream({ user_login: "a" }),
        stream({ user_login: "b" }),
        stream({ user_login: "c" }),
      ],
      ["c", "a"],
    );
    expect(pinned.map((s) => s.user_login)).toEqual(["c", "a"]);
    expect(rest.map((s) => s.user_login)).toEqual(["b"]);
  });
});

describe("paginateItems", () => {
  it("clamps the page and reports the visible range", () => {
    const result = paginateItems(["a", "b", "c", "d", "e"], 2, 2);
    expect(result.pageItems).toEqual(["c", "d"]);
    expect(result.start).toBe(3);
    expect(result.end).toBe(4);
    expect(result.pageCount).toBe(3);
    expect(paginateItems(["a"], 9, 24).page).toBe(1);
  });
});

describe("followedVisibleCount", () => {
  it("fits list rows into the available height", () => {
    expect(
      followedVisibleCount({ view: "list", width: 900, height: 400 }),
    ).toBe(6);
    expect(
      followedVisibleCount({ view: "list", width: 900, height: 80 }),
    ).toBe(1);
  });

  it("fits a grid from card min-width and 16:9 thumbs", () => {
    expect(
      followedVisibleCount({ view: "grid", width: 640, height: 500 }),
    ).toBe(6);
    expect(
      followedVisibleCount({ view: "grid", width: 200, height: 180 }),
    ).toBe(1);
  });
});

describe("followedDetailTags", () => {
  it("adds language, unique tags, and a mature marker", () => {
    expect(
      followedDetailTags(
        stream({
          user_login: "ratirl",
          language: "en",
          is_mature: true,
          tags: ["Ranked", "English", "SoloQ"],
        }),
      ),
    ).toEqual(["English", "Ranked", "SoloQ", "18+"]);
  });
});
