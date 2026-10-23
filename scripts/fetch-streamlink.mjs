/**
 * Downloads a pinned Streamlink Windows portable build into
 * src-tauri/resources/streamlink for NSIS/MSI packaging.
 *
 * The SHA-256 of the zip is pinned together with the tag: this binary is
 * bundled into signed installers, so a replaced/ poisoned release asset must
 * fail the build instead of shipping to users.
 *
 * Usage: node scripts/fetch-streamlink.mjs [tag] [sha256]
 * Default tag: 8.4.0-1
 *
 * When bumping the tag, update PINNED_SHA256 below:
 *   curl -sL -o sl.zip <asset-url> && sha256sum sl.zip
 */
import { cp, mkdir, readFile, readdir, rename, rm, stat, writeFile } from "node:fs/promises";
import { createWriteStream, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { pipeline } from "node:stream/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const PINNED = {
  "8.4.0-1":
    "a8d3bd2b409e6d1b1f7a0e2a5c0cbfba619775e475da3f31285af08d680fb71c",
};

const version = process.argv[2] ?? "8.4.0-1";
const expectedHash = process.argv[3] ?? PINNED[version];
if (!expectedHash) {
  console.error(
    `No pinned SHA-256 for tag ${version}. Pass it as second argument and add it to PINNED in this script.`,
  );
  process.exit(1);
}
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outDir = path.join(root, "src-tauri", "resources", "streamlink");
const readmePath = path.join(outDir, "README.md");
const readme = await readFile(readmePath, "utf8").catch(() => null);
const assetUrl = `https://github.com/streamlink/windows-builds/releases/download/${version}/streamlink-${version}-py314-x86_64.zip`;

console.log(`Fetching Streamlink ${version}…`);
console.log(assetUrl);

const res = await fetch(assetUrl, { redirect: "follow" });
if (!res.ok) {
  console.error(`Download failed: ${res.status} ${res.statusText}`);
  console.error(
    "Pin a valid tag from https://github.com/streamlink/windows-builds/releases",
  );
  process.exit(1);
}

await rm(outDir, { recursive: true, force: true });
await mkdir(outDir, { recursive: true });

const zipPath = path.join(outDir, "streamlink.zip");
await pipeline(res.body, createWriteStream(zipPath));

// Verify integrity BEFORE extracting anything.
// Plain for-await instead of stream.pipeline with a generator destination:
// that pattern never settles on Node 22 ("unsettled top-level await").
const hash = createHash("sha256");
const { createReadStream } = await import("node:fs");
for await (const chunk of createReadStream(zipPath)) {
  hash.update(chunk);
}
const actualHash = hash.digest("hex");
if (actualHash !== expectedHash) {
  await rm(zipPath, { force: true });
  console.error(`SHA-256 mismatch for Streamlink ${version}!`);
  console.error(`  expected: ${expectedHash}`);
  console.error(`  actual:   ${actualHash}`);
  console.error(
    "Refusing to bundle an unverified binary. If the release was re-published, verify it manually and update the pin.",
  );
  process.exit(1);
}
console.log(`SHA-256 verified: ${actualHash}`);

try {
  // Prefer Windows' own bsdtar (auto-detects zip); a GNU tar found earlier
  // on PATH (e.g. Git-Bash) cannot read zip archives. Relative paths + cwd:
  // bsdtar misparses "C:\…" as host:file and lacks --force-local.
  const systemTar = "C:\\Windows\\System32\\tar.exe";
  const tarExe =
    process.platform === "win32" && existsSync(systemTar) ? systemTar : "tar";
  execFileSync(tarExe, ["-xf", "streamlink.zip", "-C", "."], {
    cwd: outDir,
    stdio: "inherit",
  });
} catch {
  console.error("Failed to extract with tar. Install tar or extract manually.");
  process.exit(1);
}

await rm(zipPath, { force: true });

async function isFile(target) {
  try {
    return (await stat(target)).isFile();
  } catch {
    return false;
  }
}

async function isDir(target) {
  try {
    return (await stat(target)).isDirectory();
  } catch {
    return false;
  }
}

/**
 * The upstream Windows build has this layout:
 *
 *   streamlink-<version>/
 *     bin/streamlink.exe
 *     bin/streamlinkw.exe
 *     Python/python.exe
 *     pkgs/
 *     ffmpeg/ffmpeg.exe
 *
 * The launcher resolves `..\Python\python.exe` relative to itself, so the
 * bin/ directory must stay one level below Python and pkgs. Flattening only
 * `bin/` (the previous behaviour) left the launcher unable to find Python on
 * a clean install. Flatten the whole payload root instead and keep the
 * upstream layout intact.
 */
