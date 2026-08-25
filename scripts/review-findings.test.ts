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
  });

  test("disabled Channel Points HUD does not poll website auth", () => {
    const source = read("src/components/ChannelPointsHudSync.tsx");
    const syncBody = source.slice(source.indexOf("const sync = async"));
    const disabledGuard = syncBody.indexOf("if (!hudEnabled)");
    const authRead = syncBody.indexOf("getTwitchWebsiteAuthStatus");
    expect(disabledGuard).toBeGreaterThanOrEqual(0);
    expect(authRead).toBeGreaterThan(disabledGuard);
  });

  test("diagnostics bounds the always-on log", () => {
    const source = read("src-tauri/src/diagnostics.rs");
    expect(source).toContain("MAX_LOG_BYTES");
    expect(source).toContain("rotate_log_if_needed");
  });
});
