import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const read = (path: string) =>
  readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

describe("full-review regression gates", () => {
  test("CodeQL does not reference matrix in the job-level if", () => {
    const workflow = read(".github/workflows/codeql.yml");
    const analyzeHeader = workflow.slice(
      workflow.indexOf("  analyze:"),
      workflow.indexOf("    runs-on:", workflow.indexOf("  analyze:")),
    );
    expect(analyzeHeader).not.toContain("matrix.language");
    expect(workflow).toContain("fromJSON(needs.changes.outputs.languages)");
  });

  test("CI explicitly rustfmt-checks include shards", () => {
    const workflow = read(".github/workflows/ci.yml");
    for (const file of [
      "foundation.rs",
      "types_player.rs",
      "tools_process.rs",
      "dock.rs",
      "overlays.rs",
      "windows_layout.rs",
      "runtime.rs",
      "tests.rs",
    ]) {
      expect(workflow).toContain(`src/streaming/${file}`);
    }
    expect(workflow).toContain("rustfmt --edition 2021 --check");
  });

  test("updater checks again every 60 minutes", () => {
    const source = read("src/components/UpdateBanner.tsx");
    expect(source).toContain("60 * 60 * 1000");
    expect(source).toContain("setInterval");
    expect(source).toContain("shouldPromptAppUpdate");
    expect(source).toContain("import.meta.env.DEV");
  });

  test("disabled Channel Points HUD does not poll website auth", () => {
    const source = read("src/components/ChannelPointsHudSync.tsx");
    const syncBody = source.slice(source.indexOf("const sync = async"));
    const disabledGuard = syncBody.indexOf("if (!hudEnabled)");
    const authRead = syncBody.indexOf("getTwitchWebsiteAuthStatus");
    expect(disabledGuard).toBeGreaterThanOrEqual(0);
    expect(authRead).toBeGreaterThan(disabledGuard);
  });

  test("stale Channel Points HUD sync cannot recreate windows after cleanup", () => {
    const source = read("src/components/ChannelPointsHudSync.tsx");
    expect(source).toContain("isActive: () => boolean");
    expect(source).toContain("if (!isActive()) return false;");
    const syncBody = source.slice(source.indexOf("const sync = async"));
    const authRead = syncBody.indexOf("getTwitchWebsiteAuthStatus");
    const activeCheck = syncBody.indexOf("if (!active) return;", authRead);
    expect(activeCheck).toBeGreaterThan(authRead);
    expect(syncBody).toContain("if (!hudReady || !active) return;");
  });

  test("Channel Points HUD geometry uses one native placement path", () => {
    const source = read("src/components/ChannelPointsHud.tsx");
    const flushBody = source.slice(
      source.indexOf("async function flushOverlayRect"),
      source.indexOf("export function ChannelPointsHud"),
    );
    expect(flushBody).toContain('invoke("overlay_place_hud"');
    expect(flushBody).not.toContain("PhysicalPosition");
    expect(flushBody).not.toContain("PhysicalSize");
    expect(flushBody).not.toContain("win.setPosition");
    expect(flushBody).not.toContain("win.setSize");
    expect(flushBody).not.toContain("overlay_fit_webview");
    expect(source).toContain("frame = window.requestAnimationFrame(apply)");
    expect(source).not.toContain(
      "secondFrame = window.requestAnimationFrame(apply)",
    );
  });

  test("dock divider is a hairline at rest and thickens only while hovered", () => {
    const source = read("src-tauri/src/dock.rs");
    expect(source).toContain("pub const DIVIDER_REST_THICK: i32 = 2;");
    expect(source).toContain("pub const DIVIDER_HOVER_THICK: i32 = 8;");
    expect(source).toContain("fn divider_thickness(hover: bool)");
    expect(source).toContain("resize_divider_hwnd");
    expect(source).toContain("WM_MOUSELEAVE if divider_kind(kind)");
    expect(source).toContain("r.width().max(DIVIDER_REST_THICK)");
    expect(source).not.toContain("const BASE_DIVIDER_THICK: i32 = 8;");
    expect(source).not.toContain("const DIVIDER_WIDTH_PERCENT: i32 = 80;");
  });

  test("diagnostics bounds the always-on log", () => {
    const source = read("src-tauri/src/diagnostics.rs");
    expect(source).toContain("MAX_LOG_BYTES");
    expect(source).toContain("rotate_log_if_needed");
    expect(source).toContain("bounded_log_line");
  });
});
