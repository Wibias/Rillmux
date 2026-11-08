import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";

/**
 * Native Rillmux runs as the same Windows user unless the verifier itself runs
 * under a different credential boundary. APPDATA and environment variables do
 * not partition Windows Credential Manager, so they are deliberately ignored
 * by this trust decision.
 */
export const NATIVE_ISOLATION_ATTESTATION_KEY =
  "HKLM\\SOFTWARE\\Wibias\\RillmuxVerifier\\NativeIsolation";
export const NATIVE_ISOLATION_SCHEMA_VERSION = 1;
export const WINDOWS_POWERSHELL_PATH =
  "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

const WINDOWS_MODULES_PATH = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\Modules";

const SID_PATTERN = /^S-1-(?:5-21|12-1)-\d+(?:-\d+)+$/;
const PROTECTED_OWNER_SIDS = new Set(["S-1-5-18", "S-1-5-32-544"]);
const MUTATION_RIGHTS =
  0x00000002 | // SetValue
  0x00000004 | // CreateSubKey
  0x00000020 | // CreateLink
  0x00010000 | // Delete
  0x00040000 | // ChangePermissions
  0x00080000 | // TakeOwnership
  0x40000000; // GenericWrite

const VM_MARKERS = [
  "virtual machine",
  "vmware",
  "virtualbox",
  "hyper-v",
  "qemu",
  "kvm",
  "xen",
  "parallels",
];

const WINDOWS_PROBE_SCRIPT = String.raw`$ErrorActionPreference = 'Stop'
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [System.Security.Principal.WindowsPrincipal]::new($identity)
$attestation = $null
$machineRecord = $null
$virtualMachine = $null
$hive = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
  [Microsoft.Win32.RegistryHive]::LocalMachine,
  [Microsoft.Win32.RegistryView]::Default
)

try {
  $rights = [System.Security.AccessControl.RegistryRights]::ReadKey -bor
    [System.Security.AccessControl.RegistryRights]::ReadPermissions
  $key = $hive.OpenSubKey(
    'SOFTWARE\Wibias\RillmuxVerifier\NativeIsolation',
    [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadSubTree,
    $rights
  )
  if ($null -ne $key) {
    $attestation = [pscustomobject]@{
      version = $key.GetValue('version', $null)
      kind = $key.GetValue('kind', $null)
      productionUserSid = $key.GetValue('productionUserSid', $null)
      isolatedUserSid = $key.GetValue('isolatedUserSid', $null)
      approvedAt = $key.GetValue('approvedAt', $null)
      expiresAt = $key.GetValue('expiresAt', $null)
      approvalReference = $key.GetValue('approvalReference', $null)
    }

    try {
      $sections = [System.Security.AccessControl.AccessControlSections]::Owner -bor
        [System.Security.AccessControl.AccessControlSections]::Access
      $acl = $key.GetAccessControl($sections)
      $ownerSid = ([System.Security.Principal.NTAccount]::new($acl.Owner)).Translate(
        [System.Security.Principal.SecurityIdentifier]
      ).Value
      $rules = @($acl.Access | ForEach-Object {
        $ruleSid = $null
        try {
          $ruleSid = $_.IdentityReference.Translate(
            [System.Security.Principal.SecurityIdentifier]
          ).Value
        } catch {}
        $ruleRights = $null
        try { $ruleRights = [int64]$_.RegistryRights } catch {}
        [pscustomobject]@{
          sid = $ruleSid
          access = $_.AccessControlType.ToString()
          rights = $ruleRights
        }
      })
      $machineRecord = [pscustomobject]@{
        ownerSid = $ownerSid
        rules = $rules
      }
    } catch {
      $machineRecord = $null
    } finally {
      $key.Dispose()
    }
  }
} catch {}

try {
  $bios = $hive.OpenSubKey('HARDWARE\DESCRIPTION\System\BIOS')
  if ($null -ne $bios) {
    $virtualMachine = [pscustomobject]@{
      manufacturer = [string]$bios.GetValue('SystemManufacturer', '')
      model = [string]$bios.GetValue('SystemProductName', '')
    }
    $bios.Dispose()
  }
} catch {}
finally {
  $hive.Dispose()
}

[pscustomobject]@{
  identity = [pscustomobject]@{
    sid = $identity.User.Value
    isAdmin = $principal.IsInRole(
      [System.Security.Principal.WindowsBuiltInRole]::Administrator
    )
  }
  attestation = $attestation
  machineRecord = $machineRecord
  virtualMachine = $virtualMachine
} | ConvertTo-Json -Depth 6 -Compress`;

function hashSid(sid) {
  return typeof sid === "string" && SID_PATTERN.test(sid)
    ? createHash("sha256").update(sid).digest("hex")
    : null;
}

