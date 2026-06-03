param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$')]
  [string]$Version
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Utf8NoBom = [Text.UTF8Encoding]::new($false)

function Write-Utf8NoBom([string]$Path, [string]$Value) {
  [IO.File]::WriteAllText($Path, $Value, $Utf8NoBom)
}

function Invoke-External([string]$FilePath, [string[]]$Arguments) {
  & $FilePath @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "$FilePath failed with exit code $LASTEXITCODE"
  }
}

Push-Location $Root
try {
  Invoke-External "npm.cmd" @("version", $Version, "--no-git-tag-version", "--allow-same-version")

  $cargoPath = Join-Path $Root "src-tauri\Cargo.toml"
  $cargo = Get-Content $cargoPath -Raw
  $cargo = [regex]::Replace(
    $cargo,
    '(?m)^version\s*=\s*"[^"]+"',
    "version = `"$Version`"",
    1
  )
  Write-Utf8NoBom $cargoPath $cargo

  $tauriPath = Join-Path $Root "src-tauri\tauri.conf.json"
  $tauri = Get-Content $tauriPath -Raw
  $tauri = [regex]::Replace(
    $tauri,
    '("version"\s*:\s*")[^"]+(")',
    "`${1}$Version`${2}",
    1
  )
  Write-Utf8NoBom $tauriPath $tauri

  Write-Host "MONOLITH version set to $Version"
}
finally {
  Pop-Location
}
