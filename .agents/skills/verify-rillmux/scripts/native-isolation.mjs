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
export const WINDOWS_NETSTAT_PATH = "C:\\Windows\\System32\\netstat.exe";
export const WINDOWS_TASKKILL_PATH = "C:\\Windows\\System32\\taskkill.exe";

export const WINDOWS_POWERSHELL_MODULES_PATH =
  "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\Modules";

const SID_PATTERN = /^S-1-(?:5-21|12-1)-\d+(?:-\d+)+$/;
const PROTECTED_OWNER_SIDS = new Set(["S-1-5-18", "S-1-5-32-544"]);
const READ_ONLY_REGISTRY_RIGHTS =
  0x00000001 | // QueryValues
  0x00000008 | // EnumerateSubKeys
  0x00000010 | // Notify
  0x00020000 | // ReadPermissions
  0x00100000 | // Synchronize
  0x20000000 | // GenericExecute
  0x80000000; // GenericRead

const WINDOWS_PROBE_SCRIPT = String.raw`$ErrorActionPreference = 'Stop'
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [System.Security.Principal.WindowsPrincipal]::new($identity)
$attestation = $null
$machineRecord = $null
$administratorSid = 'S-1-5-32-544'
$isAdminMember = $false
# Groups include the deny-only Administrators SID in a UAC-filtered token.
foreach ($group in $identity.Groups) {
  if ($group.Value -eq $administratorSid) {
    $isAdminMember = $true
    break
  }
}
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
        areAccessRulesProtected = $acl.AreAccessRulesProtected
        rules = $rules
      }
    } catch {
      $machineRecord = $null
    } finally {
      $key.Dispose()
    }
  }
} catch {}

$hive.Dispose()

[pscustomobject]@{
  identity = [pscustomobject]@{
    sid = $identity.User.Value
    isAdmin = $isAdminMember
    isElevated = $principal.IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)
  }
  attestation = $attestation
  machineRecord = $machineRecord
} | ConvertTo-Json -Depth 6 -Compress`;

function hashSid(sid) {
  return typeof sid === "string" && SID_PATTERN.test(sid)
    ? createHash("sha256").update(sid).digest("hex")
    : null;
}

function baseResult({ platform, identity, attestation, machineRecord, checkedAt }) {
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
    currentUserIsElevated:
      typeof identity?.isElevated === "boolean" ? identity.isElevated : null,
    productionUserSidSha256: hashSid(attestation?.productionUserSid),
    isolatedUserSidSha256: hashSid(attestation?.isolatedUserSid),
    isolationKind: attestation?.kind ?? null,
    attestationApprovedAt: attestation?.approvedAt ?? null,
    attestationExpiresAt: attestation?.expiresAt ?? null,
    evidence: {
      identitySource: "WindowsIdentity.GetCurrent().User",
      attestationSource: "machine-scope HKLM registry",
      machineRecordProtected: isMachineRecordProtected(machineRecord),
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

function isMachineRecordProtected(machineRecord) {
  if (!machineRecord || !PROTECTED_OWNER_SIDS.has(machineRecord.ownerSid)) return false;
  if (machineRecord.areAccessRulesProtected !== true) return false;
  const rules = Array.isArray(machineRecord.rules) ? machineRecord.rules : [];
  if (rules.length === 0) return false;
  return !rules.some((rule) => {
    if (!rule || typeof rule !== "object") return true;
    if (rule.access === "Deny") return false;
    if (rule.access !== "Allow") return true;
    const rights = rule.rights == null ? NaN : Number(rule.rights);
    if (!Number.isFinite(rights) || !Number.isInteger(rights)) return true;
    if (rule.sid == null) return true;
    // SYSTEM and Built-in Administrators may retain FullControl. Every other
    // principal gets an explicit read-only allowlist; unknown/new rights and
    // GENERIC_ALL fail closed instead of being mistaken for read access.
    if (PROTECTED_OWNER_SIDS.has(rule.sid)) return false;
    return (rights & ~READ_ONLY_REGISTRY_RIGHTS) !== 0;
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
  checkedAt = new Date(),
} = {}) {
  const context = { platform, identity, attestation, machineRecord, checkedAt };
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
  if (attestation.kind === "isolated-windows-vm") {
    return block(
      context,
      "VM_ISOLATION_UNPROVEN",
      "A VM hardware label alone is not proof of a separate credential boundary; use a verified separate Windows user.",
    );
  }
  if (attestation.kind !== "separate-windows-user") {
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
  if (!isMachineRecordProtected(machineRecord)) {
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

  if (identity.isAdmin) {
    return block(
      context,
      "ISOLATED_USER_IS_ADMIN",
      "The isolated Windows user must not belong to the Administrators group, regardless of UAC elevation.",
    );
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
        PSModulePath: WINDOWS_POWERSHELL_MODULES_PATH,
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
      checkedAt,
    });
  } catch (error) {
    return block(
      { platform, identity: null, attestation: null, machineRecord: null, checkedAt },
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
