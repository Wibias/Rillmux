import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

function blockBody(source: string, signature: string): string {
  const start = source.indexOf(signature);
  if (start < 0) throw new Error(`missing block: ${signature}`);
  const open = source.indexOf("{", start);
  if (open < 0) throw new Error(`missing block body: ${signature}`);
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  throw new Error(`unterminated block: ${signature}`);
}

describe("monitor picker divider relocation", () => {
  test("queues grip sync immediately after dispatching the selected monitor apply", () => {
    const source = readFileSync("src-tauri/src/dock.rs", "utf8");
    const mouseDown = blockBody(source, "WM_LBUTTONDOWN => {");
    const body = blockBody(mouseDown, "if let GripKind::Identify(idx) = kind {");

    const raise = body.indexOf("request_raise_after_apply();");
    const apply = body.indexOf("run_apply();");
    const sync = body.indexOf("post_cmd(DockCmd::Sync);");

    expect(raise).toBeGreaterThanOrEqual(0);
    expect(apply).toBeGreaterThan(raise);
    expect(sync).toBeGreaterThan(apply);
  });

  test("dispatches monitor-change native placement off the grip window-proc thread", () => {
    const source = readFileSync("src-tauri/src/streaming/dock.rs", "utf8");
    const body = blockBody(source, "fn apply_dock_layout_cb()");

    const monitorApply = body.indexOf("crate::dock::take_raise_after_apply()");
    const spawn = body.indexOf("thread::spawn");
    const asyncApply = body.indexOf("apply_dock_layout_inner(true)");
    const dragApply = body.indexOf("apply_dock_layout_inner(false)");

    expect(monitorApply).toBeGreaterThanOrEqual(0);
    expect(spawn).toBeGreaterThan(monitorApply);
    expect(asyncApply).toBeGreaterThan(spawn);
    expect(dragApply).toBeGreaterThan(asyncApply);
  });

  test("preserves raise-after-monitor-move semantics inside the worker apply", () => {
    const source = readFileSync("src-tauri/src/streaming/dock.rs", "utf8");
    const body = blockBody(source, "fn apply_dock_layout_inner(raise_after_apply: bool)");

    expect(body).toContain("if raise_after_apply");
    expect(body).toContain("raise_dock_windows(&cfg.channels, cfg.reserve_chat);");
  });

  test("does not restack Channel Points HUDs on the grip-thread divider drag", () => {
    const source = readFileSync("src-tauri/src/streaming/dock.rs", "utf8");
    const body = blockBody(source, "fn apply_dock_layout_inner(raise_after_apply: bool)");

    expect(body).not.toContain("restack_all_points_huds");
  });
});
