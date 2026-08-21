/**
 * Decide the git tag for a Release workflow run and whether to publish.
 *
 * Usage: node scripts/resolve-release-tag.mjs
 * Writes `tag` and `publish` to $GITHUB_OUTPUT when that file is set.
 */
import { appendFileSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/**
 * @param {string} toml
 * @returns {string}
 */
export function parseCargoPackageVersion(toml) {
  const section = toml.split(/^\[/m).find((block) => block.startsWith("package]"));
  const match = section && /^version\s*=\s*"([^"]+)"/m.exec(section);
  if (!match) {
    throw new Error("Cargo.toml [package] is missing a version.");
  }
  return match[1];
}

/**
 * @param {{
 *   packageVersion: string,
 *   tauriVersion: string,
 *   cargoVersion: string,
 *   githubRef: string,
 *   githubRefName: string,
 *   eventName: string,
 * }} input
 * @returns {{ tag: string, publish: boolean }}
 */
export function resolveReleaseTag(input) {
  const {
    packageVersion,
    tauriVersion,
    cargoVersion,
    githubRef,
    githubRefName,
    eventName,
  } = input;
  if (packageVersion !== tauriVersion) {
    throw new Error(
      `package.json version (${packageVersion}) does not match tauri.conf.json (${tauriVersion}).`,
    );
  }
  if (packageVersion !== cargoVersion) {
    throw new Error(
      `package.json version (${packageVersion}) does not match Cargo.toml (${cargoVersion}).`,
    );
  }

  const tag = `v${packageVersion}`;
  if (githubRef.startsWith("refs/tags/")) {
    if (githubRefName !== tag) {
      throw new Error(
        `Pushed tag ${githubRefName} does not match package version ${tag}.`,
      );
    }
    return { tag, publish: true };
  }

  const onDefaultBranch =
    githubRef === "refs/heads/main" || githubRef === "refs/heads/master";
  const publish = eventName === "workflow_dispatch" && onDefaultBranch;
  return { tag, publish };
}

function versionsFromRepo(root) {
  const pkg = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
  const tauri = JSON.parse(
    readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"),
  );
  const cargo = parseCargoPackageVersion(
    readFileSync(path.join(root, "src-tauri", "Cargo.toml"), "utf8"),
  );
  return {
    packageVersion: pkg.version,
    tauriVersion: tauri.version,
    cargoVersion: cargo,
  };
}

function isCli() {
  const entry = process.argv[1];
  if (!entry) return false;
  return pathToFileURL(path.resolve(entry)).href === import.meta.url;
}

function main() {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const result = resolveReleaseTag({
    ...versionsFromRepo(root),
    githubRef: process.env.GITHUB_REF ?? "",
    githubRefName: process.env.GITHUB_REF_NAME ?? "",
    eventName: process.env.GITHUB_EVENT_NAME ?? "",
  });
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(
      process.env.GITHUB_OUTPUT,
      `tag=${result.tag}\npublish=${result.publish}\n`,
    );
  }
  console.log(`Release tag ${result.tag}; publish=${result.publish}`);
}

if (isCli()) {
  try {
    main();
  } catch (err) {
    console.error(err instanceof Error ? err.message : err);
    process.exit(1);
  }
}
