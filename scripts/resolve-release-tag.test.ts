import { describe, expect, it } from "vitest";
import {
  parseCargoPackageVersion,
  resolveReleaseTag,
} from "./resolve-release-tag.mjs";

const versions = {
  packageVersion: "0.5.0",
  tauriVersion: "0.5.0",
  cargoVersion: "0.5.0",
};

describe("parseCargoPackageVersion", () => {
  it("reads the [package] version and ignores dependency versions", () => {
    const toml = [
      "[package]",
      'name = "rillmux"',
      'version = "0.5.0"',
      "",
      "[dependencies]",
      'serde = { version = "1" }',
    ].join("\n");
    expect(parseCargoPackageVersion(toml)).toBe("0.5.0");
  });

  it("throws when [package] has no version", () => {
    expect(() => parseCargoPackageVersion('[package]\nname = "rillmux"\n')).toThrow(
      /Cargo.toml/,
    );
  });
});

describe("resolveReleaseTag", () => {
  it("publishes v{version} on workflow_dispatch from main", () => {
    expect(
      resolveReleaseTag({
        ...versions,
        githubRef: "refs/heads/main",
        githubRefName: "main",
        eventName: "workflow_dispatch",
      }),
    ).toEqual({ tag: "v0.5.0", publish: true });
  });

  it("publishes v{version} on workflow_dispatch from master", () => {
    expect(
      resolveReleaseTag({
        ...versions,
        githubRef: "refs/heads/master",
        githubRefName: "master",
        eventName: "workflow_dispatch",
      }),
    ).toEqual({ tag: "v0.5.0", publish: true });
  });

  it("builds but does not publish on workflow_dispatch from a feature branch", () => {
    expect(
      resolveReleaseTag({
        ...versions,
        githubRef: "refs/heads/feature/foo",
        githubRefName: "feature/foo",
        eventName: "workflow_dispatch",
      }),
    ).toEqual({ tag: "v0.5.0", publish: false });
  });

  it("publishes the pushed tag when it matches package.json", () => {
    expect(
      resolveReleaseTag({
        ...versions,
        githubRef: "refs/tags/v0.5.0",
        githubRefName: "v0.5.0",
        eventName: "push",
      }),
    ).toEqual({ tag: "v0.5.0", publish: true });
  });

  it("rejects a tag that does not match the app version", () => {
    expect(() =>
      resolveReleaseTag({
        ...versions,
        githubRef: "refs/tags/v0.6.0",
        githubRefName: "v0.6.0",
        eventName: "push",
      }),
    ).toThrow(/v0\.6\.0.*v0\.5\.0/);
  });

  it("rejects mismatched package.json and tauri.conf.json versions", () => {
    expect(() =>
      resolveReleaseTag({
        ...versions,
        tauriVersion: "0.4.0",
        githubRef: "refs/heads/main",
        githubRefName: "main",
        eventName: "workflow_dispatch",
      }),
    ).toThrow(/package\.json.*tauri\.conf\.json/);
  });

  it("rejects mismatched package.json and Cargo.toml versions", () => {
    expect(() =>
      resolveReleaseTag({
        ...versions,
        cargoVersion: "0.4.0",
        githubRef: "refs/heads/main",
        githubRefName: "main",
        eventName: "workflow_dispatch",
      }),
    ).toThrow(/package\.json.*Cargo\.toml/);
  });
});
