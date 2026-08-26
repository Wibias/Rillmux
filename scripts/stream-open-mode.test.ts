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

describe("stream opening modes", () => {
  test("does not equate disabling seamless switching with multistream", () => {
    const source = readFileSync("src/lib/streaming/store.ts", "utf8");
    const body = functionBody(source, "watchStream: async");

    expect(body).not.toContain("!settings.streaming.seamlessSwitch");
    expect(body).toContain('mode === "multistream"');
    expect(body).toContain('mode === "seamless"');
  });

  test("independent sessions are not fed into the multistream layout path", () => {
    const source = readFileSync("src/lib/streaming/store.ts", "utf8");
    const body = functionBody(source, "function scheduleLayoutAfterReady(");

    expect(body).toContain("layoutChannels()");
    expect(body).not.toContain("orderedChannels()");
  });
});
