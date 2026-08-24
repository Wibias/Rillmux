import { describe, expect, it } from "vitest";
import { shouldCreateDesktopTray } from "./tray";

describe("shouldCreateDesktopTray", () => {
  it("skips the tray during Vite / tauri:dev so console kills do not stack ghosts", () => {
    expect(shouldCreateDesktopTray(true)).toBe(false);
  });

  it("keeps the tray in production windows", () => {
    expect(shouldCreateDesktopTray(false)).toBe(true);
  });
});
