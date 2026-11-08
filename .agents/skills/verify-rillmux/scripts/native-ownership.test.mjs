import assert from "node:assert/strict";
import { win32 } from "node:path";
import { describe, it } from "node:test";
import {
  evaluateNativeCdpAccess,
  nativeReadinessVerified,
  planNativeCleanup,
  readNativeProcessTable,
  selectCdpLane,
  terminateVerifiedNativeProcesses,
  verifyNativeProcessOwnership,
  windowsNativeTerminateArgs,
} from "./native-ownership.mjs";

const runtimeDir = "C:\\project\\.agents\\skills\\verify-rillmux\\.runtime";
const projectRoot = "C:\\project";
const runId = "verify-rillmux-native-20261108T010203Z";
const overlayPath = win32.join(runtimeDir, `${runId}.tauri-cdp.json`);
const appDataDir = win32.join(runtimeDir, `native-appdata-${runId}`);
const headSha = "044210db7947e849e64c6dbf0c7ec2068a33f29a";
const isolation = {
  status: "VERIFIED",
  nativeIsolationVerified: true,
  isolationKind: "separate-windows-user",
  currentUserSidSha256: "a".repeat(64),
  attestationExpiresAt: "2026-12-01T00:00:00.000Z",
};

function nativeState(overrides = {}) {
  return {
    runId,
    pid: 100,
    cdpPort: 18749,
    origin: "http://localhost:1420",
    headSha,
    startedAt: "2026-11-08T01:00:00.000Z",
    rootCreatedAt: "2026-11-08T01:00:00.000Z",
    overlayPath,
    appDataDir,
    ownedPids: [100, 200, 300],
    isolation: {
      status: "VERIFIED",
      kind: "separate-windows-user",
      currentUserSidSha256: isolation.currentUserSidSha256,
      attestationExpiresAt: isolation.attestationExpiresAt,
    },
    ...overrides,
  };
}

function processTable({
  rootCommand = null,
  listenerParent = 200,
  rootCreatedAt = "2026-11-08T01:00:00.000Z",
  cargoCreatedAt = "2026-11-08T01:00:01.000Z",
  listenerCreatedAt = "2026-11-08T01:00:02.000Z",
} = {}) {
  return [
    {
      ProcessId: 100,
      ParentProcessId: 1,
      Name: "node.exe",
      CommandLine:
        rootCommand ??
        `"C:\\node.exe" "C:\\project\\node_modules\\@tauri-apps\\cli\\tauri.js" dev --config "${overlayPath}"`,
      CreationDateUtc: rootCreatedAt,
    },
    {
      ProcessId: 200,
      ParentProcessId: 100,
      Name: "cargo.exe",
      CommandLine: "cargo",
      CreationDateUtc: cargoCreatedAt,
    },
    {
      ProcessId: 300,
      ParentProcessId: listenerParent,
      Name: "msedgewebview2.exe",
      CommandLine: "msedgewebview2 --type=browser",
      CreationDateUtc: listenerCreatedAt,
    },
  ];
}

function verify(overrides = {}) {
  return verifyNativeProcessOwnership({
    native: nativeState(),
    processes: processTable(),
    listenerPids: [300],
    runtimeDir,
    projectRoot,
    currentHead: headSha,
    isolation,
    ...overrides,
  });
}

