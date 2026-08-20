import { describe, expect, it } from "vitest";
import { isSettingsTab, settingsTabFromPath } from "./tabs";

describe("settings tabs", () => {
  it("accepts known tab ids", () => {
    expect(isSettingsTab("streaming")).toBe(true);
    expect(isSettingsTab("streamlink")).toBe(false);
  });

  it("reads the last path segment and defaults to interface", () => {
    expect(settingsTabFromPath("/settings/player")).toBe("player");
    expect(settingsTabFromPath("/settings")).toBe("interface");
    expect(settingsTabFromPath("/settings/nope")).toBe("interface");
  });
});
