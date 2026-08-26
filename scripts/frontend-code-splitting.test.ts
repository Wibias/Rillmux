import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

async function readRepo(path: string): Promise<string> {
  return readFile(new URL(`../${path}`, import.meta.url), "utf8");
}

describe("frontend code splitting", () => {
  it("uses Rolldown chunk groups for stable vendor boundaries", async () => {
    const source = await readRepo("vite.config.ts");

    expect(source).toContain("rolldownOptions");
    expect(source).toContain("codeSplitting");
    expect(source).toContain('name: "react-core"');
    expect(source).toContain('name: "tauri"');
    expect(source).toContain('name: "i18n"');
  });

  it("loads route-heavy pages lazily instead of from the initial app graph", async () => {
    const source = await readRepo("src/App.tsx");

    expect(source).toContain("lazy(() =>");
    expect(source).toContain("<Suspense");
    expect(source).not.toMatch(/from ["']\.\/pages\/BrowsePages["']/);
    expect(source).not.toMatch(/from ["']\.\/pages\/BrowseExtraPages["']/);
    expect(source).not.toMatch(/from ["']\.\/pages\/SettingsPage["']/);
    expect(source).not.toMatch(/from ["']\.\/pages\/MultistreamPage["']/);
  });

  it("does not dynamically import opener when it is already part of the app graph", async () => {
    const source = await readRepo("src/components/UpdateDialog.tsx");

    expect(source).not.toContain('import("@tauri-apps/plugin-opener")');
    expect(source).toMatch(/import\s+\{\s*openUrl\s*\}\s+from\s+["']@tauri-apps\/plugin-opener["']/);
  });
});
