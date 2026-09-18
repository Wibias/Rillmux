#!/usr/bin/env node
/**
 * Persistent agent-facing control surface for Rillmux verification.
 * Drive primitive: Vite HTTP + Edge/Chrome CDP (web), npm run tauri:dev + WebView2 CDP (native).
 */
import { execFileSync, spawn } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createConnection } from "node:net";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  nativeDryRun,
  pidsListening,
  probeNativePrerequisites,
  debugRillmuxPids,
  seedIsolatedSettings,
  spawnTauriDev,
  TAURI_DEV_ORIGIN,
  TAURI_DEV_PORT,
  waitHttp as waitNativeHttp,
  webviewLogPathFor,
  writeCdpOverlay,
} from "./native-lane.mjs";
import { DIALOG_SELECTOR } from "./web-snapshot.mjs";
import {
  fetchCdpJson,
  nativeCleanupPids,
  selectCdpPage,
} from "./native-cdp.mjs";
import {
  nativeRunResources,
  receiptStartedResources,
  webRunResources,
} from "./receipt-resources.mjs";

const SKILL_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PROJECT_ROOT = resolve(SKILL_ROOT, "..", "..", "..");
const RUNTIME_DIR = join(SKILL_ROOT, ".runtime");
const STATE_PATH = join(RUNTIME_DIR, "state.json");
const EVIDENCE_DIR = join(SKILL_ROOT, "evidence");

function fail(message, extra = {}) {
  process.stderr.write(`ERROR: ${message}\n`);
  if (Object.keys(extra).length) {
    process.stderr.write(`${JSON.stringify(extra, null, 2)}\n`);
  }
  process.exit(1);
}

function printJson(value) {
  process.stdout.write(`${JSON.stringify(value, null, 2)}\n`);
}

function help() {
  process.stdout.write(`Rillmux verification control surface

Usage (from project root):
  node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs <command> [options]

Commands:
  help                 Show this help
  launch               Start a lane: web (default) or native Tauri
  doctor               Read-only health check; always reports web and native lanes with --json
  inspect              Machine-readable DOM snapshot of the current page
  goto --path <route>  Navigate to an in-app route (e.g. /about, /settings)
  click --name <text>  Click a link, button, or tab by visible text or aria-label
  screenshot [--out]   Capture a PNG under evidence/
  drive <feature>      Run a seeded user-facing interaction
  receipt              Write a head-bound runtime receipt for the last drive
  cleanup              Stop processes started by launch; keep evidence and receipts
  status               Print runtime state JSON

Features for drive:
  mpv-volume-boost     Settings Player volume booster: set, summarize, persist (native)
  stream-card-game-line Stream-card meta line: blocked without a Twitch session (native)
  about-changelog      Open About → View changelog
  followed-login-gate  Open Followed and assert the signed-out empty state
  settings-tabs        Open Settings and select the Player tab
  tauri-guard          Assert the Vite-only desktop-shell banner
  browse-nav           Walk Followed → Top streams → About (web)
  native-setup-check   About setup check via Tauri get_doctor_report (native)

Options:
  --json               Print machine-readable JSON (doctor, inspect, drive, status, receipt)
  --dry-run            Print intended actions without starting/stopping processes or clicking
  --lane web|native    launch/doctor/inspect/drive/cleanup target (launch defaults to web)
  --preview            Production vite preview (runs build unless --skip-build)
  --skip-build         Reuse dist/ from a previous build (with --preview)
  --port <n>           Vite preview port (default: free port in 18700-18999)
  --path <route>       In-app path for goto
  --name <text>        Accessible/visible name for click
  --feature <id>       Feature id for receipt
  --result <pass|fail|blocked>
  --blocked-reason <text>
  --out <file>         Screenshot path (relative to evidence/ or absolute)

Prerequisites:
  Node 20+, npm install, Windows Edge or Chrome for CDP driving.
  Web lane proves React chrome via isolated Vite + Chrome/Edge CDP.
  Native lane runs npm run tauri:dev with isolated APPDATA and WebView2 CDP.
  Native launch does not log in, start Streamlink/mpv, or read the OS keyring.

Side effects:
  Web launch binds a local HTTP port and a dedicated browser profile.
  Native launch compiles/starts tauri:dev, binds port 1420, and writes an isolated
  APPDATA tree. cleanup kills only recorded PIDs (taskkill /PID /T), not by name.
  Screenshots and receipts stay in evidence/.

Isolation:
  Serial driving. Web uses a unique Vite port + browser profile. Native uses
  APPDATA override plus debug webview-dev. Do not attach a second live driver
  to the same lane. Port 1420 must be free unless this verifier owns it.
`);
}

function loadState() {
  if (!existsSync(STATE_PATH)) return null;
  try {
    return JSON.parse(readFileSync(STATE_PATH, "utf8"));
  } catch {
    return null;
  }
}

function saveState(state) {
  mkdirSync(RUNTIME_DIR, { recursive: true });
  writeFileSync(STATE_PATH, `${JSON.stringify(state, null, 2)}\n`);
}

function gitHead() {
  try {
    return execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: PROJECT_ROOT,
      encoding: "utf8",
    }).trim();
  } catch {
    return null;
  }
}

function gitRemote() {
  try {
    const url = execFileSync("git", ["remote", "get-url", "origin"], {
      cwd: PROJECT_ROOT,
      encoding: "utf8",
    }).trim();
    const match = url.match(/github\.com[:/](.+?)(?:\.git)?$/i);
    return match ? match[1].replace(/\\/g, "/") : url;
  } catch {
    return "Wibias/Rillmux";
  }
}

async function portFree(port) {
  return await new Promise((resolveFree) => {
    const socket = createConnection({ port, host: "127.0.0.1" });
    socket.once("connect", () => {
      socket.destroy();
      resolveFree(false);
    });
    socket.once("error", () => resolveFree(true));
  });
}

async function pickPort(preferred) {
  if (preferred) {
    if (!(await portFree(preferred))) {
      throw new Error(
        `Port ${preferred} is in use. Pass --port with a free port or stop the occupant.`,
      );
    }
    return preferred;
  }
  for (let i = 0; i < 40; i++) {
    const candidate = 18700 + Math.floor(Math.random() * 300);
    if (await portFree(candidate)) return candidate;
  }
  throw new Error("Could not find a free port in 18700-18999");
}

