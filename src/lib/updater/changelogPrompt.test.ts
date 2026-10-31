import { describe, expect, it } from "vitest";
import { shouldShowChangelogAfterUpdate } from "./changelogPrompt";

describe("post-update changelog", () => {
  it("shows once for a new version after setup is complete", () => {
    expect(
      shouldShowChangelogAfterUpdate({
        currentVersion: "0.5.22",
        lastSeenVersion: "0.5.21",
        onboardingDone: true,
      }),
    ).toBe(true);
    expect(
      shouldShowChangelogAfterUpdate({
        currentVersion: "v0.5.22",
        lastSeenVersion: "0.5.21",
        onboardingDone: true,
      }),
    ).toBe(true);
  });

  it("stays quiet for the same version or during first-run setup", () => {
    expect(
      shouldShowChangelogAfterUpdate({
        currentVersion: "0.5.22",
        lastSeenVersion: "v0.5.22",
        onboardingDone: true,
      }),
    ).toBe(false);
    expect(
      shouldShowChangelogAfterUpdate({
        currentVersion: "0.5.22",
        lastSeenVersion: null,
        onboardingDone: false,
      }),
    ).toBe(false);
  });
});
