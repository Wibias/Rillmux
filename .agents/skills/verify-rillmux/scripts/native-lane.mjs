/**
 * Native / Tauri lane for verify-rillmux. Does not replace the Vite+CDP web lane.
 */
import { execFileSync, spawn } from "node:child_process";
import { closeSync, existsSync, mkdirSync, openSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createConnection } from "node:net";

export const TAURI_DEV_ORIGIN = "http://localhost:1420";
export const TAURI_DEV_PORT = 1420;

function which(names) {
  for (const name of names) {
    try {
      const out = execFileSync("where.exe", [name], {
        encoding: "utf8",
        windowsHide: true,
        stdio: ["ignore", "pipe", "ignore"],
      });
      const first = out
        .split(/\r?\n/)
        .map((line) => line.trim())
        .find(Boolean);
      if (first && existsSync(first)) return first;
    } catch {
      /* not on PATH */
    }
  }
  return null;
}

function versionOf(exe, args = ["--version"]) {
  if (!exe) return null;
  try {
    return execFileSync(exe, args, {
      encoding: "utf8",
      timeout: 15_000,
      windowsHide: true,
    })
      .trim()
      .split(/\r?\n/)[0]
      .slice(0, 200);
  } catch {
    return null;
  }
}

async function portTaken(port) {
  return await new Promise((resolveTaken) => {
    const socket = createConnection({ port, host: "127.0.0.1" });
    socket.once("connect", () => {
      socket.destroy();
      resolveTaken(true);
    });
    socket.once("error", () => resolveTaken(false));
  });
}

export async function probeNativePrerequisites({ projectRoot, runtimeDir, state }) {
  const cargo = which(["cargo.exe", "cargo"]);
  const rustc = which(["rustc.exe", "rustc"]);
  const tauriCli = join(projectRoot, "node_modules", "@tauri-apps", "cli", "tauri.js");
  const streamlink = which(["streamlink.exe", "streamlink"]);
  const mpv = which(["mpv.exe", "mpv"]);
  const chatterino = which(["chatterino.exe", "chatterino"]);
  const portBusy = await portTaken(TAURI_DEV_PORT);
  const owned = Boolean(state?.native?.pid);
  const foreignDevServer = portBusy && !owned;

  const tools = {
    cargo: { found: Boolean(cargo), path: cargo, version: versionOf(cargo) },
    rustc: { found: Boolean(rustc), path: rustc, version: versionOf(rustc) },
    tauriCli: {
      found: existsSync(tauriCli),
      path: existsSync(tauriCli) ? tauriCli : null,
    },
    streamlink: {
      found: Boolean(streamlink),
      path: streamlink,
      version: versionOf(streamlink),
    },
    mpv: { found: Boolean(mpv), path: mpv, version: versionOf(mpv) },
    chatterino: {
      found: Boolean(chatterino),
      path: chatterino,
      version: versionOf(chatterino, ["--version"]),
    },
  };

  const blockers = [];
  if (process.platform !== "win32") {
    blockers.push("Native lane is Windows-only (Rillmux desktop target).");
  }
  if (!tools.cargo.found) blockers.push("cargo not on PATH. Install Rust stable, then retry launch --lane native.");
  if (!tools.rustc.found) blockers.push("rustc not on PATH. Install Rust stable, then retry launch --lane native.");
  if (!tools.tauriCli.found) blockers.push("Tauri CLI missing. Run npm install at the project root.");
  if (foreignDevServer) {
    blockers.push(
      `Port ${TAURI_DEV_PORT} is already in use by a process this verifier did not start. Stop that Vite/tauri:dev session or reuse it only if it is this checkout.`,
    );
  }

  return {
    os: process.platform,
    headSha: null,
    tools,
    port: { id: TAURI_DEV_PORT, busy: portBusy, ownedByVerifier: owned, foreignOccupant: foreignDevServer },
    auth: {
      inspected: false,
      reason: "OS credential store is not read. Presence is inferred only from the signed-out chrome after native launch.",
    },
    isolation: {
      appDataOverride: "APPDATA → .agents/skills/verify-rillmux/.runtime/native-appdata-<runId>",
      webview: "debug tauri:dev uses webview-dev (product), not the installed-app WebView2 folder",
      note: "LOCALAPPDATA is not overridden so system mpv/Chatterino discovery still works.",
    },
    canLaunch: blockers.length === 0,
    blockedReasons: blockers,
    instance: {
      launched: Boolean(state?.native?.pid),
      pid: state?.native?.pid ?? null,
      origin: state?.native?.origin ?? null,
      cdpPort: state?.native?.cdpPort ?? null,
      appDataDir: state?.native?.appDataDir ?? null,
      ready: false,
    },
    runtimeDir,
  };
}

export function seedIsolatedSettings(appDataDir) {
  mkdirSync(appDataDir, { recursive: true });
  const settingsPath = join(appDataDir, "settings.json");
  const body = {
    settings: {
      schemaVersion: 21,
      gui: { onboardingDone: true, closeToTray: false },
      sentryEnabled: false,
    },
  };
  writeFileSync(settingsPath, `${JSON.stringify(body)}\n`);
  return settingsPath;
}