async function findPayloadRoot(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const full = path.join(dir, entry.name);
    if (
      (await isFile(path.join(full, "bin", "streamlink.exe"))) &&
      (await isFile(path.join(full, "Python", "python.exe"))) &&
      (await isDir(path.join(full, "pkgs")))
    ) {
      return full;
    }
    const nested = await findPayloadRoot(full);
    if (nested) return nested;
  }
  return null;
}

async function dirSize(dir) {
  let total = 0;
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      total += await dirSize(full);
    } else if (entry.isFile()) {
      total += (await stat(full)).size;
    }
  }
  return total;
}

async function findNamedDirs(dir, name) {
  const found = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const full = path.join(dir, entry.name);
    if (entry.name.toLowerCase() === name) {
      found.push(full);
    } else {
      found.push(...(await findNamedDirs(full, name)));
    }
  }
  return found;
}

const payloadRoot = await findPayloadRoot(outDir);
if (!payloadRoot) {
  console.error("Streamlink payload root (bin + Python + pkgs) not found after extract");
  process.exit(1);
}

const ffmpegDirs = await findNamedDirs(outDir, "ffmpeg");
let ffmpegBytes = 0;
for (const ffmpegDir of ffmpegDirs) {
  ffmpegBytes += await dirSize(ffmpegDir);
  await rm(ffmpegDir, { recursive: true, force: true });
}

if (path.resolve(payloadRoot) !== path.resolve(outDir)) {
  console.log(`Flattening payload from ${payloadRoot}`);
  for (const entry of await readdir(payloadRoot, { withFileTypes: true })) {
    if (entry.name.toLowerCase() === "ffmpeg") continue;
    const from = path.join(payloadRoot, entry.name);
    const to = path.join(outDir, entry.name);
    if (existsSync(to)) {
      await rm(to, { recursive: true, force: true });
    }
    // Node's rename() of extracted directories intermittently fails with
    // EPERM on Windows (AV/indexer holding a handle). Copy then remove.
    if (entry.isDirectory()) {
      await cp(from, to, { recursive: true });
      await rm(from, { recursive: true, force: true });
    } else {
      await rename(from, to);
    }
  }
  await rm(payloadRoot, { recursive: true, force: true });
}

for (const entry of await readdir(outDir, { withFileTypes: true })) {
  if (entry.isFile() && /^ffmpeg\.exe$/i.test(entry.name)) {
    const full = path.join(outDir, entry.name);
    ffmpegBytes += (await stat(full)).size;
    await rm(full, { force: true });
  }
}

let pycacheBytes = 0;
for (const pycacheDir of await findNamedDirs(outDir, "__pycache__")) {
  pycacheBytes += await dirSize(pycacheDir);
  await rm(pycacheDir, { recursive: true, force: true });
}

const finalLauncher = path.join(outDir, "bin", "streamlinkw.exe");
const finalExe = path.join(outDir, "bin", "streamlink.exe");
const finalPython = path.join(outDir, "Python", "python.exe");
if (
  !(await isFile(finalLauncher)) ||
  !(await isFile(finalExe)) ||
  !(await isFile(finalPython)) ||
  !(await isDir(path.join(outDir, "pkgs")))
) {
  console.error("Bundled Streamlink layout is incomplete (expected bin/, Python/, pkgs/)");
  process.exit(1);
}

const payloadBytes = await dirSize(outDir);
const maxPayloadBytes = 120 * 1024 * 1024;
if (payloadBytes > maxPayloadBytes) {
  console.error(
    `Bundled Streamlink payload is ${(payloadBytes / 1024 / 1024).toFixed(1)} MiB; limit is 120 MiB`,
  );
  process.exit(1);
}

await writeFile(path.join(outDir, "VERSION"), `${version}\n`, "utf8");
if (readme !== null) {
  await writeFile(readmePath, readme, "utf8");
}
console.log(
  `Streamlink ${version} ready at ${finalLauncher} ` +
    `(${(payloadBytes / 1024 / 1024).toFixed(1)} MiB payload, ` +
    `${(ffmpegBytes / 1024 / 1024).toFixed(1)} MiB ffmpeg removed, ` +
    `${(pycacheBytes / 1024 / 1024).toFixed(1)} MiB bytecode cache removed)`,
);
