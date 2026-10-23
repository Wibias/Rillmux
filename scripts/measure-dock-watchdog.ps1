<#
A/B the dock watchdog idle interval with the same debug build.

  npm run perf:dock-ab -- -IdleMs 500
  npm run perf:dock-ab -- -IdleMs 1000

The watchdog runs in the Rillmux main process, so main-process CPU isolates
its cost from the WebView2 renderer processes. Run each interval a few times;
this is a directional measurement, not a microbenchmark.
#>
param(
  [int]$IdleMs = 500,
  [int]$WarmupSeconds = 8,
  [int]$Seconds = 20
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$env:RILLMUX_DOCK_WATCHDOG_IDLE_MS = "$IdleMs"

$launcher = Start-Process -FilePath "cmd.exe" -ArgumentList @("/c", "npm run tauri dev") `
  -WorkingDirectory $repo -PassThru -WindowStyle Hidden
$app = $null
try {
  $deadline = [DateTime]::UtcNow.AddSeconds(150)
  while ([DateTime]::UtcNow -lt $deadline) {
    $app = Get-Process -Name Rillmux -ErrorAction SilentlyContinue |
      Where-Object { $_.Path -like "*\target\debug\Rillmux.exe" } |
      Select-Object -First 1
    if ($app -and $app.MainWindowHandle -ne 0) { break }
    Start-Sleep -Milliseconds 200
  }
  if (-not $app -or $app.MainWindowHandle -eq 0) {
    throw "Debug Rillmux window did not appear within 150 seconds"
  }

  Start-Sleep -Seconds $WarmupSeconds
  $before = [double](Get-Process -Id $app.Id).CPU
  Start-Sleep -Seconds $Seconds
  $current = Get-Process -Id $app.Id
  $after = [double]$current.CPU
  $cpuPct = (($after - $before) / $Seconds) * 100.0

  [pscustomobject]@{
    idle_ms = $IdleMs
    seconds = $Seconds
    cpu_pct_one_core = [Math]::Round($cpuPct, 3)
    working_set_mb = [Math]::Round($current.WorkingSet64 / 1MB, 1)
    private_mb = [Math]::Round($current.PrivateMemorySize64 / 1MB, 1)
  } | ConvertTo-Json
}
finally {
  if ($app -and -not $app.HasExited) {
    Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
  }
  if ($launcher -and -not $launcher.HasExited) {
    Start-Process -FilePath "taskkill.exe" -ArgumentList @("/PID", "$($launcher.Id)", "/T", "/F") `
      -Wait -WindowStyle Hidden | Out-Null
  }
}