describe("native verifier process ownership", () => {
  it("reads the live Windows process table without requiring credential access", { skip: process.platform !== "win32" }, () => {
    const processes = readNativeProcessTable();
    assert.ok(processes.some((entry) => Number(entry.ProcessId) === process.pid));
  });

  it("accepts the exact recorded tauri:dev tree and its unique WebView2 CDP listener", () => {
    const result = verify();
    assert.equal(result.verified, true);
    assert.equal(result.reasonCode, "VERIFIED");
    assert.deepEqual(result.killPids, [100, 200, 300]);
  });

  it("rejects a listener outside the recorded process tree", () => {
    const result = verify({ listenerPids: [400] });
    assert.equal(result.verified, false);
    assert.equal(result.reasonCode, "NATIVE_CDP_NOT_OWNED");
    assert.deepEqual(result.killPids, []);
  });

  it("rejects ambiguous multiple CDP listener ownership", () => {
    const result = verify({
      processes: [
        ...processTable(),
        {
          ProcessId: 301,
          ParentProcessId: 200,
          Name: "msedgewebview2.exe",
          CreationDateUtc: "2026-11-08T01:00:02.000Z",
        },
      ],
      listenerPids: [300, 301],
    });
    assert.equal(result.verified, false);
    assert.equal(result.reasonCode, "NATIVE_CDP_OWNERSHIP_AMBIGUOUS");
  });

  it("rejects stale HEAD and stale identity/isolation records", () => {
    assert.equal(verify({ currentHead: "f".repeat(40) }).reasonCode, "NATIVE_STATE_STALE");
    assert.equal(
      verify({ isolation: { ...isolation, currentUserSidSha256: "b".repeat(64) } }).reasonCode,
      "NATIVE_ISOLATION_STALE",
    );
    assert.equal(
      verify({ isolation: { ...isolation, nativeIsolationVerified: false, status: "BLOCKED" } }).reasonCode,
      "NATIVE_ISOLATION_BLOCKED",
    );
    assert.equal(
      verify({ native: nativeState({ origin: "http://example.com:1420" }) }).reasonCode,
      "NATIVE_STATE_INVALID",
    );
  });

  it("rejects PID reuse and missing recorded root creation evidence", () => {
    assert.equal(
      verify({ native: nativeState({ rootCreatedAt: "2026-11-08T01:05:00.000Z" }) }).reasonCode,
      "NATIVE_PID_REUSED",
    );
    assert.equal(
      verify({ processes: processTable({ rootCreatedAt: "2026-11-08T00:00:00.000Z" }) }).reasonCode,
      "NATIVE_PID_REUSED",
    );
    assert.equal(
      verify({ processes: processTable({ rootCreatedAt: null }) }).reasonCode,
      "NATIVE_ROOT_IDENTITY_UNPROVEN",
    );
    assert.equal(
      verify({ native: nativeState({ rootCreatedAt: undefined }) }).reasonCode,
      "NATIVE_ROOT_IDENTITY_UNPROVEN",
    );
  });

  it("rejects a same-path root process without this run's exact overlay argument", () => {
    const result = verify({
      processes: processTable({
        rootCommand:
          '"C:\\node.exe" "C:\\project\\node_modules\\@tauri-apps\\cli\\tauri.js" dev --config "C:\\other\\run.json"',
      }),
    });
    assert.equal(result.verified, false);
    assert.equal(result.reasonCode, "NATIVE_ROOT_OWNERSHIP_MISMATCH");
    assert.deepEqual(result.killPids, []);
  });

  it("rejects descendant identities older than their verified root or parent", () => {
    const oldParent = verify({ processes: processTable({ cargoCreatedAt: "2026-11-08T00:59:00.000Z" }) });
    assert.equal(oldParent.verified, false);
    assert.equal(oldParent.reasonCode, "NATIVE_DESCENDANT_IDENTITY_UNPROVEN");
    assert.deepEqual(oldParent.killPids, []);

    const oldChild = verify({
      processes: processTable({
        cargoCreatedAt: "2026-11-08T01:00:03.000Z",
        listenerCreatedAt: "2026-11-08T01:00:01.000Z",
      }),
    });
    assert.equal(oldChild.verified, false);
    assert.equal(oldChild.reasonCode, "NATIVE_DESCENDANT_IDENTITY_UNPROVEN");
    assert.deepEqual(oldChild.killPids, []);
  });

  it("rejects descendants with missing creation evidence", () => {
    const processes = processTable();
    processes.find((entry) => entry.ProcessId === 200).CreationDateUtc = null;
    const result = verify({ processes });
    assert.equal(result.verified, false);
    assert.equal(result.reasonCode, "NATIVE_DESCENDANT_IDENTITY_UNPROVEN");
    assert.deepEqual(result.killPids, []);
  });

  it("supports owner-verified cleanup without requiring a live isolation session", () => {
    const cleanup = verify({
      requireCdp: false,
      requireIsolation: false,
      requireCurrentHead: false,
      native: nativeState({ isolation: null, headSha: "old" }),
      isolation: null,
    });
    assert.equal(cleanup.verified, true);
    assert.deepEqual(cleanup.killPids, [100, 200, 300]);
  });
});

