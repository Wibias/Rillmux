# Native Credential Isolation

The native lane starts the real Tauri application. It is not a signed-out shell: app startup can reach every production Twitch credential described below. A native launch remains **BLOCKED** unless the verifier proves that it is running under a separate Windows credential boundary.

## Credential and migration inventory

All three Twitch credentials use the same fixed Windows Credential Manager services (`rillmux`, with legacy `streamlink-twitch-gui`) and differ only by username.

| Credential | Keyring username | Current entry points | Mutation risk |
|---|---|---|---|
| Twitch OAuth | `twitch-oauth` | `AuthBootstrap` -> `auth_get_session` -> `src-tauri/src/auth/mod.rs` / `auth/store.rs` | Reads tokens on startup; validates/refreshes; can migrate from the legacy service and save/delete credentials |
| Website/playback OAuth | `twitch-website-oauth` | `twitch_web_auth_status` from website-auth UI/health paths and `src-tauri/src/twitch_web_auth/*` | Reads tokens; can migrate legacy credentials and save/delete; status cleanup reads/writes the isolated Streamlink config path |
| TV Channel Points OAuth | `twitch-channel-points-tv-oauth` | `channel_points_claim_auth_status` from Channel Points UI/health paths and `src-tauri/src/channel_points_claim_auth.rs` | Reads tokens; can migrate legacy credentials and save/delete; device ID derives from stored auth |

Legacy migration is destructive only for the matching username, but it is still a credential mutation. A frontend-only auth bypass would not cover `twitch_web_auth` or `channel_points_claim_auth`; the native preflight is therefore mandatory before the Tauri process exists.

The playback path also removes managed auth blocks from `%APPDATA%\streamlink\config.twitch`. The verifier's APPDATA override keeps that file operation in its workspace runtime tree, but it does not isolate Windows Credential Manager.

## Other native side effects

| Surface | Observed implementation | Isolated-retest requirement |
|---|---|---|
| Tauri plugin-store | `tauri_plugin_store` is registered in `src-tauri/src/app/run.rs`; settings resolve through Tauri shell paths | Use a separate Windows profile and inspect the actual resolved file path; the seeded APPDATA `settings.json` is not proof of settings isolation |
| App-data migration | `src-tauri/src/app/run.rs` calls `migrate_legacy_app_data`; `diagnostics.rs` resolves `%APPDATA%\Rillmux` | The whole Windows profile must be isolated; do not run migration against the production profile |
| WebView2 profile | Stored under `%LOCALAPPDATA%` | Separate Windows user profile; the verifier's APPDATA override alone is insufficient |
| Tray and notifications | Initialized by the Tauri app/plugins | They may create transient OS UI state; confirm they belong to the isolated desktop/session and remove the verifier-owned app afterward |
| Updater | Installed-release behavior differs from debug; debug does not show the update dialog | Treat as out of scope for this verifier unless an isolated installed build is explicitly under test |
| Dock and process automation | Can launch/place external windows when streams start | Do not start streams in the basic readiness test; document the gap |
| Implicit Twitch API | Startup OAuth status can validate/refresh and call Helix | Must occur only under the isolated Windows identity; logged-out UI is the expected base test |

The isolated retest is a logged-out UI/runtime test, not authenticated product acceptance.

## Trusted isolation proof

The preflight combines two facts:

1. The current Windows process identity from `WindowsIdentity.GetCurrent()`: its SID and direct membership in `S-1-5-32-544` (Built-in Administrators). Membership comes from the token groups and is checked independently of UAC elevation.
2. A machine-scope operator attestation in `HKLM\SOFTWARE\Wibias\RillmuxVerifier\NativeIsolation`.

The registry record is intentionally outside the repository and the current user's profile. APPDATA overrides, environment variables, a separate Git worktree, a renamed keyring service, or a file in the worktree are not accepted as proof. The verifier never creates or edits the registry record and never creates Windows accounts.

Required schema-version-1 values:

