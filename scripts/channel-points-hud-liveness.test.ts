import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

function effectCleanup(source: string): string {
  const timer = source.indexOf(
    "const timer = window.setInterval(() => kick.kick(), 1000)",
  );
  if (timer < 0) throw new Error("missing HUD sync timer");
  const start = source.indexOf("return () => {", timer);
  if (start < 0) throw new Error("missing HUD sync cleanup");
  const end = source.indexOf("};", start);
  if (end < 0) throw new Error("unterminated HUD sync cleanup");
  return source.slice(start, end + 2);
}

describe("Channel Points HUD liveness", () => {
  test("keeps the last valid player geometry across a transient placement miss", () => {
    const source = readFileSync("src/components/ChannelPointsHud.tsx", "utf8");

    expect(source).not.toContain("setHost(next?.player ?? null)");
    expect(source).not.toContain("if (!next?.player) setCatalogOpen(false)");
  });

  test("does not close a running stream HUD after only three placement misses", () => {
    const source = readFileSync("src/components/ChannelPointsHudSync.tsx", "utf8");

    expect(source).not.toContain("const PLAYER_MISS_GRACE = 3");
    expect(source).not.toContain("misses >= PLAYER_MISS_GRACE");
  });

  test("does not close every HUD when the running channel set changes", () => {
    const source = readFileSync("src/components/ChannelPointsHudSync.tsx", "utf8");
    const cleanup = effectCleanup(source);

    expect(cleanup).not.toContain("closeHud(");
    expect(cleanup).not.toContain("for (const channel of wantedRef.current)");
  });

  test("hides a minimized-player HUD immediately instead of waiting out the miss grace", () => {
    const pass = readFileSync("src/lib/streaming/hudSyncPass.ts", "utf8");
    expect(pass).toContain("hudKeepOnPlayerMiss");
    expect(pass).toContain("nextPlace?.hidden");
    expect(pass).toContain('hudKeepOnPlayerMiss("missing"');
    const hud = readFileSync("src/components/ChannelPointsHud.tsx", "utf8");
    expect(hud).toContain("if (next.hidden)");
    expect(hud).toContain("setHostHidden(true)");
    expect(hud).toContain("if (!overlay || hostHidden) return");
  });

  test("does not shrink an existing HUD back to the chip over an open catalog", () => {
    const source = readFileSync("src/components/ChannelPointsHudSync.tsx", "utf8");
    const start = source.indexOf("async function ensureHud(");
    const open = source.indexOf("{", start);
    let depth = 0;
    let end = open;
    for (let i = open; i < source.length; i += 1) {
      if (source[i] === "{") depth += 1;
      if (source[i] === "}") {
        depth -= 1;
        if (depth === 0) {
          end = i;
          break;
        }
      }
    }
    const body = source.slice(open, end + 1);
    const existingStart = body.indexOf("if (existing)");
    const existingOpen = body.indexOf("{", existingStart);
    let existingDepth = 0;
    let existingEnd = existingOpen;
    for (let i = existingOpen; i < body.length; i += 1) {
      if (body[i] === "{") existingDepth += 1;
      if (body[i] === "}") {
        existingDepth -= 1;
        if (existingDepth === 0) {
          existingEnd = i;
          break;
        }
      }
    }
    const existing = body.slice(existingStart, existingEnd + 1);

    expect(existing).not.toContain("placeHud(");
    expect(existing).not.toContain("forcePlace");
  });
});
