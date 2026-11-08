import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  evaluateNativeIsolation,
  nativeLaunchCapability,
  parseWindowsIsolationProbe,
  probeNativeIsolation,
  readWindowsIsolationProbe,
  runGuardedNativeLaunch,
} from "./native-isolation.mjs";

const PRODUCTION_SID = "S-1-5-21-100-200-300-1001";
const ISOLATED_SID = "S-1-5-21-100-200-300-2002";
const ADMIN_SID = "S-1-5-32-544";
const USERS_SID = "S-1-5-32-545";
const SYSTEM_SID = "S-1-5-18";
const READ_KEY = 131097;
const WRITE_KEY = 131078;
const FULL_CONTROL = 983551;
const GENERIC_ALL = 0x10000000;
const UNKNOWN_WRITE_OR_ALL = 0x02000000;

function attestation(overrides = {}) {
  return {
    version: 1,
    kind: "separate-windows-user",
    productionUserSid: PRODUCTION_SID,
    isolatedUserSid: ISOLATED_SID,
    approvedAt: "2026-11-01T00:00:00.000Z",
    expiresAt: "2026-12-01T00:00:00.000Z",
    approvalReference: "operator-approved verifier account",
    ...overrides,
  };
}

function machineRecord(overrides = {}) {
  return {
    ownerSid: ADMIN_SID,
    areAccessRulesProtected: true,
    rules: [
      { sid: SYSTEM_SID, access: "Allow", rights: FULL_CONTROL },
      { sid: ADMIN_SID, access: "Allow", rights: FULL_CONTROL },
      { sid: USERS_SID, access: "Allow", rights: READ_KEY },
    ],
    ...overrides,
  };
}

function evaluate(overrides = {}) {
  return evaluateNativeIsolation({
    platform: "win32",
    identity: { sid: ISOLATED_SID, isAdmin: false, isElevated: false },
    attestation: attestation(),
    machineRecord: machineRecord(),
    checkedAt: new Date("2026-11-08T00:00:00.000Z"),
    ...overrides,
  });
}

