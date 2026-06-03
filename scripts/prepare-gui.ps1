param(
  [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
)

$ErrorActionPreference = "SilentlyContinue"
$rootPath = (Resolve-Path $Root).Path.TrimEnd("\")
$targetRoot = Join-Path $rootPath "src-tauri\target"

Write-Host "Preparing MONOLITH GUI startup..."

# Stop the Vite dev server for this app if a previous launcher still owns port 1420.
Get-NetTCPConnection -LocalPort 1420 -State Listen | ForEach-Object {
  $proc = Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue
  if ($proc -and $proc.ProcessName -eq "node") {
    Write-Host "Stopping previous Vite dev server on port 1420 (PID $($proc.Id))."
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
  }
}

# Stop a previous dev binary from this workspace only. Do not touch other apps.
Get-Process monolith -ErrorAction SilentlyContinue | Where-Object {
  $_.Path -and $_.Path.StartsWith($targetRoot, [System.StringComparison]::OrdinalIgnoreCase)
} | ForEach-Object {
  Write-Host "Stopping previous MONOLITH dev process (PID $($_.Id))."
  Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
}

# If a previous Cargo/Rust build from this workspace is still alive, stop it before
# starting a new one. This prevents Windows archive-lock failures during startup.
Get-CimInstance Win32_Process | Where-Object {
  ($_.Name -eq "cargo.exe" -or $_.Name -eq "rustc.exe") -and
  $_.CommandLine -and
  $_.CommandLine.Contains($rootPath)
} | ForEach-Object {
  Write-Host "Stopping stale Rust build process $($_.Name) (PID $($_.ProcessId))."
  Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
}

Start-Sleep -Milliseconds 400

# Cargo can leave these behind when Windows interrupts archive creation.
$depsPath = Join-Path $rootPath "src-tauri\target\debug\deps"
if (Test-Path $depsPath) {
  Get-ChildItem $depsPath -Directory -Filter ".tmp*.temp-archive" | ForEach-Object {
    Write-Host "Removing stale Cargo temp archive $($_.Name)."
    Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
  }
}