function baseResult({ platform, identity, attestation, machineRecord, virtualMachine, checkedAt }) {
  const currentSid = typeof identity?.sid === "string" ? identity.sid : null;
  return {
    schemaVersion: NATIVE_ISOLATION_SCHEMA_VERSION,
    preflight: "windows-user-isolation-v1",
    status: "BLOCKED",
    nativeIsolationVerified: false,
    checkedAt: (checkedAt ?? new Date()).toISOString(),
    platform: platform ?? process.platform,
    attestationKey: NATIVE_ISOLATION_ATTESTATION_KEY,
    reasonCode: null,
    reason: null,
    currentUserSidSha256: hashSid(currentSid),
    currentUserIsAdmin: typeof identity?.isAdmin === "boolean" ? identity.isAdmin : null,
    productionUserSidSha256: hashSid(attestation?.productionUserSid),
    isolatedUserSidSha256: hashSid(attestation?.isolatedUserSid),
    isolationKind: attestation?.kind ?? null,
    attestationApprovedAt: attestation?.approvedAt ?? null,
    attestationExpiresAt: attestation?.expiresAt ?? null,
    evidence: {
      identitySource: "WindowsIdentity.GetCurrent().User",
      attestationSource: "machine-scope HKLM registry",
      machineRecordProtected: isMachineRecordProtected(machineRecord, currentSid),
      virtualMachineDetected: isVirtualMachine(virtualMachine),
      appDataOverrideIsCredentialProof: false,
      environmentIsCredentialProof: false,
    },
  };
}

function block(context, reasonCode, reason) {
  return {
    ...baseResult(context),
    status: "BLOCKED",
    nativeIsolationVerified: false,
    reasonCode,
    reason,
  };
}

function isVirtualMachine(virtualMachine) {
  const description = `${virtualMachine?.manufacturer ?? ""} ${virtualMachine?.model ?? ""}`
    .trim()
    .toLowerCase();
  return description.length > 0 && VM_MARKERS.some((marker) => description.includes(marker));
}

function isMachineRecordProtected(machineRecord, currentSid) {
  if (!machineRecord || !PROTECTED_OWNER_SIDS.has(machineRecord.ownerSid)) return false;
  const rules = Array.isArray(machineRecord.rules) ? machineRecord.rules : [];
  if (rules.length === 0) return false;
  return !rules.some((rule) => {
    if (rule?.access !== "Allow") return false;
    const rights = rule.rights == null ? NaN : Number(rule.rights);
    const canWrite = !Number.isFinite(rights) || (rights & MUTATION_RIGHTS) !== 0;
    if (!canWrite) return false;
    if (rule.sid === currentSid) return true;
    // Only SYSTEM and Built-in Administrators may mutate the machine record to
    // make it independent of the ordinary isolated-user token.
    return !PROTECTED_OWNER_SIDS.has(rule.sid);
  });
}

