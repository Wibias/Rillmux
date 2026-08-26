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
  test("moves existing grips before applying the selected monitor layout", () => {
    const source = readFileSync("src-tauri/src/dock.rs", "utf8");
    const body = blockBody(source, "if let GripKind::Identify(idx) = kind {");

    expect(body).toContain("reposition_all_grips_static();");
    expect(body.indexOf("reposition_all_grips_static();")).toBeLessThan(
      body.indexOf("thread::spawn"),
    );
  });

  test("does not run the cross-process dock apply synchronously in the grip window proc", () => {
    const source = readFileSync("src-tauri/src/dock.rs", "utf8");
    const body = blockBody(source, "if let GripKind::Identify(idx) = kind {");

    const spawn = body.indexOf("thread::spawn");
    const apply = body.indexOf("run_apply();");
    const sync = body.indexOf("post_cmd(DockCmd::Sync);");

    expect(spawn).toBeGreaterThanOrEqual(0);
    expect(apply).toBeGreaterThan(spawn);
    expect(sync).toBeGreaterThan(apply);
  });
});
