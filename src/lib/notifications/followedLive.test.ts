import { describe, expect, it } from "vitest";
import {
  followedStreamsQueryKey,
  liveFollowedLogins,
  newlyLiveFollowedLogins,
  shouldNotifyFollowedLive,
  toggleMutedFollowed,
} from "./followedLive";

describe("shouldNotifyFollowedLive", () => {
  it("respects global off and mute list", () => {
    expect(
      shouldNotifyFollowedLive("forsen", {
        followedOnline: false,
        mutedFollowed: [],
      }),
    ).toBe(false);
    expect(
      shouldNotifyFollowedLive("forsen", {
        followedOnline: true,
        mutedFollowed: ["Forsen"],
      }),
    ).toBe(false);
    expect(
      shouldNotifyFollowedLive("xqc", {
        followedOnline: true,
        mutedFollowed: ["forsen"],
      }),
    ).toBe(true);
  });
});

describe("toggleMutedFollowed", () => {
  it("adds and removes muted logins", () => {
    expect(toggleMutedFollowed([], "Forsen", false)).toEqual(["forsen"]);
    expect(toggleMutedFollowed(["forsen", "xqc"], "forsen", true)).toEqual([
      "xqc",
    ]);
  });
});

describe("followed live query sharing", () => {
  it("uses the Followed page query key so tray notifications reuse that cache", () => {
    expect(followedStreamsQueryKey("45537718")).toEqual([
      "followed-streams",
      "45537718",
    ]);
  });

  it("notifies only logins that were not already live", () => {
    const previous = liveFollowedLogins([{ user_login: "Forsen" }]);
    const next = liveFollowedLogins([
      { user_login: "Forsen" },
      { user_login: "xQc" },
    ]);
    expect(newlyLiveFollowedLogins(previous, next)).toEqual(["xqc"]);
  });
});
