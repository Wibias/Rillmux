import { execFileSync } from "node:child_process";
import { win32 } from "node:path";
import {
  WINDOWS_POWERSHELL_MODULES_PATH,
  WINDOWS_POWERSHELL_PATH,
} from "./native-isolation.mjs";

const RUN_ID_PATTERN = /^verify-rillmux-native-\d{8}T\d{6}Z$/;

const PROCESS_TABLE_SCRIPT = String.raw`$ErrorActionPreference = 'Stop'
$items = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, CommandLine, @{ Name = 'CreationDateUtc'; Expression = { if ($_.CreationDate) { $_.CreationDate.ToUniversalTime().ToString('o') } else { $null } } }
$items | ConvertTo-Json -Depth 3 -Compress`;

function blocked(reasonCode, reason, details = {}) {
  return {
    verified: false,
    reasonCode,
    reason,
    rootPid: null,
    listenerPids: [],
    killPids: [],
    ...details,
  };
}

function normalized(value) {
  return String(value ?? "").replaceAll("/", "\\").toLowerCase();
}

function processMap(processes) {
  return new Map(
    (Array.isArray(processes) ? processes : [])
      .filter((process) => Number.isInteger(Number(process?.ProcessId)))
      .map((process) => [Number(process.ProcessId), process]),
  );
}

function descendantsOf(processes, rootPid) {
  const byParent = new Map();
  for (const process of processes) {
    const parent = Number(process.ParentProcessId);
    if (!byParent.has(parent)) byParent.set(parent, []);
    byParent.get(parent).push(Number(process.ProcessId));
  }
  const result = new Set();
  const queue = [Number(rootPid)];
  while (queue.length) {
    const parent = queue.shift();
    for (const child of byParent.get(parent) ?? []) {
      if (!result.has(child)) {
        result.add(child);
        queue.push(child);
      }
    }
  }
  return [...result];
}

function pathMatches(value, expected) {
  return normalized(value).includes(normalized(expected));
}

function isTauriDevOrigin(value) {
  try {
    const url = new URL(String(value));
    return (
      url.protocol === "http:" &&
      url.port === "1420" &&
      ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname)
    );
  } catch {
    return false;
  }
}

export function readNativeProcessTable() {
  if (process.platform !== "win32") return [];
  const output = execFileSync(
    WINDOWS_POWERSHELL_PATH,
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", PROCESS_TABLE_SCRIPT],
    {
      encoding: "utf8",
      timeout: 20_000,
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
      env: {
        ...process.env,
        PSModulePath: WINDOWS_POWERSHELL_MODULES_PATH,
      },
    },
  );
  const parsed = JSON.parse(String(output).trim() || "[]");
  return Array.isArray(parsed) ? parsed : [parsed];
}

