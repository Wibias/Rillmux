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

const privileged = [
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

function expectNoDesktopPrivileges(name: string) {
  const capability = readCapability(name);
  for (const permission of privileged) {
    expect(capability.permissions).not.toContain(permission);
  }
  return capability;
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
    expect(build).toContain('"points_hud_place_window"');
  });

  test("keeps privileged desktop plugins and app commands on main", () => {
    const main = readCapability("default");
    expect(main.windows).toEqual(["main"]);
    expect(main.permissions).toContain("allow-app-quit");
    expect(main.permissions).toContain("allow-auth-logout");
    expect(main.permissions).toContain("allow-stream-start");
    expect(main.permissions).toContain("allow-points-hud-place-window");
    expect(main.permissions).not.toContain("allow-overlay-fit-webview");
    expect(main.permissions).not.toContain("allow-overlay-place-hud");
  });

  test("points HUD only gets reward and self-placement commands", () => {
    const points = expectNoDesktopPrivileges("overlay");
    expect(points.windows).toEqual(["points-hud-*"]);
    expect(points.permissions).toContain("allow-channel-points-redeem-reward");
    expect(points.permissions).toContain("allow-overlay-fit-webview");
    expect(points.permissions).toContain("allow-overlay-place-hud");
    expect(points.permissions).not.toContain("allow-channel-points-vote-poll");
    expect(points.permissions).not.toContain("allow-channel-points-vote-prediction");
  });

  test("poll overlay gets voting but no reward or HUD placement authority", () => {
    const poll = expectNoDesktopPrivileges("poll-overlay");
    expect(poll.windows).toEqual(["poll-overlay"]);
    expect(poll.permissions).toContain("allow-channel-points-vote-poll");
    expect(poll.permissions).toContain("allow-channel-points-vote-prediction");
    expect(poll.permissions).not.toContain("allow-channel-points-redeem-reward");
    expect(poll.permissions).not.toContain("allow-overlay-fit-webview");
    expect(poll.permissions).not.toContain("allow-overlay-place-hud");
  });

  test("raid overlay has no Channel Points mutation authority", () => {
    const raid = expectNoDesktopPrivileges("raid-overlay");
    expect(raid.windows).toEqual(["raid-overlay"]);
    for (const permission of [
      "allow-channel-points-redeem-reward",
      "allow-channel-points-vote-poll",
      "allow-channel-points-vote-prediction",
      "allow-overlay-fit-webview",
      "allow-overlay-place-hud",
    ]) {
      expect(raid.permissions).not.toContain(permission);
    }
  });
});
