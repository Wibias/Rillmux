import { describe, expect, it } from "vitest";
import { parseDeepLinkChannel } from "./DeepLinkAndUpdaterBootstrap";

describe("parseDeepLinkChannel", () => {
  it("accepts only the documented watch and channel routes", () => {
    expect(parseDeepLinkChannel("stg://watch/some_streamer")).toBe(
      "some_streamer",
    );
    expect(parseDeepLinkChannel("stg://channel/SomeStreamer")).toBe(
      "somestreamer",
    );
    expect(parseDeepLinkChannel("stg:///watch/some_streamer")).toBe(
      "some_streamer",
    );
  });

  it("rejects unsupported hosts and ambiguous fallback paths", () => {
    expect(parseDeepLinkChannel("stg://anything/some_streamer")).toBeNull();
    expect(parseDeepLinkChannel("stg:///some_streamer")).toBeNull();
    expect(parseDeepLinkChannel("stg://watch/some_streamer/extra")).toBeNull();
  });

  it("rejects wrong schemes and invalid Twitch logins", () => {
    expect(parseDeepLinkChannel("https://watch/some_streamer")).toBeNull();
    expect(parseDeepLinkChannel("stg://watch/not-valid!")).toBeNull();
    expect(parseDeepLinkChannel("stg://watch/abcdefghijklmnopqrstuvwxyz")).toBeNull();
  });
});
