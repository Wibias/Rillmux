import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

function readCapability(name: string) {
  return JSON.parse(
    readFileSync(new URL(`../src-tauri/capabilities/${name}.json`, import.meta.url), "utf8"),
  ) as { windows: string[]; permissions: string[] };
}

describe("Tauri capability boundaries", () => {
  test("keeps privileged desktop plugins on the main window only", () => {
    const main = readCapability("default");
    expect(main.windows).toEqual(["main"]);
  });

  test("overlay windows cannot use main-only desktop plugins", () => {
    const overlay = readCapability("overlay");
    expect(overlay.windows).toEqual(["raid-overlay", "poll-overlay", "points-hud-*"]);

    const forbidden = [
      "updater:default",
      "process:default",
      "opener:default",
      "deep-link:default",
      "notification:default",
      "dialog:allow-open",
      "window-controls:default",
      "core:webview:allow-create-webview-window",
      "core:tray:default",
    ];
    for (const permission of forbidden) {
      expect(overlay.permissions).not.toContain(permission);
    }
  });
});
