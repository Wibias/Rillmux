import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

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
});