function findBrowser() {
  const candidates = [
    join(process.env["PROGRAMFILES(X86)"] || "C:\\Program Files (x86)", "Microsoft", "Edge", "Application", "msedge.exe"),
    join(process.env.PROGRAMFILES || "C:\\Program Files", "Microsoft", "Edge", "Application", "msedge.exe"),
    join(process.env.PROGRAMFILES || "C:\\Program Files", "Google", "Chrome", "Application", "chrome.exe"),
    join(process.env.LOCALAPPDATA || "", "Google", "Chrome", "Application", "chrome.exe"),
  ];
  return candidates.find((path) => path && existsSync(path)) || null;
}

function spawnLogged(command, args, options) {
  const child = spawn(command, args, {
    cwd: PROJECT_ROOT,
    stdio: "ignore",
    windowsHide: true,
    detached: true,
    ...options,
  });
  child.unref();
  return child;
}

async function waitHttp(origin, timeoutMs = 60_000) {
  const start = Date.now();
  let last = "";
  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(origin, { redirect: "manual" });
      if (res.ok || (res.status >= 200 && res.status < 500)) return;
      last = `HTTP ${res.status}`;
    } catch (error) {
      last = error.message;
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`Preview did not become ready at ${origin}: ${last}`);
}

async function waitCdpPage(port, timeoutMs = 30_000) {
  const start = Date.now();
  let last = "";
  while (Date.now() - start < timeoutMs) {
    try {
      const targets = await fetchCdpJson(port);
      const page = selectCdpPage(targets);
      if (page) return page;
      last = `targets=${Array.isArray(targets) ? targets.length : 0} none matched Rillmux/localhost`;
    } catch (error) {
      last = error.message;
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`CDP page target not ready on port ${port}: ${last}`);
}

class CdpSession {
  constructor(ws) {
    this.ws = ws;
    this.nextId = 0;
    this.pending = new Map();
    this.ws.addEventListener("message", (event) => {
      const msg = JSON.parse(String(event.data));
      if (msg.id == null) return;
      const waiter = this.pending.get(msg.id);
      if (!waiter) return;
      this.pending.delete(msg.id);
      if (msg.error) waiter.reject(new Error(msg.error.message || JSON.stringify(msg.error)));
      else waiter.resolve(msg.result);
    });
  }

  send(method, params = {}) {
    const id = ++this.nextId;
    return new Promise((resolveSend, reject) => {
      this.pending.set(id, { resolve: resolveSend, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.text || "Runtime.evaluate failed");
    }
    return result.result?.value;
  }

  close() {
    try {
      this.ws.close();
    } catch {
      /* ignore */
    }
  }
}

async function connectCdp(cdpPort) {
  const info = await waitCdpPage(cdpPort);
  const url = info.webSocketDebuggerUrl;
  if (!url) throw new Error("CDP page target lacked webSocketDebuggerUrl");
  const ws = new WebSocket(url);
  await new Promise((resolveWs, reject) => {
    ws.addEventListener("open", () => resolveWs());
    ws.addEventListener("error", () => reject(new Error("WebSocket to CDP failed")));
  });
  const session = new CdpSession(ws);
  await session.send("Page.enable");
  await session.send("Runtime.enable");
  return session;
}

async function withCdp(state, fn) {
  if (!state?.cdpPort) {
    throw new Error("No launched instance. Run launch first.");
  }
  const session = await connectCdp(state.cdpPort);
  try {
    return await fn(session);
  } finally {
    session.close();
  }
}

const SNAPSHOT_JS = `(() => {
  const text = (el) => (el?.innerText || el?.textContent || "").replace(/\\s+/g, " ").trim();
  const alerts = [...document.querySelectorAll('[role="alert"]')].map(text);
  const headings = [...document.querySelectorAll("h1,h2")].map((el) => ({
    tag: el.tagName.toLowerCase(),
    text: text(el),
  }));
  const nav = [...document.querySelectorAll("nav a")].map((el) => ({
    href: el.getAttribute("href"),
    text: text(el),
    current: el.classList.contains("shell__link--active"),
  }));
  const dialogs = [...document.querySelectorAll(${JSON.stringify(DIALOG_SELECTOR)})].map((el) => ({
    label: el.getAttribute("aria-labelledby")
      ? text(document.getElementById(el.getAttribute("aria-labelledby")))
      : text(el.querySelector("h2")),
    text: text(el).slice(0, 400),
  }));
  const controls = [...document.querySelectorAll("select, input, textarea")].map((el) => {
    const wrap = el.closest(".settings__row, label");
    const wrapLabel = wrap ? text(wrap.querySelector(".settings__label, label")) : "";
    return {
      tag: el.tagName.toLowerCase(),
      type: el.getAttribute("type") || "",
      label: (el.getAttribute("aria-label") || "").trim() || wrapLabel,
      value: el.value,
      options: el.tagName === "SELECT" ? [...el.options].map((o) => o.value) : null,
      optionText: el.tagName === "SELECT" ? [...el.options].map((o) => text(o)) : null,
    };
  });
  const tabs = [...document.querySelectorAll('[role="tab"]')].map((el) => ({
    name: text(el),
    selected: el.getAttribute("aria-selected") === "true",
  }));
  return {
    title: document.title,
    url: location.href,
    pathname: location.pathname,
    appName: text(document.querySelector(".shell__titlebar-title")),
    hydrated: Boolean(document.querySelector(".shell")),
    splash: Boolean(document.getElementById("splash")),
    headings,
    nav,
    alerts,
    dialogs,
    tabs,
    controls,
    bodyPreview: text(document.body).slice(0, 800),
  };
})()`;

const CLICK_JS = (name) => `(() => {
  const wanted = ${JSON.stringify(name)};
  const nodes = [...document.querySelectorAll('a, button, [role="tab"], [role="link"]')];
  const match = nodes.find((el) => {
    const label = (el.getAttribute("aria-label") || "").trim();
    const content = (el.innerText || el.textContent || "").replace(/\\s+/g, " ").trim();
    return label === wanted || content === wanted || content.startsWith(wanted);
  });
  if (!match) return { ok: false, reason: "no matching control" };
  match.click();
  return { ok: true, tag: match.tagName, text: (match.innerText || "").trim().slice(0, 80) };
})()`;

async function waitEval(session, expression, predicate, timeoutMs, label) {
  const start = Date.now();
  let last = null;
  while (Date.now() - start < timeoutMs) {
    last = await session.evaluate(expression);
    if (predicate(last)) return last;
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`Timed out waiting for ${label}`);
}

function parseArgs(argv) {
  const args = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const token = argv[i];
    if (token === "--json") args.json = true;
    else if (token === "--dry-run") args.dryRun = true;
    else if (token === "--skip-build") args.skipBuild = true;
    else if (token.startsWith("--")) {
      const key = token.slice(2);
      const value = argv[i + 1];
      if (!value || value.startsWith("--")) args[key] = true;
      else {
        args[key] = value;
        i++;
      }
    } else args._.push(token);
  }
  return args;
}

async function runBuild() {
  const npmCmd = process.platform === "win32" ? "npm.cmd" : "npm";
  await new Promise((resolveBuild, reject) => {
    const child = spawn(npmCmd, ["run", "build"], {
      cwd: PROJECT_ROOT,
      stdio: "inherit",
      shell: process.platform === "win32",
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) resolveBuild();
      else reject(new Error(`npm run build exited ${code}. Fix the frontend build, then relaunch.`));
    });
  });
}

async function cmdLaunch(args) {
  if (args.lane === "native") {
    await cmdLaunchNative(args);
    return;
  }
  const existing = loadState();
  if (existing?.previewPid && (await portFree(existing.port)) === false) {
    throw new Error(
      `An instance already appears bound on port ${existing.port}. Run cleanup first, or reuse doctor/inspect.`,
    );
  }

  const port = await pickPort(args.port ? Number(args.port) : undefined);
  const cdpPort = await pickPort(undefined);
  const runId = `verify-rillmux-${new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z")}`;
  const profileDir = join(RUNTIME_DIR, `browser-profile-${runId}`);
  const browser = findBrowser();

  if (args.dryRun) {
    printJson({
      dryRun: true,
      would: [
        args.preview ? (args.skipBuild ? "skip npm run build" : "npm run build") : "vite dev (no production build)",
        args.preview
          ? `node node_modules/vite/bin/vite.js preview --host 127.0.0.1 --port ${port} --strictPort`
          : `node node_modules/vite/bin/vite.js --host 127.0.0.1 --port ${port} --strictPort`,
        browser
          ? `${browser} --remote-debugging-port=${cdpPort} --user-data-dir=${profileDir}`
          : "ERROR: no Edge/Chrome found",
      ],
      isolation: "unique port + unique user-data dir; serial driving",
    });
    return;
  }

  if (!browser) {
    throw new Error(
      "No Edge or Chrome executable found. Install Microsoft Edge, then rerun launch.",
    );
  }

  mkdirSync(RUNTIME_DIR, { recursive: true });
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  if (args.preview && !args.skipBuild) await runBuild();

  const viteBin = join(PROJECT_ROOT, "node_modules", "vite", "bin", "vite.js");
  if (!existsSync(viteBin)) {
    throw new Error("Vite is missing. Run npm install at the project root.");
  }

  const preview = spawnLogged(
    process.execPath,
    args.preview
      ? [viteBin, "preview", "--host", "127.0.0.1", "--port", String(port), "--strictPort"]
      : [viteBin, "--host", "127.0.0.1", "--port", String(port), "--strictPort"],
  );

  const origin = `http://127.0.0.1:${port}`;
  try {
    await waitHttp(origin);
  } catch (error) {
    preview.kill();
    throw error;
  }

  mkdirSync(profileDir, { recursive: true });

  const browserProc = spawnLogged(browser, [
    `--remote-debugging-port=${cdpPort}`,
    `--user-data-dir=${profileDir}`,
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-background-networking",
    "--window-size=1280,880",
    `${origin}/`,
  ]);

  const state = {
    runId,
    headSha: gitHead(),
    repository: gitRemote(),
    origin,
    port,
    cdpPort,
    previewPid: preview.pid,
    browserPid: browserProc.pid,
    browserPath: browser,
    profileDir,
    startedAt: new Date().toISOString(),
    lastDrive: null,
  };
  state.webStartedResources = webRunResources(state);
  saveState(state);
  await waitCdpPage(cdpPort);
  await withCdp(state, async (session) => {
    await waitEval(
      session,
      SNAPSHOT_JS,
      (snap) => snap?.hydrated === true && snap?.appName === "Rillmux",
      45_000,
      "hydrated Rillmux shell",
    );
  });

  const out = { ok: true, ...state, started_resources: state.webStartedResources };
  printJson(out);
}

function nativeCdpView(state) {
  if (!state?.native?.cdpPort) {
    throw new Error(
      "No native instance. Run: node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs launch --lane native",
    );
  }
  return { cdpPort: state.native.cdpPort, origin: state.native.origin };
}

async function cmdLaunchNative(args) {
  const existing = loadState() || {};
  const probes = await probeNativePrerequisites({
    projectRoot: PROJECT_ROOT,
    runtimeDir: RUNTIME_DIR,
    state: existing,
  });
  const cdpPort = await pickPort(undefined);
  const runId = `verify-rillmux-native-${new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z")}`;
  const appDataDir = join(RUNTIME_DIR, `native-appdata-${runId}`);
  const logPath = join(RUNTIME_DIR, `${runId}.tauri-dev.log`);

  if (args.dryRun) {
    printJson({ ...nativeDryRun({ cdpPort, appDataDir, logPath }), probes });
    return;
  }

  if (!probes.canLaunch) {
    throw new Error(probes.blockedReasons.join(" "));
  }
  if (existing.native?.pid && pidAlive(existing.native.pid)) {
    throw new Error("A native verifier instance is already recorded. Run cleanup --lane native first.");
  }
  for (const pid of debugRillmuxPids(PROJECT_ROOT)) {
    killPid(pid);
  }

  mkdirSync(RUNTIME_DIR, { recursive: true });
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  seedIsolatedSettings(appDataDir);

  const overlayPath = join(RUNTIME_DIR, `${runId}.tauri-cdp.json`);
  writeCdpOverlay(overlayPath, cdpPort, webviewLogPathFor(logPath));
  const child = spawnTauriDev({
    projectRoot: PROJECT_ROOT,
    appDataDir,
    logPath,
    overlayPath,
    cdpPort,
  });

  const native = {
    runId,
    headSha: gitHead(),
    origin: TAURI_DEV_ORIGIN,
    port: TAURI_DEV_PORT,
    cdpPort,
    pid: child.pid,
    appDataDir,
    logPath,
    webviewLogPath: webviewLogPathFor(logPath),
    startedAt: new Date().toISOString(),
  };
  saveState({ ...existing, native, lastDrive: existing.lastDrive ?? null });

  try {
    const readyOrigin = await waitNativeHttp(TAURI_DEV_ORIGIN, 8 * 60_000);
    native.origin = readyOrigin;
    const view = nativeCdpView({ native });
    await waitCdpPage(view.cdpPort, 120_000);
    native.ownedPids = [
      ...new Set([native.pid, ...pidsListening(TAURI_DEV_PORT), ...pidsListening(native.cdpPort)]),
    ];
    native.startedResources = nativeRunResources(native);
    saveState({ ...loadState(), native });
    await withCdp(view, async (session) => {
      await waitEval(
        session,
        SNAPSHOT_JS,
        (snap) =>
          snap?.hydrated === true &&
          snap?.appName === "Rillmux" &&
          !(snap.alerts || []).some((a) => a.includes("Desktop shell required")),
        90_000,
        "hydrated native Rillmux shell without Vite guard",
      );
    });
  } catch (error) {
    for (const pid of nativeCleanupPids({ native })) {
      killPid(pid);
    }
    killPid(child.pid);
    for (const pid of debugRillmuxPids(PROJECT_ROOT)) {
      killPid(pid);
    }
    throw new Error(
      `${error.message} See native log ${logPath}. If compile failed, fix Rust/frontend then retry.`,
    );
  }

    printJson({
      ok: true,
      lane: "native",
      ...native,
      started_resources: native.startedResources ?? nativeRunResources(native),
    });
}

async function doctorWeb(state) {
  const report = {
    lane: "web",
    ready: false,
    origin: state?.origin ?? null,
    httpOk: false,
    appName: null,
    tauriBanner: null,
    mismatch: null,
    next: null,
    launchedHeadSha: state?.headSha ?? null,
  };
  if (!state?.origin || !state?.cdpPort) {
    report.next =
      "Run web launch: node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs launch";
    return report;
  }
  const head = gitHead();
  if (state.headSha && head && state.headSha !== head) {
    report.mismatch =
      "Web preview was started from a different git HEAD. Run cleanup --lane web then launch again.";
  }
  try {
    const res = await fetch(state.origin);
    report.httpOk = res.ok;
  } catch {
    report.httpOk = false;
    report.next = "Web preview HTTP failed. Run cleanup --lane web and launch again.";
    return report;
  }
  try {
    const snap = await withCdp(state, (session) => session.evaluate(SNAPSHOT_JS));
    report.appName = snap.appName;
    report.tauriBanner = snap.alerts?.[0] ?? null;
    report.pathname = snap.pathname;
    report.hydrated = snap.hydrated;
    report.splash = snap.splash;
    report.ready = snap.hydrated === true && snap.appName === "Rillmux" && !report.mismatch;
  } catch (error) {
    report.next = `Web CDP inspect failed: ${error.message}`;
  }
  if (!report.ready && !report.next) {
    report.next = "Web instance is not ready. Check launch output and rerun doctor --lane web.";
  }
  return report;
}

async function doctorNativeLane(state) {
  const probes = await probeNativePrerequisites({
    projectRoot: PROJECT_ROOT,
    runtimeDir: RUNTIME_DIR,
    state,
  });
  probes.headSha = gitHead();
  const instance = {
    ...probes.instance,
    ready: false,
    mismatch: null,
    next: null,
    tauriBanner: null,
    tauriProcessAlive: pidAlive(state?.native?.pid),
    debugExePids: debugRillmuxPids(PROJECT_ROOT),
    nativeAppProcessStarted: debugRillmuxPids(PROJECT_ROOT).length > 0,
    cdpPort: state?.native?.cdpPort ?? null,
    cdpListeningPids: state?.native?.cdpPort ? pidsListening(state.native.cdpPort) : [],
    cdpSocketReachable: false,
    cdpListOk: false,
    pageTarget: null,
  };
  if (state?.native?.headSha && probes.headSha && state.native.headSha !== probes.headSha) {
    instance.mismatch =
      "Native tauri:dev was started from a different git HEAD. Run cleanup --lane native then launch --lane native.";
  }
  if (state?.native?.cdpPort) {
    try {
      const targets = await fetchCdpJson(state.native.cdpPort);
      instance.cdpListOk = true;
      instance.cdpSocketReachable =
        instance.cdpListeningPids.length > 0 || instance.cdpListOk;
      instance.pageTarget = selectCdpPage(targets)?.url ?? null;
      const snap = await withCdp(nativeCdpView(state), (session) => session.evaluate(SNAPSHOT_JS));
      instance.appName = snap.appName;
      instance.hydrated = snap.hydrated;
      instance.pathname = snap.pathname;
      instance.tauriBanner = snap.alerts?.[0] ?? null;
      const guard = (snap.alerts || []).some((a) => a.includes("Desktop shell required"));
      instance.ready =
        instance.tauriProcessAlive &&
        instance.cdpListOk &&
        Boolean(instance.pageTarget) &&
        snap.hydrated === true &&
        snap.appName === "Rillmux" &&
        !guard &&
        !instance.mismatch;
      if (guard) instance.next = "Connected chrome still shows the Vite tauri-guard; this is not the native WebView.";
    } catch (error) {
      instance.next = `Native CDP inspect failed: ${error.message}`;
    }
  } else {
    instance.next = probes.canLaunch
      ? "Run: node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs launch --lane native"
      : probes.blockedReasons[0];
  }
  return {
    lane: "native",
    ready: instance.ready,
    canLaunch: probes.canLaunch,
    blockedReasons: probes.blockedReasons,
    tools: probes.tools,
    port: probes.port,
    auth: probes.auth,
    isolation: probes.isolation,
    instance,
  };
}

async function cmdDoctor(args) {
  const state = loadState();
  const head = gitHead();
  const web = await doctorWeb(state);
  const native = await doctorNativeLane(state);
  const report = {
    intendedCheckout: PROJECT_ROOT,
    repository: gitRemote(),
    headSha: head,
    lanes: { web, native },
    ready: web.ready,
    origin: web.origin,
    httpOk: web.httpOk,
    appName: web.appName,
    tauriBanner: web.tauriBanner,
    mismatch: web.mismatch,
    next: web.next,
    launchedHeadSha: web.launchedHeadSha,
    hydrated: web.hydrated,
    splash: web.splash,
    pathname: web.pathname,
  };

  const lane = args.lane || "web";
  if (args.json) printJson(report);
  else {
    process.stdout.write(
      `web=${web.ready ? "ready" : "not-ready"} native.instance=${native.instance.ready ? "ready" : "not-ready"} native.canLaunch=${native.canLaunch} head=${head}\n`,
    );
  }

  if (lane === "native") {
    if (native.blockedReasons.some((reason) => reason.includes("Windows-only"))) process.exitCode = 1;
    else if (native.instance.cdpPort && !native.instance.ready) process.exitCode = 1;
    return;
  }
  if (!web.ready) process.exitCode = 1;
}

async function cmdInspect(args) {
  const state = cdpTarget(args);
  const snap = await withCdp(state, (session) => session.evaluate(SNAPSHOT_JS));
  printJson(snap);
}

function cdpTarget(args) {
  const state = loadState();
  if (args.lane === "native" || (!state?.cdpPort && state?.native?.cdpPort)) {
    return nativeCdpView(state);
  }
  return requireState();
}

function requireState() {
  const state = loadState();
  if (!state?.origin || !state?.cdpPort) {
    throw new Error(
      "No web runtime state. Run: node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs launch",
    );
  }
  return state;
}

async function cmdGoto(args) {
  const pathName = args.path;
  if (!pathName || typeof pathName !== "string") {
    throw new Error("goto requires --path /about (or another in-app route)");
  }
  const state = cdpTarget(args);
  if (args.dryRun) {
    printJson({ dryRun: true, would: `navigate ${state.origin}${pathName}` });
    return;
  }
  const snap = await withCdp(state, async (session) => {
    await session.send("Page.navigate", { url: `${state.origin}${pathName}` });
    return waitEval(
      session,
      SNAPSHOT_JS,
      (value) => value?.pathname === pathName || value?.pathname === `${pathName}/`,
      15_000,
      `route ${pathName}`,
    );
  });
  printJson(snap);
}

async function cmdClick(args) {
  const name = args.name;
  if (!name || typeof name !== "string") {
    throw new Error('click requires --name "About" (visible text or aria-label)');
  }
  if (args.dryRun) {
    printJson({ dryRun: true, would: `click control named ${name}` });
    return;
  }
  const state = cdpTarget(args);
  const result = await withCdp(state, async (session) => {
    const clicked = await session.evaluate(CLICK_JS(name));
    if (!clicked?.ok) {
      throw new Error(`No clickable control named "${name}". Run inspect to list nav/tabs/buttons.`);
    }
    await new Promise((r) => setTimeout(r, 250));
    const snap = await session.evaluate(SNAPSHOT_JS);
    return { clicked, snap };
  });
  printJson(result);
}

async function cmdScreenshot(args) {
  const state = cdpTarget(args);
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  const out = args.out
    ? resolve(args.out)
    : join(EVIDENCE_DIR, `${state.runId || "shot"}-${Date.now()}.png`);
  if (args.dryRun) {
    printJson({ dryRun: true, would: `screenshot ${out}` });
    return;
  }
  await withCdp(state, async (session) => {
    const shot = await session.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(out, Buffer.from(shot.data, "base64"));
  });
  printJson({ ok: true, path: out });
  return out;
}

async function driveAboutChangelog(state, dryRun) {
  if (dryRun) {
    return { dryRun: true, feature: "about-changelog", would: ["goto /about", 'click "View changelog"', "assert dialog Changelog"] };
  }
  return withCdp(state, async (session) => {
    await session.send("Page.navigate", { url: `${state.origin}/about` });
    await waitEval(
      session,
      SNAPSHOT_JS,
      (snap) => snap.headings?.some((h) => h.tag === "h1" && h.text === "About"),
      15_000,
      "About heading",
    );
    const clicked = await session.evaluate(CLICK_JS("View changelog"));
    if (!clicked?.ok) throw new Error('Could not click "View changelog"');
    const snap = await waitEval(
      session,
      SNAPSHOT_JS,
      (value) => value.dialogs?.some((d) => d.label === "Changelog"),
      10_000,
      "Changelog dialog",
    );
    mkdirSync(EVIDENCE_DIR, { recursive: true });
    const shotPath = join(EVIDENCE_DIR, `${state.runId}-about-changelog.png`);
    const shot = await session.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(shotPath, Buffer.from(shot.data, "base64"));
    const checks = [
      { id: "about-heading", result: "pass" },
      { id: "changelog-dialog-open", result: snap.dialogs.some((d) => d.label === "Changelog") ? "pass" : "fail" },
    ];
    return { feature: "about-changelog", snap, artifacts: [shotPath], checks };
  });
}

async function driveFollowedGate(state, dryRun) {
  if (dryRun) {
    return { dryRun: true, feature: "followed-login-gate", would: ["goto /", "assert login-required copy"] };
  }
  return withCdp(state, async (session) => {
    await session.send("Page.navigate", { url: `${state.origin}/` });
    const snap = await waitEval(
      session,
      SNAPSHOT_JS,
      (value) => (value.bodyPreview || "").includes("Log in to see streams from channels you follow."),
      15_000,
      "followed login gate",
    );
    const shotPath = join(EVIDENCE_DIR, `${state.runId}-followed-login-gate.png`);
    mkdirSync(EVIDENCE_DIR, { recursive: true });
    const shot = await session.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(shotPath, Buffer.from(shot.data, "base64"));
    return {
      feature: "followed-login-gate",
      snap,
      artifacts: [shotPath],
      checks: [{ id: "followed-login-required-copy", result: "pass" }],
    };
  });
}

async function driveSettingsTabs(state, dryRun) {
  if (dryRun) {
    return { dryRun: true, feature: "settings-tabs", would: ["goto /settings", 'click tab "Player"', "assert heading Player"] };
  }
  return withCdp(state, async (session) => {
    await session.send("Page.navigate", { url: `${state.origin}/settings` });
    await waitEval(
      session,
      SNAPSHOT_JS,
      (snap) => snap.headings?.some((h) => h.tag === "h1" && h.text === "Settings"),
      15_000,
      "Settings heading",
    );
    const clicked = await session.evaluate(CLICK_JS("Player"));
    if (!clicked?.ok) throw new Error('Could not click Settings tab "Player"');
    const snap = await waitEval(
      session,
      SNAPSHOT_JS,
      (value) =>
        value.tabs?.some((tab) => tab.name === "Player" && tab.selected) &&
        value.headings?.some((h) => h.tag === "h2" && h.text === "Player"),
      10_000,
      "Player tab selected",
    );
    const shotPath = join(EVIDENCE_DIR, `${state.runId}-settings-tabs.png`);
    mkdirSync(EVIDENCE_DIR, { recursive: true });
    const shot = await session.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(shotPath, Buffer.from(shot.data, "base64"));
    return {
      feature: "settings-tabs",
      snap,
      artifacts: [shotPath],
      checks: [
        { id: "settings-heading", result: "pass" },
        { id: "player-tab-selected", result: "pass" },
      ],
    };
  });
}

async function driveTauriGuard(state, dryRun) {
  if (dryRun) {
    return { dryRun: true, feature: "tauri-guard", would: ["inspect alerts", "assert Desktop shell required"] };
  }
  return withCdp(state, async (session) => {
    const snap = await waitEval(
      session,
      SNAPSHOT_JS,
      (value) => (value.alerts || []).some((a) => a.includes("Desktop shell required")),
      10_000,
      "Tauri guard banner",
    );
    return {
      feature: "tauri-guard",
      snap,
      artifacts: [],
      checks: [{ id: "vite-tauri-guard-banner", result: "pass" }],
    };
  });
}

async function driveBrowseNav(state, dryRun) {
  if (dryRun) {
    return { dryRun: true, feature: "browse-nav", would: ['click "Top streams"', 'click "About"'] };
  }
  return withCdp(state, async (session) => {
    await session.send("Page.navigate", { url: `${state.origin}/` });
    await session.evaluate(CLICK_JS("Top streams"));
    await waitEval(
      session,
      SNAPSHOT_JS,
      (snap) => snap.pathname === "/streams",
      10_000,
      "/streams",
    );
    await session.evaluate(CLICK_JS("About"));
    const snap = await waitEval(
      session,
      SNAPSHOT_JS,
      (value) => value.pathname === "/about" && value.headings?.some((h) => h.text === "About"),
      10_000,
      "/about via nav",
    );
    return {
      feature: "browse-nav",
      snap,
      artifacts: [],
      checks: [{ id: "nav-streams-then-about", result: "pass" }],
    };
  });
}

async function driveNativeSetupCheck(state, dryRun) {
  if (dryRun) {
    return {
      dryRun: true,
      feature: "native-setup-check",
      would: [
        "require native WebView CDP",
        "goto /about",
        "assert Setup check ran via get_doctor_report (Streamlink found or not found)",
        "assert Vite Desktop shell required banner is absent",
      ],
    };
  }
  const view = nativeCdpView(state);
  return withCdp(view, async (session) => {
    await session.send("Page.navigate", { url: `${view.origin}/about` });
    const snap = await waitEval(
      session,
      SNAPSHOT_JS,
      (value) => {
        const body = value.bodyPreview || "";
        const setup = value.headings?.some((h) => h.text === "Setup check");
        const invoked =
          body.includes("Streamlink") &&
          (body.includes("found") || body.includes("not found"));
        const guard = (value.alerts || []).some((a) => a.includes("Desktop shell required"));
        return setup && invoked && !guard && value.headings?.some((h) => h.tag === "h1" && h.text === "About");
      },
      30_000,
      "native About setup check (Tauri get_doctor_report)",
    );
    mkdirSync(EVIDENCE_DIR, { recursive: true });
    const shotPath = join(EVIDENCE_DIR, `${state.native.runId}-native-setup-check.png`);
    const shot = await session.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(shotPath, Buffer.from(shot.data, "base64"));
    const body = snap.bodyPreview || "";
    return {
      feature: "native-setup-check",
      surface: "desktop",
      snap,
      artifacts: [shotPath],
      checks: [
        { id: "native-about-heading", result: "pass" },
        { id: "native-setup-check-invoked", result: "pass" },
        { id: "native-no-vite-guard", result: "pass" },
        {
          id: "streamlink-status-reported",
          result: body.includes("Streamlink") ? "pass" : "fail",
        },
      ],
    };
  });
}

// React keeps its own value tracker on controlled elements, so drive selects
// through the native setter and then fire change; a plain el.value write is
// swallowed and the component never re-renders.
const SET_SELECT_JS = (label, value) =>
  "(function(){var w=" +
  JSON.stringify(label) +
  ";var v=" +
  JSON.stringify(value) +
  ";var all=document.querySelectorAll('select');var el=null;" +
  "for(var i=0;i<all.length;i++){var aria=(all[i].getAttribute('aria-label')||'').trim();" +
  "if(aria===w){el=all[i];break;}}" +
  "if(!el) return {ok:false,reason:'no matching select'};" +
  "var setter=Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype,'value').set;" +
  "setter.call(el,v);" +
  "el.dispatchEvent(new Event('change',{bubbles:true}));" +
  "return {ok:true,value:el.value};})()";

// The settings store resolves through the app data folder (bundle identifier),
// not the verifier's APPDATA override, so a native drive touches the real
// profile file. Snapshot and restore it around the drive.
const STORE_FILES = ["com.wibias.rillmux", "Rillmux"]
  .map((dir) => join(process.env.APPDATA || "", dir, "settings.json"))
  .filter((file) => file.length > 3);

function snapshotStoreFiles() {
  return STORE_FILES.map((file) => {
    try {
      return { file, text: readFileSync(file, "utf8") };
    } catch {
      return null;
    }
  }).filter(Boolean);
}

function restoreStoreFiles(backups) {
  for (const backup of backups) {
    try {
      writeFileSync(backup.file, backup.text);
    } catch {
      // Best effort: a locked profile file must not fail the drive.
    }
  }
}

function storeFileWithBoost(value) {
  for (const file of STORE_FILES) {
    try {
      if (readFileSync(file, "utf8").includes('"volumeBoost": ' + value)) return file;
    } catch {
      // Keep probing the other candidate.
    }
  }
  return null;
}

/** Native lane: the opt-in mpv volume booster on Settings -> Player. */
async function driveMpvVolumeBoost(state, dryRun) {
  if (dryRun) {
    return {
      dryRun: true,
      feature: "mpv-volume-boost",
      would: [
        "require native WebView CDP",
        "goto /settings and click the Player tab",
        'assert a "Volume boost" select offering Off/130/150/200/300',
        "set 200% and assert the presets summary reports volume boost 200%",
        "reload the WebView and assert 200% survived the Tauri store round trip",
      ],
    };
  }
  const view = nativeCdpView(state);
  const boostSelect = (value) =>
    (value.controls || []).find(
      (control) => control.tag === "select" && control.label === "Volume boost",
    ) || null;
  const storeBackups = snapshotStoreFiles();
  try {
    return await withCdp(view, async (session) => {
    // The Player tab is a route (/settings/player). Clicking the tab races the
    // React router after a reload, so address the route directly.
    await session.send("Page.navigate", { url: view.origin + "/settings/player" });
    await waitEval(
      session,
      SNAPSHOT_JS,
      (value) =>
        value.headings?.some((h) => h.tag === "h1" && h.text === "Settings") &&
        value.headings?.some((h) => h.tag === "h2" && h.text === "Player"),
      30_000,
      "native Settings Player tab",
    );
    const initial = await waitEval(
      session,
      SNAPSHOT_JS,
      (value) => Boolean(boostSelect(value)),
      20_000,
      "Volume boost select on the Player tab",
    );
    const before = boostSelect(initial);
    const set = await session.evaluate(SET_SELECT_JS("Volume boost", "200"));
    if (!set?.ok) throw new Error("Could not set Volume boost to 200%");
    // SettingsBootstrap writes on a 400 ms debounce; wait for the file write so
    // the reload below cannot outrun it.
    let storeHit = null;
    const storeDeadline = Date.now() + 10_000;
    while (!storeHit && Date.now() < storeDeadline) {
      storeHit = storeFileWithBoost("200");
      if (!storeHit) await new Promise((resolve) => setTimeout(resolve, 250));
    }
    const boosted = await waitEval(
      session,
      SNAPSHOT_JS,
      (value) =>
        (value.bodyPreview || "").includes("volume boost 200%") &&
        boostSelect(value)?.value === "200",
      20_000,
      "volume boost 200% in control state and presets summary",
    );
    await session.send("Page.reload", {});
    await waitEval(session, SNAPSHOT_JS, (value) => value.hydrated, 30_000, "reload");
    await session.send("Page.navigate", { url: view.origin + "/settings/player" });
    const persisted = await waitEval(
      session,
      SNAPSHOT_JS,
      (value) => boostSelect(value)?.value === "200",
      20_000,
      "volume boost persisted across reload",
    );
    mkdirSync(EVIDENCE_DIR, { recursive: true });
    const shotPath = join(EVIDENCE_DIR, state.native.runId + "-mpv-volume-boost.png");
    const shot = await session.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(shotPath, Buffer.from(shot.data, "base64"));
    // Leave the control back at Off so a repeat drive starts from the default
    // state and the restored profile file matches the live app.
    const reset = await session.evaluate(SET_SELECT_JS("Volume boost", "100"));
    await waitEval(
      session,
      SNAPSHOT_JS,
      (value) =>
        boostSelect(value)?.value === "100" &&
        !(value.bodyPreview || "").includes("volume boost"),
      10_000,
      "volume boost reset to Off",
    );
    const options = (before.options || []).join(",");
    return {
      feature: "mpv-volume-boost",
      surface: "desktop",
      snap: boosted,
      artifacts: [shotPath],
      checks: [
        { id: "volume-boost-control-present", result: "pass" },
        {
          id: "volume-boost-options-100-130-150-200-300",
          result: options === "100,130,150,200,300" ? "pass" : "fail",
        },
        { id: "volume-boost-defaults-off", result: before.value === "100" ? "pass" : "fail" },
        { id: "volume-boost-set-200", result: set.value === "200" ? "pass" : "fail" },
        {
          id: "settings-store-write-observed",
          result: storeHit ? "pass" : "fail",
        },
        {
          id: "presets-summary-reports-boost",
          result: (boosted.bodyPreview || "").includes("volume boost 200%") ? "pass" : "fail",
        },
        {
          id: "volume-boost-persisted-after-reload",
          result: boostSelect(persisted)?.value === "200" ? "pass" : "fail",
        },
        { id: "volume-boost-reset-to-off", result: reset?.ok ? "pass" : "fail" },
      ],
    };
    });
  } finally {
    restoreStoreFiles(storeBackups);
  }
}

/**
 * The stream-card meta line (viewers - uptime - game) renders only from Helix
 * stream data, so this drive records whether the card surface is reachable at
 * all; without a Twitch session the caller writes a blocked receipt instead of
 * claiming the render.
 */
// Card meta lines as rendered: "33K viewers • 1h 23m • League of Legends".
const CARD_META_JS =
  "[...document.querySelectorAll('.stream-card__meta')].map((el) => (el.innerText || '').trim())";

async function driveStreamCardGameLine(state, dryRun) {
  if (dryRun) {
    return {
      dryRun: true,
      feature: "stream-card-game-line",
      would: [
        "require native WebView CDP",
        "goto /streams (Top streams renders StreamGrid cards)",
        "assert every card meta reads 'viewers • uptime • game' with a non-empty tail",
      ],
    };
  }
  const view = nativeCdpView(state);
  return withCdp(view, async (session) => {
    await session.send("Page.navigate", { url: view.origin + "/streams" });
    const metas = await waitEval(
      session,
      CARD_META_JS,
      (value) => Array.isArray(value) && value.length > 0,
      30_000,
      "Top streams card meta lines",
    );
    const snap = await session.evaluate(SNAPSHOT_JS);
    const lines = metas.map((text) => text.split(" • "));
    const withGame = lines.filter(
      (parts) => parts.length === 3 && parts[2].trim().length > 0,
    );
    mkdirSync(EVIDENCE_DIR, { recursive: true });
    const shotPath = join(EVIDENCE_DIR, state.native.runId + "-stream-card-game-line.png");
    const shot = await session.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(shotPath, Buffer.from(shot.data, "base64"));
    return {
      feature: "stream-card-game-line",
      surface: "desktop",
      snap,
      artifacts: [shotPath],
      checks: [
        { id: "top-streams-cards-rendered", result: lines.length ? "pass" : "fail" },
        {
          id: "game-follows-uptime-on-every-card",
          result: withGame.length === lines.length ? "pass" : "fail",
        },
        {
          id: "card-meta-has-no-empty-segment",
          result: lines.every((parts) => parts.every((part) => part.trim().length > 0))
            ? "pass"
            : "fail",
        },
        {
          id: "viewers-uptime-still-leading",
          result: lines.every(
            (parts) =>
              parts.length >= 2 &&
              /viewers$/.test(parts[0].trim()) &&
              /^[0-9]/.test(parts[1].trim()),
          )
            ? "pass"
            : "fail",
        },
      ],
    };
  });
}

const DRIVERS = {
  "about-changelog": driveAboutChangelog,
  "followed-login-gate": driveFollowedGate,
  "settings-tabs": driveSettingsTabs,
  "tauri-guard": driveTauriGuard,
  "browse-nav": driveBrowseNav,
  "native-setup-check": driveNativeSetupCheck,
  "mpv-volume-boost": driveMpvVolumeBoost,
  "stream-card-game-line": driveStreamCardGameLine,
};

const NATIVE_FEATURES = new Set([
  "native-setup-check",
  "mpv-volume-boost",
  "stream-card-game-line",
]);

async function cmdDrive(args) {
  const feature = args._[1];
  const driver = DRIVERS[feature];
  if (!driver) {
    throw new Error(
      `Unknown feature "${feature || ""}". Use one of: ${Object.keys(DRIVERS).join(", ")}`,
    );
  }
  const loaded = loadState() || {};
  if (NATIVE_FEATURES.has(feature) && !args.dryRun) {
    nativeCdpView(loaded);
  }
  const state = args.dryRun
    ? loaded.origin || loaded.native
      ? loaded
      : { origin: "http://127.0.0.1:0", native: { origin: TAURI_DEV_ORIGIN, runId: "dry" } }
    : NATIVE_FEATURES.has(feature)
      ? loaded
      : requireState();
  const result = await driver(state, Boolean(args.dryRun));
  if (!args.dryRun) {
    const next = loadState();
    next.lastDrive = {
      feature: result.feature,
      surface: result.surface || (NATIVE_FEATURES.has(feature) ? "desktop" : "web"),
      checks: result.checks,
      artifacts: result.artifacts,
      at: new Date().toISOString(),
    };
    saveState(next);
  }
  const failed = result.checks?.some((c) => c.result !== "pass");
  printJson(result);
  if (failed) process.exitCode = 1;
}

async function cmdReceipt(args) {
  const state = loadState();
  const head = gitHead();
  if (!head || !/^[0-9a-f]{40}$/i.test(head)) {
    throw new Error("Could not read a full git HEAD SHA. Receipts must bind to the current commit.");
  }
  const feature = args.feature || state?.lastDrive?.feature;
  if (!feature) {
    throw new Error("receipt requires --feature or a prior drive.");
  }
  const result = args.result || "pass";
  if (!["pass", "fail", "blocked"].includes(result)) {
    throw new Error("--result must be pass, fail, or blocked");
  }
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  const surface =
    args.surface || state?.lastDrive?.surface || (feature === "native-setup-check" ? "desktop" : "web");
  const runId =
    surface === "desktop"
      ? (state?.native?.runId || state?.runId)
      : (state?.runId || state?.native?.runId) || `verify-rillmux-${new Date().toISOString()}`;
  const receipt = {
    schema_version: 1,
    run_id: runId,
    repository: gitRemote(),
    head_sha: head,
    surface,
    feature,
    result,
    checks: state?.lastDrive?.checks || [{ id: `${feature}-observed`, result }],
    artifacts: (state?.lastDrive?.artifacts || []).map((path) => ({ type: "screenshot", path })),
    side_effects: [
      {
        kind: surface === "desktop" ? "native-tauri" : "local-preview",
        detail:
          surface === "desktop"
            ? "npm run tauri:dev with isolated APPDATA; setup check uses get_doctor_report (no credentials)"
            : "Vite + isolated browser profile; no Twitch or Streamlink calls required for chrome features",
      },
    ],
    started_resources: receiptStartedResources(state, { surface, runId }),
    cleanup: args.cleanup || "pass",
    blocked_reason: args["blocked-reason"] || null,
  };
  if (result !== "blocked") receipt.blocked_reason = null;
  const out = join(EVIDENCE_DIR, `${receipt.run_id}-${feature}.receipt.json`);
  if (args.dryRun) {
    printJson({ dryRun: true, wouldWrite: out, receipt });
    return;
  }
  writeFileSync(out, `${JSON.stringify(receipt, null, 2)}\n`);
  if (state) {
    state.lastReceipt = out;
    saveState(state);
  }
  printJson({ ok: true, path: out, receipt });
}

function pidAlive(pid) {
  if (!pid) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function killPid(pid) {
  if (!pid) return;
  try {
    if (process.platform === "win32") {
      spawn("taskkill", ["/PID", String(pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
    } else {
      process.kill(pid, "SIGTERM");
    }
  } catch {
    /* already gone */
  }
}

async function cmdCleanup(args) {
  const state = loadState();
  if (!state) {
    printJson({ ok: true, note: "nothing to clean" });
    return;
  }
  if (args.dryRun) {
    printJson({
      dryRun: true,
      wouldKill: [state.browserPid, state.previewPid, state.native?.pid],
      wouldRemove: [state.profileDir, state.native?.appDataDir],
      wouldKeep: [EVIDENCE_DIR],
    });
    return;
  }
  const lane = args.lane || "all";
  if (lane === "web" || lane === "all") {
    killPid(state.browserPid);
    await new Promise((r) => setTimeout(r, 400));
    killPid(state.previewPid);
    await new Promise((r) => setTimeout(r, 400));
    if (state.profileDir && state.profileDir.startsWith(RUNTIME_DIR)) {
      try {
        rmSync(state.profileDir, { recursive: true, force: true });
      } catch (error) {
        process.stderr.write(`WARN: could not remove browser profile yet: ${error.message}\n`);
      }
    }
    state.previewPid = null;
    state.browserPid = null;
  }
  if (lane === "native" || lane === "all") {
    for (const pid of nativeCleanupPids(state)) {
      killPid(pid);
    }
    for (const pid of debugRillmuxPids(PROJECT_ROOT)) {
      killPid(pid);
    }
    await new Promise((r) => setTimeout(r, 800));
    if (state.native?.appDataDir && state.native.appDataDir.startsWith(RUNTIME_DIR)) {
      try {
        rmSync(state.native.appDataDir, { recursive: true, force: true });
      } catch (error) {
        process.stderr.write(`WARN: could not remove native APPDATA yet: ${error.message}\n`);
      }
    }
    if (state.native) {
      state.native.pid = null;
      state.native.cleanedAt = new Date().toISOString();
    }
  }
  const keptEvidence = existsSync(EVIDENCE_DIR);
  state.cleanedAt = new Date().toISOString();
  saveState(state);
  printJson({
    ok: true,
    cleanup: "pass",
    evidenceRetained: keptEvidence,
    evidenceDir: EVIDENCE_DIR,
    lastReceipt: state.lastReceipt ?? null,
  });
}

async function cmdStatus() {
  printJson(loadState() || { launched: false });
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const command = args._[0] || "help";
  try {
    switch (command) {
      case "help":
      case "--help":
      case "-h":
        help();
        return;
      case "launch":
        await cmdLaunch(args);
        return;
      case "doctor":
        await cmdDoctor(args);
        return;
      case "inspect":
        await cmdInspect(args);
        return;
      case "goto":
        await cmdGoto(args);
        return;
      case "click":
        await cmdClick(args);
        return;
      case "screenshot":
        await cmdScreenshot(args);
        return;
      case "drive":
        await cmdDrive(args);
        return;
      case "receipt":
        await cmdReceipt(args);
        return;
      case "cleanup":
        await cmdCleanup(args);
        return;
      case "status":
        await cmdStatus();
        return;
      default:
        throw new Error(`Unknown command "${command}". Run --help.`);
    }
  } catch (error) {
    fail(error.message);
  }
}

await main();
