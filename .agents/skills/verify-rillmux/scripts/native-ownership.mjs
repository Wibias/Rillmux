import { execFileSync, spawn } from "node:child_process";
import { win32 } from "node:path";
import {
  WINDOWS_POWERSHELL_MODULES_PATH,
  WINDOWS_POWERSHELL_PATH,
  WINDOWS_TASKKILL_PATH,
} from "./native-isolation.mjs";

const RUN_ID_PATTERN = /^verify-rillmux-native-\d{8}T\d{6}Z$/;
const CREATION_TIME_TOLERANCE_MS = 1_000;

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

function createdAtMs(processRecord) {
  const value = Date.parse(processRecord?.CreationDateUtc);
  return Number.isFinite(value) ? value : null;
}

function processIdentity(processRecord) {
  return {
    pid: Number(processRecord.ProcessId),
    parentPid: Number(processRecord.ParentProcessId),
    name: String(processRecord.Name ?? "").toLowerCase(),
    creationDateUtc: processRecord.CreationDateUtc ?? null,
    commandLine: processRecord.CommandLine ?? null,
  };
}

function identityMatches(expected, actual) {
  if (!expected || !actual) return false;
  if (Number(actual.ProcessId) !== expected.pid) return false;
  if (Number(actual.ParentProcessId) !== expected.parentPid) return false;
  if (String(actual.Name ?? "").toLowerCase() !== expected.name) return false;
  const actualCreatedAt = createdAtMs(actual);
  const expectedCreatedAt = Date.parse(expected.creationDateUtc);
  if (
    !Number.isFinite(actualCreatedAt) ||
    !Number.isFinite(expectedCreatedAt) ||
    Math.abs(actualCreatedAt - expectedCreatedAt) > CREATION_TIME_TOLERANCE_MS
  ) {
    return false;
  }
  if (
    expected.commandLine != null &&
    normalized(actual.CommandLine) !== normalized(expected.commandLine)
  ) {
    return false;
  }
  return true;
}

function verifiedDescendants(processes, root) {
  const rootPid = Number(root.ProcessId);
  const rootCreatedAt = createdAtMs(root);
  if (!Number.isFinite(rootCreatedAt)) {
    return {
      verified: false,
      reasonCode: "NATIVE_ROOT_IDENTITY_UNPROVEN",
      reason: "The verifier root process creation time is unavailable.",
    };
  }
  const byPid = processMap(processes);
  const byParent = new Map();
  for (const process of processes) {
    const parent = Number(process.ParentProcessId);
    if (!byParent.has(parent)) byParent.set(parent, []);
    byParent.get(parent).push(Number(process.ProcessId));
  }
  const result = new Set();
  const depthByPid = new Map([[rootPid, 0]]);
  const queue = [rootPid];
  while (queue.length) {
    const parentPid = queue.shift();
    const parent = byPid.get(parentPid);
    const parentCreatedAt = createdAtMs(parent);
    if (!Number.isFinite(parentCreatedAt)) {
      return {
        verified: false,
        reasonCode: "NATIVE_DESCENDANT_IDENTITY_UNPROVEN",
        reason: `Process ${parentPid} has no usable creation time for descendant verification.`,
      };
    }
    for (const child of byParent.get(parentPid) ?? []) {
      if (child === rootPid || depthByPid.has(child)) {
        return {
          verified: false,
          reasonCode: "NATIVE_DESCENDANT_IDENTITY_UNPROVEN",
          reason: `Process ${child} forms an ambiguous or cyclic parent chain.`,
        };
      }
      if (result.has(child)) continue;
      const childProcess = byPid.get(child);
      const childCreatedAt = createdAtMs(childProcess);
      if (!Number.isFinite(childCreatedAt)) {
        return {
          verified: false,
          reasonCode: "NATIVE_DESCENDANT_IDENTITY_UNPROVEN",
          reason: `Process ${child} has no usable creation time for descendant verification.`,
        };
      }
      if (
        childCreatedAt < rootCreatedAt - CREATION_TIME_TOLERANCE_MS ||
        childCreatedAt < parentCreatedAt - CREATION_TIME_TOLERANCE_MS
      ) {
        return {
          verified: false,
          reasonCode: "NATIVE_DESCENDANT_IDENTITY_UNPROVEN",
          reason: `Process ${child} predates its verified parent ${parentPid}; the parent PID is stale or reused.`,
        };
      }
      result.add(child);
      depthByPid.set(child, (depthByPid.get(parentPid) ?? 0) + 1);
      queue.push(child);
    }
  }
  const pids = [...result];
  return {
    verified: true,
    pids,
    terminationOrder: pids
      .sort((left, right) => (depthByPid.get(right) ?? 0) - (depthByPid.get(left) ?? 0) || left - right)
      .map((pid) => byPid.get(pid)),
  };
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

  const descendantTree = verifiedDescendants(records, root);
  if (!descendantTree.verified) {
    return blocked(descendantTree.reasonCode, descendantTree.reason, { rootPid, killPids: [] });
  }
  const descendantPids = descendantTree.pids;
  const killPids = [...new Set([rootPid, ...descendantPids])].sort((a, b) => a - b);
  const processIdentities = [
    processIdentity(root),
    ...descendantTree.terminationOrder.map(processIdentity),
  ];
  const terminationOrder = [
    ...descendantTree.terminationOrder.map((process) => Number(process.ProcessId)),
    rootPid,
  ];
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
    terminationOrder,
    processIdentities,
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
    terminationOrder: ownership.terminationOrder,
    processIdentities: ownership.processIdentities,
  };
}

