import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

async function readRepo(path: string): Promise<string> {
  return readFile(new URL(`../${path}`, import.meta.url), "utf8");
}

describe("release build hygiene", () => {
  it("does not require unconditional mutable Tauri context in release builds", async () => {
    const source = await readRepo("src-tauri/src/lib.rs");

    expect(source).not.toContain("let mut ctx = tauri::generate_context!();");
    expect(source).toContain("let ctx = tauri::generate_context!();");
    expect(source).toMatch(/#\[cfg\(all\(windows, debug_assertions\)\)\][\s\S]*let mut ctx = ctx;/);
  });

  it("fails CI on release-profile compiler warnings", async () => {
    const source = await readRepo(".github/workflows/ci.yml");

    expect(source).toContain("Cargo check release warnings");
    expect(source).toContain("RUSTFLAGS: -D warnings");
    expect(source).toContain("cargo check --release");
  });

  it("runs React Doctor as a CI gate on frontend changes", async () => {
    const source = await readRepo(".github/workflows/ci.yml");
    const pkg = await readRepo("package.json");

    expect(pkg).toContain("react-doctor@0.9.12");
    expect(pkg).toContain("--scope full");
    expect(pkg).toContain("--blocking warning");
    expect(source).toContain("run: npm run doctor");
    expect(source).toContain("needs: [changes, frontend, rust, react-doctor]");
  });

  it("keeps the actual Tauri release build warning-fatal", async () => {
    const source = await readRepo(".github/workflows/release.yml");

    const buildStep = source.slice(source.indexOf("- name: Build Tauri (NSIS + MSI)"));
    expect(buildStep).toContain("RUSTFLAGS: -D warnings");
  });

  it("does not reference secrets in workflow if conditions", async () => {
    const source = await readRepo(".github/workflows/release.yml");

    expect(source).toContain("name: Release");
    expect(source).not.toMatch(/if:.*secrets\./);
    expect(source).toContain(
      "Authenticode secrets not configured; building unsigned installers",
    );
  });

  it("publishes only updater JSON that Tauri actually produces", async () => {
    const source = await readRepo(".github/workflows/release.yml");

    expect(source).not.toContain("src-tauri/target/release/bundle/nsis/*.json");
    expect(source).not.toContain("src-tauri/target/release/bundle/msi/*.json");
    expect(source).toContain("src-tauri/target/release/bundle/latest.json");
  });
});