| Value | Type | Meaning |
|---|---|---|
| `version` | DWORD `1` | Attestation schema |
| `kind` | String | Currently only `separate-windows-user` |
| `productionUserSid` | String | SID of the production Windows user whose credentials must stay untouched |
| `isolatedUserSid` | String | SID of the dedicated test user in the isolated credential boundary |
| `approvedAt` | String | ISO-8601 approval time; must not be in the future |
| `expiresAt` | String | ISO-8601 expiry; must be later than approval and still in the future |
| `approvalReference` | String | Non-secret operator/ticket reference, at most 200 characters |

The probe also checks the registry owner and ACL. ACL inheritance must be disabled. The owner must be SYSTEM or Built-in Administrators. Those two principals may have full control; every other `Allow` ACE must be covered by an explicit read-only allowlist. Unknown ACE types, unknown/recent right bits, `GENERIC_ALL`, `GENERIC_WRITE`, write/set/delete/ownership rights, and unresolved SIDs fail closed. A separate Windows user must not be a member of Built-in Administrators, regardless of whether its current process is UAC-elevated.

VM hardware labels are not accepted as isolation proof in this revision. `isolated-windows-vm` is therefore reported as BLOCKED until a separately verifiable VM credential boundary is designed. A restart or hardware string alone must not be treated as identity isolation.

This is an operator-controlled fail-closed gate, not a defense against a hostile administrator of the isolated machine. A local administrator there can alter its own HKLM record; the security property is that a production-user token is never used for the native process, so the production Credential Manager remains a different Windows security boundary.

`preflight --json` reports hashed SIDs, never raw production SIDs or usernames.

## Operator provisioning

No account is created automatically. An operator must explicitly provision a separate standard Windows user and accept the host change outside the verifier. VM testing remains BLOCKED until independent VM credential-boundary proof exists.

1. In the production profile, read the production SID with:

   ```powershell
   [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
   ```

2. Sign in to the separately provisioned standard user and read the isolated SID the same way. Compare the production and isolated SIDs independently. They must differ. The isolated user must not belong to Built-in Administrators, must not be able to write the HKLM key, and must not be able to read the production profile.

3. As a separate administrator context on the host, create the machine-scope attestation. Populate the independently checked SIDs and an expiring approval reference from steps 1-2; the verifier itself must not perform this write.

   ```powershell
   $key = 'HKLM:\SOFTWARE\Wibias\RillmuxVerifier\NativeIsolation'
   New-Item -Path $key -Force | Out-Null
   New-ItemProperty -Path $key -Name version -PropertyType DWord -Value 1 -Force | Out-Null
   New-ItemProperty -Path $key -Name kind -PropertyType String -Value 'separate-windows-user' -Force | Out-Null
   New-ItemProperty -Path $key -Name productionUserSid -PropertyType String -Value '<production-SID>' -Force | Out-Null
   New-ItemProperty -Path $key -Name isolatedUserSid -PropertyType String -Value '<isolated-SID>' -Force | Out-Null
   New-ItemProperty -Path $key -Name approvedAt -PropertyType String -Value '<ISO-8601>' -Force | Out-Null
   New-ItemProperty -Path $key -Name expiresAt -PropertyType String -Value '<ISO-8601>' -Force | Out-Null
   New-ItemProperty -Path $key -Name approvalReference -PropertyType String -Value '<operator reference>' -Force | Out-Null
   ```

   Protect the DACL explicitly; inherited HKLM ACEs and `CREATOR OWNER` are intentionally not accepted:

   ```powershell
   $acl = Get-Acl -LiteralPath $key
   $acl.SetAccessRuleProtection($true, $false)
   foreach ($rule in @($acl.Access)) { $acl.RemoveAccessRule($rule) | Out-Null }
   $system = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
   $admins = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
   $users = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-545')
   $allow = [System.Security.AccessControl.AccessControlType]::Allow
   $full = [System.Security.AccessControl.RegistryRights]::FullControl
   $read = [System.Security.AccessControl.RegistryRights]::ReadKey
   $acl.SetOwner($admins)
   $acl.AddAccessRule([System.Security.AccessControl.RegistryAccessRule]::new($system, $full, $allow))
   $acl.AddAccessRule([System.Security.AccessControl.RegistryAccessRule]::new($admins, $full, $allow))
   $acl.AddAccessRule([System.Security.AccessControl.RegistryAccessRule]::new($users, $read, $allow))
   Set-Acl -LiteralPath $key -AclObject $acl
   ```

   Use ISO-8601 UTC timestamps, for example `(Get-Date).ToUniversalTime().ToString('o')` and an explicit short expiry such as seven days. Keep the key owned by SYSTEM/Built-in Administrators and grant the isolated standard user only read access. If preflight reports `ATTESTATION_NOT_PROTECTED`, do not relax the verifier; fix the elevated owner/DACL setup and retry.

