import { describe, expect, it } from "vitest";
import {
  LIVE_STREAM_QUERY,
  STREAM_THUMBNAIL_REFRESH_MS,
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

describe("LIVE_STREAM_QUERY", () => {
  it("refetches mounted live lists every minute", () => {
    expect(LIVE_STREAM_QUERY.refetchInterval).toBe(STREAM_THUMBNAIL_REFRESH_MS);
    expect(LIVE_STREAM_QUERY.refetchOnMount).toBe("always");
    expect(LIVE_STREAM_QUERY.staleTime).toBe(0);
  });
});
