import { describe, expect, it } from "vitest";
import { isOverlayWebview, shouldAttachDebugConsole } from "./debugConsole";

describe("shouldAttachDebugConsole", () => {
  it("lets the main window own the debug console", () => {
    expect(shouldAttachDebugConsole("")).toBe(true);
    expect(shouldAttachDebugConsole("?tab=streaming")).toBe(true);
  });

  it("skips overlay webviews so they cannot spam AllocConsole", () => {
    expect(isOverlayWebview("")).toBe(false);
    expect(isOverlayWebview("?overlay=points-hud&channel=forsen")).toBe(true);
    expect(shouldAttachDebugConsole("?overlay=points-hud&channel=forsen")).toBe(
      false,
    );
    expect(shouldAttachDebugConsole("?overlay=poll&channel=xqc")).toBe(false);
    expect(shouldAttachDebugConsole("?overlay=raid")).toBe(false);
  });
});
