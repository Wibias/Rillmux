import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

function readCapability(name: string) {
  return JSON.parse(
    readFileSync(
      new URL(`../src-tauri/capabilities/${name}.json`, import.meta.url),
      "utf8",
    ),
  ) as { windows: string[]; permissions: string[] };
}

describe("Tauri capability boundaries", () => {
  test("registers app commands with the Tauri ACL", () => {
    const build = readFileSync(
      new URL("../src-tauri/build.rs", import.meta.url),
      "utf8",
    );
    expect(build).toContain("AppManifest::new().commands(COMMANDS)");
    expect(build).toContain('"app_quit"');
    expect(build).toContain('"channel_points_redeem_reward"');
  });

  test("keeps privileged desktop plugins and app commands on main", () => {
    const main = readCapability("default");
    expect(main.windows).toEqual(["main"]);
    expect(main.permissions).toContain("allow-app-quit");
    expect(main.permissions).toContain("allow-auth-logout");
    expect(main.permissions).toContain("allow-stream-start");
  });

  test("overlay windows only get the app commands they need", () => {
    const overlay = readCapability("overlay");
    expect(overlay.windows).toEqual([
      "raid-overlay",
      "poll-overlay",
      "points-hud-*",
    ]);

    const forbidden = [
      "core:default",
      "updater:default",
      "process:default",
      "opener:default",
      "deep-link:default",
      "notification:default",
      "dialog:allow-open",
      "window-controls:default",
      "core:webview:allow-create-webview-window",
      "core:tray:default",
      "allow-app-quit",
      "allow-auth-logout",
      "allow-stream-start",
      "allow-twitch-web-auth-save",
    ];
    for (const permission of forbidden) {
      expect(overlay.permissions).not.toContain(permission);
    }

    expect(overlay.permissions).toContain("allow-channel-points-redeem-reward");
    expect(overlay.permissions).toContain("allow-channel-points-vote-poll");
    expect(overlay.permissions).toContain("allow-channel-points-vote-prediction");
  });
});