describe("explicit CDP lane selection", () => {
  it("never falls back from missing web state to a persisted native target", () => {
    assert.throws(
      () => selectCdpLane({ requested: undefined, state: { native: { cdpPort: 18749 } } }),
      /No web runtime state/,
    );
    assert.equal(selectCdpLane({ requested: "native", state: {} }), "native");
    assert.equal(
      selectCdpLane({ requested: undefined, state: { origin: "http://127.0.0.1:1", cdpPort: 2 } }),
      "web",
    );
  });
});

describe("native CDP access decision", () => {
  it("requires both current isolation and unique owned process proof", () => {
    const owned = { verified: true, reasonCode: "VERIFIED", reason: "owned" };
    const blockedIsolation = { status: "BLOCKED", nativeIsolationVerified: false, reasonCode: "ATTESTATION_MISSING", reason: "missing" };
    const blockedOwnership = { verified: false, reasonCode: "NATIVE_CDP_NOT_OWNED", reason: "foreign listener" };
    assert.equal(evaluateNativeCdpAccess({ isolation, ownership: owned }).verified, true);
    assert.equal(
      evaluateNativeCdpAccess({ isolation: blockedIsolation, ownership: owned }).reasonCode,
      "ATTESTATION_MISSING",
    );
    assert.equal(
      evaluateNativeCdpAccess({ isolation, ownership: blockedOwnership }).reasonCode,
      "NATIVE_CDP_NOT_OWNED",
    );
  });

  it("reports doctor readiness only for live isolation, ownership, CDP and snapshot", () => {
    const owned = { verified: true, reasonCode: "VERIFIED", reason: "owned" };
    const ready = nativeReadinessVerified({
      isolation,
      ownership: owned,
      tauriProcessAlive: true,
      cdpListOk: true,
      pageTarget: "http://localhost:1420/",
      snapshotReady: true,
    });
    assert.equal(ready, true);
    assert.equal(
      nativeReadinessVerified({
        isolation: { ...isolation, nativeIsolationVerified: false, status: "BLOCKED" },
        ownership: owned,
        tauriProcessAlive: true,
        cdpListOk: true,
        pageTarget: "http://localhost:1420/",
        snapshotReady: true,
      }),
      false,
    );
    assert.equal(
      nativeReadinessVerified({
        isolation,
        ownership: { verified: false, reasonCode: "NATIVE_STATE_STALE", reason: "old" },
        tauriProcessAlive: true,
        cdpListOk: true,
        pageTarget: "http://localhost:1420/",
        snapshotReady: true,
      }),
      false,
    );
    assert.equal(
      nativeReadinessVerified({
        isolation,
        ownership: owned,
        tauriProcessAlive: true,
        cdpListOk: true,
        pageTarget: "http://localhost:1420/",
        snapshotReady: true,
        mismatch: "stale",
      }),
      false,
    );
  });
});