export function windowsNativeTerminateArgs(pid) {
  return ["/PID", String(pid), "/F"];
}

function terminateWindowsPid(pid) {
  if (process.platform !== "win32") {
    try {
      process.kill(Number(pid), "SIGTERM");
      return Promise.resolve({ ok: true, exitCode: 0 });
    } catch (error) {
      return Promise.resolve({ ok: false, error: String(error?.message ?? error) });
    }
  }
  return new Promise((resolve) => {
    const child = spawn(WINDOWS_TASKKILL_PATH, windowsNativeTerminateArgs(pid), {
      stdio: "ignore",
      windowsHide: true,
    });
    child.once("error", (error) =>
      resolve({ ok: false, error: String(error?.message ?? error) }),
    );
    child.once("exit", (exitCode) => resolve({ ok: exitCode === 0, exitCode }));
  });
}

export async function terminateVerifiedNativeProcesses({
  plan,
  readProcesses = readNativeProcessTable,
  terminatePid = terminateWindowsPid,
} = {}) {
  if (plan?.status !== "ready") {
    return {
      ok: false,
      reasonCode: plan?.reasonCode || "NATIVE_KILL_PLAN_NOT_READY",
      reason: plan?.reason || "Native cleanup ownership was not verified.",
      terminatedPids: [],
    };
  }
  const order = Array.isArray(plan.terminationOrder) ? plan.terminationOrder.map(Number) : [];
  const identities = Array.isArray(plan.processIdentities) ? plan.processIdentities : [];
  const expectedByPid = new Map(identities.map((identity) => [Number(identity.pid), identity]));
  if (
    order.length === 0 ||
    order.length !== identities.length ||
    order.some((pid) => !expectedByPid.has(pid))
  ) {
    return {
      ok: false,
      reasonCode: "NATIVE_KILL_PLAN_INVALID",
      reason: "The native termination plan does not contain one verified identity per PID.",
      terminatedPids: [],
    };
  }

  const terminatedPids = [];
  for (let index = 0; index < order.length; index += 1) {
    let records;
    try {
      records = readProcesses();
    } catch (error) {
      return {
        ok: false,
        reasonCode: "NATIVE_PROCESS_PROBE_FAILED",
        reason: String(error?.message ?? error),
        terminatedPids,
      };
    }
    const byPid = processMap(records);
    for (const pid of order.slice(index)) {
      const expected = expectedByPid.get(pid);
      const actual = byPid.get(pid);
      if (!identityMatches(expected, actual)) {
        return {
          ok: false,
          reasonCode: "NATIVE_PID_REUSED_OR_UNPROVEN",
          reason: `Process ${pid} no longer matches its verified creation identity; no further kill was issued.`,
          blockedPid: pid,
          terminatedPids,
        };
      }
    }
    const pid = order[index];
    const result = await terminatePid(pid);
    if (!result?.ok) {
      return {
        ok: false,
        reasonCode: "NATIVE_TERMINATION_FAILED",
        reason: `Verified process ${pid} could not be terminated: ${result?.error || `exit ${result?.exitCode}`}`,
        blockedPid: pid,
        terminatedPids,
      };
    }
    terminatedPids.push(pid);
  }
  return {
    ok: true,
    reasonCode: "TERMINATED",
    reason: "Only independently verified native verifier PIDs were terminated without recursive taskkill.",
    terminatedPids,
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
