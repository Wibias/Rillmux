import { describe, expect, it } from "vitest";
import {
  FOLLOWED_STREAM_PAGE_SIZE,
  LIVE_STREAM_QUERY,
  STREAM_THUMBNAIL_REFRESH_MS,
  gameBoxArt,
  streamThumbnail,
} from "./helix";

describe("streamThumbnail", () => {
  const template =
    "https://static-cdn.jtvnw.net/previews-ttv/live_user_lirik-{width}x{height}.jpg";

  it("sizes the preview and appends a minute-bucket cache-bust", () => {
    expect(streamThumbnail(template, 440, 248, 120_000)).toBe(
      "https://static-cdn.jtvnw.net/previews-ttv/live_user_lirik-440x248.jpg?t=2",
    );
  });

  it("keeps an existing query string", () => {
    expect(
      streamThumbnail(
        "https://static-cdn.jtvnw.net/previews-ttv/live_user_lirik-440x248.jpg?sig=1",
        440,
        248,
        60_000,
      ),
    ).toBe(
      "https://static-cdn.jtvnw.net/previews-ttv/live_user_lirik-440x248.jpg?sig=1&t=1",
    );
  });

  it("changes the cache-bust once the minute bucket advances", () => {
    const first = streamThumbnail(template, 320, 180, 59_999);
    const second = streamThumbnail(template, 320, 180, 60_000);
    expect(first).toContain("?t=0");
    expect(second).toContain("?t=1");
    expect(first).not.toBe(second);
  });
});

describe("gameBoxArt", () => {
  it("fills the Helix width/height template", () => {
    expect(
      gameBoxArt(
        "https://static-cdn.jtvnw.net/ttv-boxart/Fortnite-{width}x{height}.jpg",
      ),
    ).toBe("https://static-cdn.jtvnw.net/ttv-boxart/Fortnite-285x380.jpg");
  });

  it("upsizes the fixed thumbnail from search/categories", () => {
    expect(
      gameBoxArt(
        "https://static-cdn.jtvnw.net/ttv-boxart/Fortnite-52x72.jpg",
      ),
    ).toBe("https://static-cdn.jtvnw.net/ttv-boxart/Fortnite-285x380.jpg");
  });

  it("accepts an explicit size", () => {
    expect(
      gameBoxArt(
        "https://static-cdn.jtvnw.net/ttv-boxart/512710-52x72.jpg",
        144,
        192,
      ),
    ).toBe("https://static-cdn.jtvnw.net/ttv-boxart/512710-144x192.jpg");
  });
});

describe("LIVE_STREAM_QUERY", () => {
  it("keeps the last live page for a minute, then refreshes in the background", () => {
    expect(LIVE_STREAM_QUERY.refetchInterval).toBe(STREAM_THUMBNAIL_REFRESH_MS);
    expect(LIVE_STREAM_QUERY.refetchOnMount).toBe(true);
    expect(LIVE_STREAM_QUERY.staleTime).toBe(STREAM_THUMBNAIL_REFRESH_MS);
  });
});

describe("FOLLOWED_STREAM_PAGE_SIZE", () => {
  it("asks Helix for the maximum followed-live page", () => {
    expect(FOLLOWED_STREAM_PAGE_SIZE).toBe(100);
  });
});