export function nativeDryRun({ cdpPort, appDataDir, logPath }) {
  return {
    dryRun: true,
    lane: "native",
    would: [
      `set APPDATA=${appDataDir}`,
      `set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=${cdpPort} --remote-allow-origins=*`,
      "npx tauri dev --config <runtime overlay with WebView2 remote-debugging-port>",
      `wait ${TAURI_DEV_ORIGIN} then CDP page with hydrated shell and no Vite tauri-guard banner`,
    ],
    logPath,
    webviewLogPath: webviewLogPathFor(logPath),
    isolation: "isolated APPDATA; kill only the recorded npm/tauri PID tree",
  };
}

export function pidsListening(port) {
  try {
    const out = execFileSync("netstat", ["-ano"], {
      encoding: "utf8",
      windowsHide: true,
    });
    const pids = new Set();
    for (const line of out.split(/\r?\n/)) {
      if (!line.includes("LISTENING")) continue;
      if (!line.includes(`:${port}`) && !line.includes(`:${port} `)) continue;
      const pid = line.trim().split(/\s+/).pop();
      if (/^\d+$/.test(pid)) pids.add(Number(pid));
    }
    return [...pids];
  } catch {
    return [];
  }
}

export function debugRillmuxPids(projectRoot) {
  const marker = join(projectRoot, "src-tauri", "target", "debug", "rillmux.exe");
  const escaped = marker.replace(/'/g, "''");
  try {
    const out = execFileSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        `Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq '${escaped}' } | ForEach-Object { $_.ProcessId }`,
      ],
      { encoding: "utf8", windowsHide: true, timeout: 20_000 },
    );
    return out
      .split(/\r?\n/)
      .map((line) => Number(line.trim()))
      .filter((pid) => Number.isInteger(pid) && pid > 0);
  } catch {
    return [];
  }
}

/** Chromium writes its own diagnostics here (window-class teardown, GPU, ...).
 * The Tauri CLI's stdio stays silent under a redirected pipe, so a receipt that
 * needs WebView-side messages reads this file instead. */
export function webviewLogPathFor(logPath) {
  return String(logPath).replace(/\.tauri-dev\.log$/, ".webview.log");
}

export function writeCdpOverlay(overlayPath, cdpPort, webviewLogPath) {
  const logFlag = webviewLogPath
    ? " --enable-logging --log-file=" +
      (webviewLogPath.includes(" ") ? '"' + webviewLogPath + '"' : webviewLogPath)
    : "";
  const overlay = {
    app: {
      windows: [
        {
          additionalBrowserArgs: `--remote-debugging-port=${cdpPort} --remote-allow-origins=*${logFlag}`,
        },
      ],
    },
  };
  writeFileSync(overlayPath, `${JSON.stringify(overlay, null, 2)}\n`);
  return overlayPath;
}

export function spawnTauriDev({
  projectRoot,
  appDataDir,
  logPath,
  overlayPath,
  cdpPort,
}) {
  mkdirSync(appDataDir, { recursive: true });
  const quoted = overlayPath.replace(/"/g, '\\"');
  const cmdLine = `npx tauri dev --config "${quoted}"`;
  const webviewLogPath = webviewLogPathFor(logPath);
  const browserArgs =
    `--remote-debugging-port=${cdpPort} --remote-allow-origins=*` +
    " --enable-logging --log-file=" +
    (webviewLogPath.includes(" ") ? '"' + webviewLogPath + '"' : webviewLogPath);
  const logFd = openSync(logPath, "w");
  const env = {
    ...process.env,
    APPDATA: appDataDir,
    RILLMUX_DEBUG: process.env.RILLMUX_DEBUG || "0",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: browserArgs,
  };
  const child =
    process.platform === "win32"
      ? spawn(process.env.ComSpec || "cmd.exe", ["/d", "/s", "/c", cmdLine], {
          cwd: projectRoot,
          env,
          stdio: ["ignore", logFd, logFd],
          windowsHide: true,
          windowsVerbatimArguments: true,
          detached: true,
        })
      : spawn("npx", ["tauri", "dev", "--config", overlayPath], {
          cwd: projectRoot,
          env,
          stdio: ["ignore", logFd, logFd],
          detached: true,
        });
  child.unref();
  closeSync(logFd);
  return child;
}

export async function waitHttp(origin, timeoutMs) {
  const start = Date.now();
  let last = "";
  const urls = [
    origin,
    origin.replace("localhost", "127.0.0.1"),
    origin.replace("http://localhost", "http://[::1]"),
    origin.replace("http://127.0.0.1", "http://[::1]"),
  ];
  while (Date.now() - start < timeoutMs) {
    for (const url of urls) {
      try {
        const res = await fetch(url, { redirect: "manual" });
        if (res.status >= 200 && res.status < 500) return url;
        last = `${url} HTTP ${res.status}`;
      } catch (error) {
        last = `${url} ${error.message}`;
      }
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`Tauri Vite dev URL did not become ready at ${origin}: ${last}`);
}