describe("owner-proven native cleanup", () => {
  it("plans only the proven process tree and verifier-owned APPDATA path", () => {
    const plan = planNativeCleanup({
      native: nativeState(),
      processes: processTable(),
      runtimeDir,
      projectRoot,
    });
    assert.equal(plan.status, "ready");
    assert.deepEqual(plan.killPids, [100, 200, 300]);
    assert.equal(plan.appDataDir, appDataDir);
  });

  it("refuses a same-path foreign process without the run's exact overlay", () => {
    const plan = planNativeCleanup({
      native: nativeState(),
      processes: processTable({
        rootCommand:
          '"C:\\node.exe" "C:\\project\\node_modules\\@tauri-apps\\cli\\tauri.js" dev --config "C:\\other\\run.json"',
      }),
      runtimeDir,
      projectRoot,
    });
    assert.equal(plan.status, "blocked");
    assert.equal(plan.ownership.reasonCode, "NATIVE_ROOT_OWNERSHIP_MISMATCH");
    assert.deepEqual(plan.killPids, []);
  });

  it("allows inert verifier state cleanup only within its exact run directory", () => {
    const inactive = planNativeCleanup({
      native: nativeState({ pid: null }),
      processes: [],
      runtimeDir,
      projectRoot,
    });
    assert.equal(inactive.status, "inactive");
    assert.deepEqual(inactive.killPids, []);
    assert.equal(inactive.appDataDir, appDataDir);

    const forged = planNativeCleanup({
      native: nativeState({ pid: null, appDataDir: "C:\\Users\\foreign\\Rillmux" }),
      processes: [],
      runtimeDir,
      projectRoot,
    });
    assert.equal(forged.status, "blocked");
    assert.equal(forged.reasonCode, "NATIVE_APPDATA_NOT_OWNED");
  });

  it("refuses to kill recorded descendants after the owner root disappears", () => {
    const plan = planNativeCleanup({
      native: nativeState(),
      processes: [
        {
          ProcessId: 200,
          ParentProcessId: 1,
          Name: "cargo.exe",
          CreationDateUtc: "2026-11-08T01:00:01.000Z",
        },
        {
          ProcessId: 300,
          ParentProcessId: 200,
          Name: "msedgewebview2.exe",
          CreationDateUtc: "2026-11-08T01:00:02.000Z",
        },
      ],
      runtimeDir,
      projectRoot,
    });
    assert.equal(plan.status, "blocked");
    assert.equal(plan.reasonCode, "NATIVE_ORPHANED_OWNED_PIDS");
    assert.deepEqual(plan.killPids, []);
  });

  it("blocks cleanup when the root PID was reused or a parent PID is stale", () => {
    const reusedRoot = planNativeCleanup({
      native: nativeState(),
      processes: processTable({ rootCreatedAt: "2026-11-08T01:05:00.000Z" }),
      runtimeDir,
      projectRoot,
    });
    assert.equal(reusedRoot.status, "blocked");
    assert.equal(reusedRoot.killPids.length, 0);

    const staleParent = planNativeCleanup({
      native: nativeState(),
      processes: processTable({ cargoCreatedAt: "2026-11-08T00:59:00.000Z" }),
      runtimeDir,
      projectRoot,
    });
    assert.equal(staleParent.status, "blocked");
    assert.equal(staleParent.ownership.reasonCode, "NATIVE_DESCENDANT_IDENTITY_UNPROVEN");
    assert.equal(staleParent.killPids.length, 0);
  });

  it("terminates only verified identities, deepest first, without recursive taskkill", async () => {
    const plan = planNativeCleanup({
      native: nativeState(),
      processes: processTable(),
      runtimeDir,
      projectRoot,
    });
    const calls = [];
    const result = await terminateVerifiedNativeProcesses({
      plan,
      readProcesses: () => processTable(),
      terminatePid: async (pid) => {
        calls.push(pid);
        return { ok: true, exitCode: 0 };
      },
    });
    assert.equal(result.ok, true);
    assert.deepEqual(calls, [300, 200, 100]);
    assert.deepEqual(result.terminatedPids, [300, 200, 100]);
    assert.deepEqual(windowsNativeTerminateArgs(42), ["/PID", "42", "/F"]);
    assert.equal(windowsNativeTerminateArgs(42).includes("/T"), false);
  });

  it("blocks before the first kill when a planned PID was reused", async () => {
    const plan = planNativeCleanup({
      native: nativeState(),
      processes: processTable(),
      runtimeDir,
      projectRoot,
    });
    const reused = processTable({ listenerCreatedAt: "2026-11-08T01:00:05.000Z" });
    const calls = [];
    const result = await terminateVerifiedNativeProcesses({
      plan,
      readProcesses: () => reused,
      terminatePid: async (pid) => {
        calls.push(pid);
        return { ok: true, exitCode: 0 };
      },
    });
    assert.equal(result.ok, false);
    assert.equal(result.reasonCode, "NATIVE_PID_REUSED_OR_UNPROVEN");
    assert.deepEqual(result.terminatedPids, []);
    assert.deepEqual(calls, []);
  });
});