4. Run `preflight --json` in the isolated context. It must report `status: VERIFIED`, `nativeIsolationVerified: true`, `currentUserIsAdmin: false` and `evidence.appDataOverrideIsCredentialProof: false`. A missing, malformed, expired, same-user, administrator-member, VM-label-only, or writable-attestation result is BLOCKED.

## Isolated native retest

Never run these steps in the production Windows account.

1. Clone the exact PR revision into the isolated standard user's writable workspace and run `npm ci` there.
2. Confirm `preflight --json` is VERIFIED before any native command.
3. Record only non-secret production settings hashes. Do not enumerate, query, export, or copy Credential Manager entries or the production profile into the test context.
4. Run `launch --lane native`. The verifier may proceed only after the trusted preflight.
5. Run `doctor --lane native --json`; parse the JSON and require `lanes.native.ready === true`. Exit 0 alone is not sufficient.
6. Run `inspect --lane native`; require `hydrated: true`, `appName: "Rillmux"`, and no `Desktop shell required` alert:

   ```powershell
   $doctor = node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs doctor --lane native --json | ConvertFrom-Json
   if (-not $doctor.lanes.native.ready) { throw 'Native lane is not ready' }
   $inspect = node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs inspect --lane native | ConvertFrom-Json
   if (-not $inspect.hydrated -or $inspect.appName -ne 'Rillmux') { throw 'Native shell is not hydrated' }
   if (($inspect.alerts -join ' ') -match 'Desktop shell required') { throw 'Connected chrome is the Vite lane' }
   ```

7. Verify actual Tauri/WebView2 CDP on IPv4/localhost/IPv6, then test only the logged-out UI. Do not authenticate unless a separate explicitly authorized test identity/account is provisioned.
8. Compare the non-secret production settings hashes; there must be no changes. Credential entries are protected by the separate Windows SID boundary and must not be enumerated. Cleanup may stop only owner-proven verifier processes and remove only verifier-owned runtime paths.
9. Record the attestation expiry. Do not roll the Agent Hub release gate or claim authenticated-session acceptance from this run.

If any isolation precondition is unavailable, stop and report `BLOCKED`.

The probe invokes `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe` by absolute path with a system-only `PSModulePath`, so a caller-controlled PATH cannot substitute the identity probe. A nonstandard Windows system directory fails closed.

## Native CDP and cleanup ownership

Native CDP is never selected implicitly. Commands without `--lane native` continue to use only the web runtime state; if no web state exists they fail instead of falling back to a persisted native target. Every native inspect/goto/click/screenshot/drive attachment requires:

- current `preflight` status VERIFIED for the current Windows SID;
- the stored native run's isolation hash and expiry to match the current preflight;
- the current Git HEAD to match the launch receipt;
- the recorded root PID to be alive, match the recorded process creation time and run the exact verifier-owned `tauri.js dev --config <runId>.tauri-cdp.json` command;
- exactly one CDP listener PID, and that PID must be a WebView2 descendant of the recorded root process.

`doctor` re-evaluates the same gate before opening CDP. A running app with expired/mismatched attestation, a stale HEAD, missing ownership data, or ambiguous listeners is reported as `ready: false` and receives no CDP attachment.

Cleanup uses only the live descendants of the exact recorded root command and its run-specific APPDATA directory. It never kills all debug binaries by project path. If the recorded root is gone while recorded children remain, or if the root PID/command does not match, cleanup is BLOCKED and performs no kill.
