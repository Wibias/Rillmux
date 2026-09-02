import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  readSkippedUpdateVersion,
  shouldPromptAppUpdate,
  writeSkippedUpdateVersion,
} from "./prompt";

describe("shouldPromptAppUpdate", () => {
  it("skips the auto-update dialog in Vite / tauri:dev", () => {
    expect(
      shouldPromptAppUpdate({
        viteDev: true,
        currentVersion: "0.5.6",
        availableVersion: "0.5.8",
      }),
    ).toBe(false);
  });

  it("offers a newer production release", () => {
    expect(
      shouldPromptAppUpdate({
        viteDev: false,
        currentVersion: "0.5.6",
        availableVersion: "0.5.8",
      }),
    ).toBe(true);
  });

  it("does not offer the version the app is already running", () => {
    expect(
      shouldPromptAppUpdate({
        viteDev: false,
        currentVersion: "0.5.8",
        availableVersion: "0.5.8",
      }),
    ).toBe(false);
    expect(
      shouldPromptAppUpdate({
        viteDev: false,
        currentVersion: "v0.5.8",
        availableVersion: "0.5.8",
      }),
    ).toBe(false);
  });

  it("does not re-prompt a version the user already dismissed", () => {
    expect(
      shouldPromptAppUpdate({
        viteDev: false,
        currentVersion: "0.5.6",
        availableVersion: "0.5.8",
        skippedVersion: "0.5.8",
      }),
    ).toBe(false);
  });

  it("still offers a newer release after a previous skip", () => {
    expect(
      shouldPromptAppUpdate({
        viteDev: false,
        currentVersion: "0.5.6",
        availableVersion: "0.5.9",
        skippedVersion: "0.5.8",
      }),
    ).toBe(true);
  });
});

describe("skipped update version storage", () => {
  it("round-trips the skipped version through web storage", () => {
    const memory = new Map<string, string>();
    const storage = {
      getItem: (key: string) => memory.get(key) ?? null,
      setItem: (key: string, value: string) => {
        memory.set(key, value);
      },
    };

    expect(readSkippedUpdateVersion(storage)).toBeNull();
    writeSkippedUpdateVersion(storage, "0.5.8");
    expect(readSkippedUpdateVersion(storage)).toBe("0.5.8");
  });
});

describe("update prompt call sites", () => {
  it("gates the startup dialog and About check on shouldPromptAppUpdate", () => {
    const banner = readFileSync(
      new URL("../../components/UpdateBanner.tsx", import.meta.url),
      "utf8",
    );
    const about = readFileSync(
      new URL("../../components/DeepLinkAndUpdaterBootstrap.tsx", import.meta.url),
      "utf8",
    );
    expect(banner).toContain("shouldPromptAppUpdate");
    expect(banner).toContain("import.meta.env.DEV");
    expect(banner).toContain("writeSkippedUpdateVersion");
    expect(about).toContain("shouldPromptAppUpdate");
    expect(about).toContain("import.meta.env.DEV");
  });
});
