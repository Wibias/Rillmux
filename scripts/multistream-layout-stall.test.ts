import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

function functionBody(source: string, signature: string): string {
  const start = source.indexOf(signature);
  if (start < 0) throw new Error(`missing function: ${signature}`);
  const open = source.indexOf("{", start);
  if (open < 0) throw new Error(`missing function body: ${signature}`);
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  throw new Error(`unterminated function body: ${signature}`);
}

describe("multistream layout synchronization", () => {
  test("does not route ordinary layout refreshes through immediate-apply dock setters", () => {
    const source = readFileSync("src-tauri/src/streaming/dock.rs", "utf8");
    const body = functionBody(source, "pub fn layout_watching(");

    // `set_chat_fraction` is the interactive setter: it synchronously calls
    // run_apply(), which performs cross-process Win32 MoveWindow work. A normal
    // session/layout refresh must only synchronize config and let the existing
    // background retile path do the actual placement.
    expect(body).not.toContain("crate::dock::set_chat_fraction");
  });
});
