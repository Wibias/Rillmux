import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

function effectCleanup(source: string): string {
  const timer = source.indexOf("const timer = window.setInterval(() => void sync(), 1000)");
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
});
