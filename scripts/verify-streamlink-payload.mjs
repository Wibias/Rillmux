import { readdir, stat } from "node:fs/promises";
import { spawn, spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const payload = path.join(root, "src-tauri", "resources", "streamlink");
const binDir = path.join(payload, "bin");
const streamlink = path.join(binDir, "streamlink.exe");
const streamlinkw = path.join(binDir, "streamlinkw.exe");
const python = path.join(payload, "Python", "python.exe");
const pkgs = path.join(payload, "pkgs");
const online = process.argv.includes("--online");
const urlFlag = process.argv.indexOf("--url");
const channel =
  urlFlag !== -1 && process.argv[urlFlag + 1]
    ? process.argv[urlFlag + 1]
    : process.env.RILLMUX_STREAMLINK_SMOKE_URL || "twitch.tv/eslcs";

function fail(message) {
  console.error(`verify-streamlink-payload: ${message}`);
  process.exit(1);
}

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

async function findFfmpeg(dir) {
  const found = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name.toLowerCase() === "ffmpeg") {
        found.push(full);
      } else {
        found.push(...(await findFfmpeg(full)));
      }
    } else if (/^ffmpeg\.exe$/i.test(entry.name)) {
      found.push(full);
    }
  }
  return found;
}

async function runVersion(exe, label) {
  const result = spawnSync(exe, ["--version"], {
    cwd: binDir,
    encoding: "utf8",
    timeout: 15_000,
    env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
  });
  if (result.error) fail(`cannot start bundled ${label}: ${result.error.message}`);
  if (result.status !== 0) {
    fail(
      `bundled ${label} --version exited ${result.status}: ${(
        result.stderr || result.stdout || ""
      ).trim()}`,
    );
  }
  const output = `${result.stdout || ""}${result.stderr || ""}`;
  if (!/streamlink/i.test(output)) {
    fail(`unexpected bundled ${label} --version output: ${output.trim()}`);
  }
  console.log(`bundled ${label} starts: ${output.trim()}`);
}

async function runOnline() {
  const port = 19_500 + Math.floor(Math.random() * 400);
  const args = [
    "-l",
    "info",
    "--webbrowser",
    "no",
    "--player-external-http",
    "--player-external-http-interface",
    "127.0.0.1",
    "--player-external-http-port",
    String(port),
    "--player-external-http-continuous",
    "no",
    "--stream-segment-threads",
    "3",
    "--hls-segment-stream-data",
    channel,
    "best",
  ];
  console.log(`online smoke: ${channel} (exact app playback arguments)`);
  await new Promise((resolve, reject) => {
    const child = spawn(streamlink, args, {
      cwd: binDir,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
      env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
    });
    let output = "";
    let settled = false;
    let timer;
    const finish = (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      try {
        child.kill();
      } catch {}
      if (error) reject(error);
      else resolve();
    };
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      output += chunk;
      if (output.includes("Starting server") || output.includes("http://127.0.0.1:")) {
        finish();
      }
    });
    child.stderr.on("data", (chunk) => {
      output += chunk;
    });
    child.on("error", (error) => finish(error));
    child.on("exit", (code) => {
      if (settled) return;
      finish(
        new Error(
          `streamlink exited before serving (code ${code}): ${output.trim()}`,
        ),
      );
    });
    timer = setTimeout(() => {
      finish(new Error(`timed out waiting for Streamlink server: ${output.trim()}`));
    }, 25_000);
  });
  console.log("online smoke: Streamlink started the external HTTP server");
}

const missing = [];
for (const [label, target] of [
  ["bin/streamlink.exe", streamlink],
  ["bin/streamlinkw.exe", streamlinkw],
  ["Python/python.exe", python],
  ["pkgs/", pkgs],
  ["pkgs/streamlink/__init__.py", path.join(pkgs, "streamlink", "__init__.py")],
]) {
  const ok = label.endsWith("/") ? await isDir(target) : await isFile(target);
  if (!ok) missing.push(label);
}
if (missing.length) {
  fail(`bundled Streamlink is incomplete; missing ${missing.join(", ")}`);
}

const ffmpeg = await findFfmpeg(payload);
if (ffmpeg.length) {
  fail(`bundled Streamlink still contains ffmpeg: ${ffmpeg.join(", ")}`);
}

const bytes = await dirSize(payload);
const maxBytes = 120 * 1024 * 1024;
if (bytes > maxBytes) {
  fail(`bundled Streamlink payload is ${(bytes / 1024 / 1024).toFixed(1)} MiB (limit 120 MiB)`);
}
console.log(`bundled Streamlink payload: ${(bytes / 1024 / 1024).toFixed(1)} MiB, no ffmpeg`);

await runVersion(streamlink, "streamlink");
await runVersion(streamlinkw, "streamlinkw");
if (online) await runOnline();

console.log("verify-streamlink-payload: ok");