function validDate(value) {
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

export function evaluateNativeIsolation({
  platform = process.platform,
  identity,
  attestation,
  machineRecord,
  virtualMachine,
  checkedAt = new Date(),
} = {}) {
  const context = { platform, identity, attestation, machineRecord, virtualMachine, checkedAt };
  if (platform !== "win32") {
    return block(context, "WINDOWS_ONLY", "Native Rillmux verification is Windows-only.");
  }

  const currentSid = identity?.sid;
  if (typeof currentSid !== "string" || !SID_PATTERN.test(currentSid)) {
    return block(context, "CURRENT_IDENTITY_UNAVAILABLE", "The current Windows user SID could not be proven.");
  }
  if (typeof identity.isAdmin !== "boolean") {
    return block(context, "CURRENT_IDENTITY_UNAVAILABLE", "Windows administrator status could not be proven.");
  }
  if (!attestation || typeof attestation !== "object") {
    return block(
      context,
      "ATTESTATION_MISSING",
      `No operator attestation exists at ${NATIVE_ISOLATION_ATTESTATION_KEY}.`,
    );
  }
  if (Number(attestation.version) !== NATIVE_ISOLATION_SCHEMA_VERSION) {
    return block(context, "ATTESTATION_SCHEMA_UNSUPPORTED", "The machine-scope isolation attestation version is unsupported.");
  }
  if (!["separate-windows-user", "isolated-windows-vm"].includes(attestation.kind)) {
    return block(context, "ISOLATION_KIND_UNSUPPORTED", "The isolation kind is not recognized.");
  }
  if (
    typeof attestation.productionUserSid !== "string" ||
    !SID_PATTERN.test(attestation.productionUserSid) ||
    typeof attestation.isolatedUserSid !== "string" ||
    !SID_PATTERN.test(attestation.isolatedUserSid)
  ) {
    return block(context, "ATTESTATION_INVALID", "The attestation does not contain valid production and isolated user SIDs.");
  }
  if (
    typeof attestation.approvalReference !== "string" ||
    !attestation.approvalReference.trim() ||
    attestation.approvalReference.length > 200
  ) {
    return block(context, "ATTESTATION_INVALID", "The attestation lacks a bounded operator approval reference.");
  }

  const approvedAt = validDate(attestation.approvedAt);
  const expiresAt = validDate(attestation.expiresAt);
  const now = checkedAt instanceof Date ? checkedAt.getTime() : Date.parse(checkedAt);
  if (approvedAt === null || expiresAt === null || !Number.isFinite(now)) {
    return block(context, "ATTESTATION_INVALID", "The attestation timestamps are invalid.");
  }
  if (approvedAt > now + 5 * 60_000) {
    return block(context, "ATTESTATION_INVALID", "The attestation approval time is in the future.");
  }
  if (expiresAt <= now) {
    return block(context, "ATTESTATION_EXPIRED", "The machine-scope isolation attestation has expired.");
  }
  if (expiresAt <= approvedAt) {
    return block(context, "ATTESTATION_INVALID", "The attestation expiry does not follow its approval time.");
  }
  if (!isMachineRecordProtected(machineRecord, currentSid)) {
    return block(
      context,
      "ATTESTATION_NOT_PROTECTED",
      "The HKLM attestation is missing or writable by the current/broad user principals.",
    );
  }
  if (currentSid !== attestation.isolatedUserSid) {
    return block(context, "CURRENT_USER_NOT_ISOLATED", "The current Windows user does not match the attested isolated user.");
  }
  if (attestation.productionUserSid === attestation.isolatedUserSid || currentSid === attestation.productionUserSid) {
    return block(context, "CURRENT_USER_IS_PRODUCTION", "The current Windows user is the production user, not an isolated user.");
  }

  if (attestation.kind === "separate-windows-user" && identity.isAdmin) {
    return block(
      context,
      "ISOLATED_USER_IS_ADMIN",
      "A separate Windows user must run without an elevated administrator token.",
    );
  }
  if (attestation.kind === "isolated-windows-vm" && !isVirtualMachine(virtualMachine)) {
    return block(context, "ISOLATED_VM_NOT_PROVEN", "The attested Windows VM boundary could not be confirmed.");
  }

  return {
    ...baseResult(context),
    status: "VERIFIED",
    nativeIsolationVerified: true,
    reasonCode: "VERIFIED",
    reason: "Current Windows identity and machine-scope operator attestation prove a distinct credential-store boundary.",
  };
}

export function parseWindowsIsolationProbe(output) {
  const value = JSON.parse(String(output).trim());
  if (!value || typeof value !== "object") {
    throw new Error("Windows isolation probe returned no object");
  }
  return value;
}

export function readWindowsIsolationProbe() {
  const output = execFileSync(
    WINDOWS_POWERSHELL_PATH,
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", WINDOWS_PROBE_SCRIPT],
    {
      encoding: "utf8",
      timeout: 20_000,
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
      env: {
        ...process.env,
        PSModulePath: WINDOWS_MODULES_PATH,
      },
    },
  );
  return parseWindowsIsolationProbe(output);
}

export function probeNativeIsolation({
  platform = process.platform,
  readProbe = readWindowsIsolationProbe,
  checkedAt = new Date(),
} = {}) {
  if (platform !== "win32") {
    return evaluateNativeIsolation({ platform, checkedAt });
  }
  try {
    const probe = readProbe();
    return evaluateNativeIsolation({
      platform,
      identity: probe.identity,
      attestation: probe.attestation,
      machineRecord: probe.machineRecord,
      virtualMachine: probe.virtualMachine,
      checkedAt,
    });
  } catch (error) {
    return block(
      { platform, identity: null, attestation: null, machineRecord: null, virtualMachine: null, checkedAt },
      "WINDOWS_PROBE_FAILED",
      `Windows isolation probe failed closed: ${String(error?.message ?? error).slice(0, 500)}`,
    );
  }
}

export async function runGuardedNativeLaunch({ isolation, start }) {
  if (isolation?.nativeIsolationVerified !== true || isolation?.status !== "VERIFIED") {
    return { started: false, isolation };
  }
  return { started: true, isolation, value: await start(isolation) };
}

export function formatNativeIsolationBlock(isolation) {
  const code = isolation?.reasonCode || "UNKNOWN";
  const reason = isolation?.reason || "Native isolation could not be verified.";
  return `Native launch blocked before spawn (${code}): ${reason}`;
}

export function nativeLaunchCapability(probes, isolation) {
  const verified = isolation?.nativeIsolationVerified === true;
  return {
    nativeIsolationVerified: verified,
    canLaunch: Boolean(probes?.canLaunch) && verified,
    blockedReasons: [
      ...(Array.isArray(probes?.blockedReasons) ? probes.blockedReasons : []),
      ...(verified ? [] : [formatNativeIsolationBlock(isolation)]),
    ],
  };
}
