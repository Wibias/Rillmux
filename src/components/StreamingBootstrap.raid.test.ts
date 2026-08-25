import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(
  new URL("./StreamingBootstrap.tsx", import.meta.url),
  "utf8",
);

describe("raid EventSub lifecycle", () => {
  it("reconciles EventSub after settings hydration and when followRaids changes", () => {
    expect(source).toContain("syncEventSub");
    expect(source).toMatch(/const followRaids = useSettingsStore/);
    expect(source).toMatch(
      /if \(!settingsHydrated\) return;[\s\S]*syncEventSub\(\);[\s\S]*\[settingsHydrated, followRaids\]/,
    );
  });
});