export function verifyNativeProcessOwnership({
  native,
  processes,
  listenerPids = [],
  runtimeDir,
  projectRoot,
  currentHead,
  isolation,
  requireCdp = true,
  requireIsolation = true,
  requireCurrentHead = true,
  requireRecordedRootCreatedAt = true,
} = {}) {
  if (!native || typeof native !== "object") {
    return blocked("NATIVE_STATE_MISSING", "No recorded native verifier state exists.");
  }
  if (!RUN_ID_PATTERN.test(String(native.runId ?? ""))) {
    return blocked("NATIVE_STATE_INVALID", "The native run ID is not verifier-owned.");
  }
  const rootPid = Number(native.pid);
  const cdpPort = Number(native.cdpPort);
  if (!Number.isInteger(rootPid) || rootPid <= 0) {
    return blocked("NATIVE_STATE_INVALID", "The recorded native root PID is invalid.");
  }
  if (!Number.isInteger(cdpPort) || cdpPort <= 0 || cdpPort > 65535) {
    return blocked("NATIVE_STATE_INVALID", "The recorded native CDP port is invalid.");
  }
  if (!isTauriDevOrigin(native.origin)) {
    return blocked("NATIVE_STATE_INVALID", "The recorded native origin is not the local Tauri dev origin.");
  }
  if (!Array.isArray(native.ownedPids) || !native.ownedPids.map(Number).includes(rootPid)) {
    return blocked("NATIVE_OWNERSHIP_INCOMPLETE", "The native root PID is not recorded as verifier-owned.");
  }

  const expectedOverlay = win32.join(runtimeDir, `${native.runId}.tauri-cdp.json`);
  const expectedAppData = win32.join(runtimeDir, `native-appdata-${native.runId}`);
  if (normalized(native.overlayPath) !== normalized(expectedOverlay)) {
    return blocked("NATIVE_STATE_INVALID", "The native overlay path does not belong to this verifier run.");
  }
  if (normalized(native.appDataDir) !== normalized(expectedAppData)) {
    return blocked("NATIVE_STATE_INVALID", "The native APPDATA path does not belong to this verifier run.");
  }
  if (requireCurrentHead && (!currentHead || native.headSha !== currentHead)) {
    return blocked("NATIVE_STATE_STALE", "The native state was created for a different Git HEAD.");
  }
  if (requireIsolation) {
    if (isolation?.nativeIsolationVerified !== true || isolation?.status !== "VERIFIED") {
      return blocked("NATIVE_ISOLATION_BLOCKED", "Current Windows user isolation is not verified.");
    }
    if (
      native.isolation?.status !== "VERIFIED" ||
      native.isolation?.currentUserSidSha256 !== isolation.currentUserSidSha256 ||
      native.isolation?.kind !== isolation.isolationKind ||
      native.isolation?.attestationExpiresAt !== isolation.attestationExpiresAt
    ) {
      return blocked("NATIVE_ISOLATION_STALE", "The runtime isolation record is stale or belongs to another identity.");
    }
  }

  const records = Array.isArray(processes) ? processes : [];
  const byPid = processMap(records);
  const root = byPid.get(rootPid);
  if (!root) {
    return blocked("NATIVE_ROOT_NOT_RUNNING", "The recorded verifier root process is not running.");
  }
  const tauriCli = win32.join(projectRoot, "node_modules", "@tauri-apps", "cli", "tauri.js");
  if (
    String(root.Name).toLowerCase() !== "node.exe" ||
    !pathMatches(root.CommandLine, tauriCli) ||
    !pathMatches(root.CommandLine, expectedOverlay)
  ) {
    return blocked("NATIVE_ROOT_OWNERSHIP_MISMATCH", "The recorded PID is not running this verifier's tauri:dev command.");
  }
  const rootStartedAt = Date.parse(root.CreationDateUtc);
  const launchedAt = Date.parse(native.startedAt);
  if (!Number.isFinite(rootStartedAt) || !Number.isFinite(launchedAt)) {
    return blocked("NATIVE_ROOT_IDENTITY_UNPROVEN", "The verifier root process creation time is unavailable.");
  }
  if (rootStartedAt < launchedAt - 60_000 || rootStartedAt > launchedAt + 10 * 60_000) {
    return blocked("NATIVE_PID_REUSED", "The recorded PID was created outside this verifier launch window.");
  }
  const recordedRootStartedAt = Date.parse(native.rootCreatedAt);
  if (requireRecordedRootCreatedAt && !Number.isFinite(recordedRootStartedAt)) {
    return blocked(
      "NATIVE_ROOT_IDENTITY_UNPROVEN",
      "The native state does not contain the recorded root process creation time.",
    );
  }
  if (
    Number.isFinite(recordedRootStartedAt) &&
    Math.abs(rootStartedAt - recordedRootStartedAt) > 1_000
  ) {
    return blocked("NATIVE_PID_REUSED", "The recorded PID belongs to a different process instance.");
  }

  const descendantPids = descendantsOf(records, rootPid);
  const killPids = [...new Set([rootPid, ...descendantPids])].sort((a, b) => a - b);
  const uniqueListeners = [...new Set(listenerPids.map(Number).filter((pid) => Number.isInteger(pid) && pid > 0))];

  if (requireCdp) {
    if (uniqueListeners.length !== 1) {
      return blocked(
        "NATIVE_CDP_OWNERSHIP_AMBIGUOUS",
        "Exactly one CDP listener PID is required for safe native attachment.",
        { rootPid, listenerPids: uniqueListeners, killPids },
      );
    }
    const listenerPid = uniqueListeners[0];
    if (!descendantPids.includes(listenerPid)) {
      return blocked(
        "NATIVE_CDP_NOT_OWNED",
        "The CDP listener is not a descendant of the recorded verifier process.",
        { rootPid, listenerPids: uniqueListeners, killPids: [] },
      );
    }
    const listener = byPid.get(listenerPid);
    if (!listener || String(listener.Name).toLowerCase() !== "msedgewebview2.exe") {
      return blocked(
        "NATIVE_CDP_NOT_OWNED",
        "The CDP listener is not the verifier WebView2 process tree.",
        { rootPid, listenerPids: uniqueListeners, killPids: [] },
      );
    }
  }

  return {
    verified: true,
    reasonCode: "VERIFIED",
    reason: "Native process ownership is proven by the recorded run ID, exact tauri:dev command, live process ancestry and unique CDP listener.",
    rootPid,
    listenerPids: uniqueListeners,
    killPids,
    rootCreatedAt: root.CreationDateUtc ?? null,
  };
}

