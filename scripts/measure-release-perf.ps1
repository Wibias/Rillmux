<#
Measures a built Rillmux binary and enforces release-performance budgets.

Usage:
  npm run perf:release
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-release-perf.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-release-perf.ps1 `
    -Exe src-tauri/target/debug/Rillmux.exe -AllowRunning

Close the installed Rillmux before measuring a release build. A running
release instance makes Tauri's single-instance plugin hand off the launch and
exit, so the measurement would describe the wrong process.
#>
param(
  [string]$Exe = "src-tauri/target/release/Rillmux.exe",
  [int]$IdleSeconds = 15,
  [int]$SampleSeconds = 10,
  [double]$BudgetWindowMs = 2500,
  [double]$BudgetPrivateMb = 350,
  [double]$BudgetCpuPct = 2.0,
  [switch]$AllowRunning
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
if (-not [IO.Path]::IsPathRooted($Exe)) {
  $Exe = Join-Path $repo $Exe
}
if (-not (Test-Path -LiteralPath $Exe)) {
  throw "Missing $Exe. Build it first with: npm run tauri -- build --no-bundle"
}

$running = @(Get-Process -Name Rillmux -ErrorAction SilentlyContinue)
if ($running.Count -gt 0 -and -not $AllowRunning) {
  throw "Rillmux is already running. Close it first, or pass -AllowRunning for a debug A/B run."
}

$launched = $null
try {
  $stopwatch = [Diagnostics.Stopwatch]::StartNew()
  $launched = Start-Process -FilePath $Exe -PassThru
  $windowMs = $null
  $deadline = [DateTime]::UtcNow.AddSeconds(25)
  while ([DateTime]::UtcNow -lt $deadline) {
    $process = Get-Process -Id $launched.Id -ErrorAction SilentlyContinue
    if ($process -and $process.MainWindowHandle -ne 0 -and $process.MainWindowTitle -like "*Rillmux*") {
      $windowMs = $stopwatch.Elapsed.TotalMilliseconds
      break
    }
    Start-Sleep -Milliseconds 20
  }
  if ($null -eq $windowMs) {
    throw "Rillmux did not show a window within 25 seconds (did this launch hand off to another instance?)"
  }

  Start-Sleep -Seconds $IdleSeconds

  function Get-ProcessTreeIds([int]$rootId) {
    $ids = [System.Collections.Generic.List[int]]::new()
    $ids.Add($rootId)
    $all = @(Get-CimInstance Win32_Process)
    $frontier = @($rootId)
    for ($depth = 0; $depth -lt 8; $depth++) {
      $children = @($all | Where-Object { $frontier -contains [int]$_.ParentProcessId })
      if ($children.Count -eq 0) { break }
      $next = @()
      foreach ($child in $children) {
        $childId = [int]$child.ProcessId
        if (-not $ids.Contains($childId)) {
          $ids.Add($childId)
          $next += $childId
        }
      }
      $frontier = $next
    }
    return $ids
  }

  function Get-Snapshot([int[]]$ids) {
    $snapshot = @{}
    foreach ($id in $ids) {
      $process = Get-Process -Id $id -ErrorAction SilentlyContinue
      if ($null -eq $process) { continue }
      $snapshot[$id] = [pscustomobject]@{
        CPU = [double]$process.CPU
        WorkingSet = [double]$process.WorkingSet64
        Private = [double]$process.PrivateMemorySize64
      }
    }
    return $snapshot
  }

  $ids = Get-ProcessTreeIds $launched.Id
  $before = Get-Snapshot $ids
  Start-Sleep -Seconds $SampleSeconds
  $after = Get-Snapshot $ids

  $cpuDelta = 0.0
  $workingSet = 0.0
  $private = 0.0
  foreach ($id in $after.Keys) {
    $current = $after[$id]
    $workingSet += $current.WorkingSet
    $private += $current.Private
    if ($before.ContainsKey($id)) {
      $cpuDelta += [Math]::Max(0.0, $current.CPU - $before[$id].CPU)
    }
  }
  $cpuPct = if ($SampleSeconds -gt 0) { ($cpuDelta / $SampleSeconds) * 100.0 } else { 0.0 }

  $result = [pscustomobject]@{
    exe = $Exe
    window_ms = [Math]::Round($windowMs, 0)
    idle_seconds = $IdleSeconds
    sample_seconds = $SampleSeconds
    working_set_mb = [Math]::Round($workingSet / 1MB, 1)
    private_mb = [Math]::Round($private / 1MB, 1)
    cpu_pct_one_core = [Math]::Round($cpuPct, 2)
    budgets = [pscustomobject]@{
      window_ms = $BudgetWindowMs
      private_mb = $BudgetPrivateMb
      cpu_pct_one_core = $BudgetCpuPct
    }
    pass = ($windowMs -le $BudgetWindowMs) -and
      (($private / 1MB) -le $BudgetPrivateMb) -and
      ($cpuPct -le $BudgetCpuPct)
  }
  $result | ConvertTo-Json -Depth 4
  if (-not $result.pass) { exit 1 }
}
finally {
  if ($launched -and -not $launched.HasExited) {
    Start-Process -FilePath "taskkill.exe" -ArgumentList @("/PID", "$($launched.Id)", "/T", "/F") -Wait -WindowStyle Hidden | Out-Null
  }
}