describe("fail-closed Windows native isolation preflight", () => {
  it("blocks the real application user when APPDATA/env claims exist but no machine attestation does", () => {
    const priorIsolation = process.env.RILLMUX_NATIVE_ISOLATED;
    const priorAppData = process.env.APPDATA;
    process.env.RILLMUX_NATIVE_ISOLATED = "1";
    process.env.APPDATA = "C:\\totally-isolated-appdata";
    try {
      const result = evaluate({ attestation: null, machineRecord: null });
      assert.equal(result.nativeIsolationVerified, false);
      assert.equal(result.reasonCode, "ATTESTATION_MISSING");
      assert.equal(result.evidence.appDataOverrideIsCredentialProof, false);
      assert.equal(result.evidence.environmentIsCredentialProof, false);
    } finally {
      if (priorIsolation === undefined) delete process.env.RILLMUX_NATIVE_ISOLATED;
      else process.env.RILLMUX_NATIVE_ISOLATED = priorIsolation;
      if (priorAppData === undefined) delete process.env.APPDATA;
      else process.env.APPDATA = priorAppData;
    }
  });

  it("blocks malformed, stale and unsupported attestations", () => {
    assert.equal(
      evaluate({ attestation: attestation({ version: 2 }) }).reasonCode,
      "ATTESTATION_SCHEMA_UNSUPPORTED",
    );
    assert.equal(
      evaluate({ attestation: attestation({ kind: "renamed-service" }) }).reasonCode,
      "ISOLATION_KIND_UNSUPPORTED",
    );
    assert.equal(
      evaluate({ attestation: attestation({ expiresAt: "2026-11-07T00:00:00.000Z" }) }).reasonCode,
      "ATTESTATION_EXPIRED",
    );
    assert.equal(
      evaluate({ attestation: attestation({ isolatedUserSid: "not-a-sid" }) }).reasonCode,
      "ATTESTATION_INVALID",
    );
  });

  it("blocks the production SID and any non-attested Windows identity", () => {
    assert.equal(
      evaluate({ identity: { sid: PRODUCTION_SID, isAdmin: false } }).reasonCode,
      "CURRENT_USER_NOT_ISOLATED",
    );
    assert.equal(
      evaluate({ identity: { sid: "S-1-5-21-100-200-300-3003", isAdmin: false } }).reasonCode,
      "CURRENT_USER_NOT_ISOLATED",
    );
    assert.equal(
      evaluate({
        attestation: attestation({ isolatedUserSid: PRODUCTION_SID }),
        identity: { sid: PRODUCTION_SID, isAdmin: false },
      }).reasonCode,
      "CURRENT_USER_IS_PRODUCTION",
    );
  });

  it("blocks administrator-group membership and broadly writable HKLM records", () => {
    assert.equal(
      evaluate({ identity: { sid: ISOLATED_SID, isAdmin: true } }).reasonCode,
      "ISOLATED_USER_IS_ADMIN",
    );
    assert.equal(
      evaluate({ identity: { sid: ISOLATED_SID, isAdmin: true, isElevated: false } }).reasonCode,
      "ISOLATED_USER_IS_ADMIN",
    );
    assert.equal(
      evaluate({
        machineRecord: machineRecord({
          rules: [{ sid: USERS_SID, access: "Allow", rights: WRITE_KEY }],
        }),
      }).reasonCode,
      "ATTESTATION_NOT_PROTECTED",
    );
    assert.equal(
      evaluate({
        machineRecord: machineRecord({
          rules: [{ sid: USERS_SID, access: "Allow", rights: GENERIC_ALL }],
        }),
      }).reasonCode,
      "ATTESTATION_NOT_PROTECTED",
    );
    assert.equal(
      evaluate({
        machineRecord: machineRecord({
          rules: [{ sid: USERS_SID, access: "Allow", rights: FULL_CONTROL }],
        }),
      }).reasonCode,
      "ATTESTATION_NOT_PROTECTED",
    );
    assert.equal(
      evaluate({
        machineRecord: machineRecord({
          rules: [{ sid: USERS_SID, access: "Unknown", rights: READ_KEY }],
        }),
      }).reasonCode,
      "ATTESTATION_NOT_PROTECTED",
    );
    assert.equal(
      evaluate({
        machineRecord: machineRecord({
          rules: [{ sid: USERS_SID, access: "Allow", rights: UNKNOWN_WRITE_OR_ALL }],
        }),
      }).reasonCode,
      "ATTESTATION_NOT_PROTECTED",
    );
    assert.equal(
      evaluate({
        machineRecord: machineRecord({
          rules: [{ sid: null, access: "Allow", rights: READ_KEY }],
        }),
      }).reasonCode,
      "ATTESTATION_NOT_PROTECTED",
    );
    assert.equal(
      evaluate({
        machineRecord: machineRecord({
          ownerSid: ISOLATED_SID,
        }),
      }).reasonCode,
      "ATTESTATION_NOT_PROTECTED",
    );
    assert.equal(
      evaluate({
        machineRecord: machineRecord({ areAccessRulesProtected: false }),
      }).reasonCode,
      "ATTESTATION_NOT_PROTECTED",
    );
    assert.equal(
      evaluate({
        machineRecord: machineRecord({
          rules: [{ sid: "S-1-5-21-9-8-7-6", access: "Allow", rights: null }],
        }),
      }).reasonCode,
      "ATTESTATION_NOT_PROTECTED",
    );
  });

  it("verifies only a distinct, attested, non-admin Windows user with a protected HKLM record", () => {
    const result = evaluate();
    assert.equal(result.status, "VERIFIED");
    assert.equal(result.nativeIsolationVerified, true);
    assert.equal(result.reasonCode, "VERIFIED");
    assert.match(result.currentUserSidSha256, /^[0-9a-f]{64}$/);
    assert.notEqual(result.productionUserSidSha256, result.isolatedUserSidSha256);
    assert.doesNotMatch(JSON.stringify(result), /S-1-5-21/);
    assert.doesNotMatch(JSON.stringify(result), /operator-approved verifier account/);
    assert.equal(
      evaluate({ identity: { sid: ISOLATED_SID, isAdmin: false, isElevated: true } })
        .nativeIsolationVerified,
      true,
    );
  });

  it("does not accept a VM hardware label as credential-isolation proof", () => {
    assert.equal(
      evaluate({ attestation: attestation({ kind: "isolated-windows-vm" }) }).reasonCode,
      "VM_ISOLATION_UNPROVEN",
    );
  });

  it("does not invoke the native launcher when preflight is blocked", async () => {
    let spawned = 0;
    const blocked = evaluate({ attestation: null });
    const result = await runGuardedNativeLaunch({
      isolation: blocked,
      start: async () => {
        spawned += 1;
        return "started";
      },
    });
    assert.equal(result.started, false);
    assert.equal(spawned, 0);
  });

  it("keeps doctor canLaunch false when tool probes pass but isolation is blocked", () => {
    const probes = { canLaunch: true, blockedReasons: [] };
    const blocked = nativeLaunchCapability(probes, evaluate({ attestation: null }));
    assert.equal(blocked.nativeIsolationVerified, false);
    assert.equal(blocked.canLaunch, false);
    assert.match(blocked.blockedReasons[0], /ATTESTATION_MISSING/);

    const verified = nativeLaunchCapability(probes, evaluate());
    assert.equal(verified.nativeIsolationVerified, true);
    assert.equal(verified.canLaunch, true);
    assert.deepEqual(verified.blockedReasons, []);

    const toolBlocked = nativeLaunchCapability(
      { canLaunch: false, blockedReasons: ["cargo missing"] },
      evaluate(),
    );
    assert.equal(toolBlocked.canLaunch, false);
    assert.deepEqual(toolBlocked.blockedReasons, ["cargo missing"]);
  });

  it("allows the native launcher callback after a verified preflight", async () => {
    let received = null;
    const result = await runGuardedNativeLaunch({
      isolation: evaluate(),
      start: async (isolation) => {
        received = isolation;
        return "started";
      },
    });
    assert.equal(result.started, true);
    assert.equal(result.value, "started");
    assert.equal(received.nativeIsolationVerified, true);
  });

  it("parses the structured Windows probe and fails closed on invalid JSON", () => {
    assert.deepEqual(
      parseWindowsIsolationProbe('{"identity":{"sid":"S-1-5-21-1-2-3-4","isAdmin":false}}'),
      { identity: { sid: "S-1-5-21-1-2-3-4", isAdmin: false } },
    );
    assert.throws(() => parseWindowsIsolationProbe("not-json"));
  });

  it("turns probe errors and non-Windows hosts into explicit blocked results", () => {
    const failed = probeNativeIsolation({
      platform: "win32",
      readProbe: () => {
        throw new Error("PowerShell unavailable");
      },
      checkedAt: new Date("2026-11-08T00:00:00.000Z"),
    });
    assert.equal(failed.status, "BLOCKED");
    assert.equal(failed.reasonCode, "WINDOWS_PROBE_FAILED");
    assert.match(failed.reason, /PowerShell unavailable/);

    const nonWindows = probeNativeIsolation({
      platform: "linux",
      checkedAt: new Date("2026-11-08T00:00:00.000Z"),
    });
    assert.equal(nonWindows.nativeIsolationVerified, false);
    assert.equal(nonWindows.reasonCode, "WINDOWS_ONLY");
  });

  it("reads the live Windows token and machine probe only", { skip: process.platform !== "win32" }, () => {
    const probe = readWindowsIsolationProbe();
    assert.match(probe.identity.sid, /^S-1-(?:5-21|12-1)-\d+(?:-\d+)+$/);
    assert.equal(typeof probe.identity.isAdmin, "boolean");
    assert.equal(typeof probe.identity.isElevated, "boolean");
    assert.ok("attestation" in probe);
    assert.ok("machineRecord" in probe);
  });
});