export function planNativeCleanup({
  native,
  processes,
  runtimeDir,
  projectRoot,
} = {}) {
  if (!native) return { status: "nothing" };
  if (!RUN_ID_PATTERN.test(String(native.runId ?? ""))) {
    return {
      status: "blocked",
      reasonCode: "NATIVE_STATE_INVALID",
      reason: "The native run ID is not verifier-owned.",
      killPids: [],
    };
  }
  const expectedAppData = win32.join(runtimeDir, `native-appdata-${native.runId}`);
  if (normalized(native.appDataDir) !== normalized(expectedAppData)) {
    return {
      status: "blocked",
      reasonCode: "NATIVE_APPDATA_NOT_OWNED",
      reason: "The native APPDATA path does not belong to this verifier run.",
      killPids: [],
    };
  }
  const rootPid = Number(native.pid);
  const records = Array.isArray(processes) ? processes : [];
  const rootRunning = Number.isInteger(rootPid) && records
    .some((process) => Number(process.ProcessId) === rootPid);
  if (!rootRunning) {
    const recordedLive = (Array.isArray(native.ownedPids) ? native.ownedPids : [])
      .map(Number)
      .filter((pid) => pid && records.some((process) => Number(process.ProcessId) === pid));
    if (recordedLive.length > 0) {
      return {
        status: "blocked",
        reasonCode: "NATIVE_ORPHANED_OWNED_PIDS",
        reason: "Recorded native descendants are alive without their recorded root; ownership cannot be recovered safely.",
        appDataDir: expectedAppData,
        livePids: recordedLive,
        killPids: [],
      };
    }
    return {
      status: "inactive",
      appDataDir: expectedAppData,
      killPids: [],
    };
  }
  const ownership = verifyNativeProcessOwnership({
    native,
    processes,
    runtimeDir,
    projectRoot,
    requireCdp: false,
    requireIsolation: false,
    requireCurrentHead: false,
  });
  if (!ownership.verified) {
    return {
      status: "blocked",
      appDataDir: expectedAppData,
      ownership,
      killPids: [],
    };
  }
  return {
    status: "ready",
    appDataDir: expectedAppData,
    ownership,
    killPids: ownership.killPids,
  };
}

export function evaluateNativeCdpAccess({ isolation, ownership } = {}) {
  if (isolation?.nativeIsolationVerified !== true || isolation?.status !== "VERIFIED") {
    return {
      verified: false,
      reasonCode: isolation?.reasonCode || "NATIVE_ISOLATION_BLOCKED",
      reason: isolation?.reason || "Native isolation is not currently verified.",
    };
  }
  if (ownership?.verified !== true) {
    return {
      verified: false,
      reasonCode: ownership?.reasonCode || "NATIVE_OWNERSHIP_BLOCKED",
      reason: ownership?.reason || "Native process ownership is not proven.",
    };
  }
  return {
    verified: true,
    reasonCode: "VERIFIED",
    reason: "Current isolation and unique native verifier process ownership are proven.",
  };
}

export function nativeReadinessVerified({
  isolation,
  ownership,
  tauriProcessAlive = false,
  cdpListOk = false,
  pageTarget = null,
  snapshotReady = false,
  mismatch = null,
} = {}) {
  return (
    evaluateNativeCdpAccess({ isolation, ownership }).verified === true &&
    tauriProcessAlive === true &&
    cdpListOk === true &&
    typeof pageTarget === "string" &&
    pageTarget.length > 0 &&
    snapshotReady === true &&
    !mismatch
  );
}

export function selectCdpLane({ requested, state } = {}) {
  if (requested === "native") return "native";
  if (requested && requested !== "web") {
    throw new Error('--lane must be "web" or "native"');
  }
  if (!state?.origin || !state?.cdpPort) {
    throw new Error(
      "No web runtime state. Run: node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs launch",
    );
  }
  return "web";
}
