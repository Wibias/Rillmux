import { describe, expect, it } from "vitest";
import { formatStartedAt, formatUptime, formatViewers, twitchChannelUrl } from "./format";

describe("formatViewers", () => {
  it("keeps small counts raw", () => {
    expect(formatViewers(42)).toBe("42");
  });

  it("abbreviates thousands", () => {
    expect(formatViewers(8100)).toBe("8.1K");
    expect(formatViewers(12_000)).toBe("12K");
  });
});

describe("formatUptime", () => {
  const now = Date.parse("2026-08-20T12:00:00Z");

  it("formats minutes, hours, and days", () => {
    expect(formatUptime("2026-08-20T11:13:00Z", now)).toBe("47m");
    expect(formatUptime("2026-08-20T09:13:00Z", now)).toBe("2h 47m");
    expect(formatUptime("2026-08-19T10:00:00Z", now)).toBe("1d 2h");
  });
});

describe("twitchChannelUrl", () => {
  it("points at the channel page", () => {
    expect(twitchChannelUrl("Forsen")).toBe("https://www.twitch.tv/Forsen");
  });
});

describe("formatStartedAt", () => {
  it("labels today and yesterday in local time", () => {
    const now = Date.parse("2026-08-20T15:00:00");
    const today = new Date(now);
    today.setHours(10, 42, 0, 0);
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);
    expect(formatStartedAt(today.toISOString(), now, "en-US")).toMatch(
      /^Today, /,
    );
    expect(formatStartedAt(yesterday.toISOString(), now, "en-US")).toMatch(
      /^Yesterday, /,
    );
  });
});
