param(
  [string]$Version,
  [string]$Repo,
  [string]$Notes = "MONOLITH release",
  [switch]$Publish,
  [switch]$SkipChecks,
  [switch]$AllowDirty
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

function Import-DotEnv([string]$Path) {
  if (!(Test-Path $Path)) { return }
  Get-Content $Path | ForEach-Object {
    $line = $_.Trim()
    if (!$line -or $line.StartsWith("#") -or !$line.Contains("=")) { return }
    $name, $value = $line.Split("=", 2)
    $name = $name.Trim()
    $value = $value.Trim().Trim('"').Trim("'")
    if ($name) {
      [Environment]::SetEnvironmentVariable($name, $value, "Process")
    }
  }
}

function Required-Env([string[]]$Names, [string]$Message) {
  foreach ($name in $Names) {
    $value = [Environment]::GetEnvironmentVariable($name, "Process")
    if ($value) { return $value }
  }
  throw $Message
}

function Invoke-GitHubJson([string]$Method, [string]$Uri, [object]$Body = $null) {
  $token = Required-Env @("GITHUB_TOKEN", "GH_TOKEN", "GITHUB") "Missing GitHub token. Set GITHUB in .env.local."
  $headers = @{
    Authorization = "Bearer $token"
    Accept = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
  }
  if ($null -eq $Body) {
    return Invoke-RestMethod -Method $Method -Uri $Uri -Headers $headers
  }
  return Invoke-RestMethod -Method $Method -Uri $Uri -Headers $headers -Body ($Body | ConvertTo-Json -Depth 20) -ContentType "application/json"
}

function Upload-ReleaseAsset([int64]$ReleaseId, [string]$RepoName, [string]$Path) {
  $token = Required-Env @("GITHUB_TOKEN", "GH_TOKEN", "GITHUB") "Missing GitHub token. Set GITHUB in .env.local."
  $headers = @{
    Authorization = "Bearer $token"
    Accept = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
  }
  $name = [IO.Path]::GetFileName($Path)
  $assets = Invoke-RestMethod -Method Get -Uri "https://api.github.com/repos/$RepoName/releases/$ReleaseId/assets" -Headers $headers
  foreach ($asset in $assets) {
    if ($asset.name -eq $name) {
      Invoke-RestMethod -Method Delete -Uri "https://api.github.com/repos/$RepoName/releases/assets/$($asset.id)" -Headers $headers | Out-Null
    }
  }
  $uploadUrl = "https://uploads.github.com/repos/$RepoName/releases/$ReleaseId/assets?name=$([uri]::EscapeDataString($name))"
  Invoke-RestMethod -Method Post -Uri $uploadUrl -Headers $headers -InFile $Path -ContentType "application/octet-stream" | Out-Null
}

Push-Location $Root
try {
  Import-DotEnv (Join-Path $Root ".env.local")

  if (!$Repo) { $Repo = [Environment]::GetEnvironmentVariable("GITHUB_REPOSITORY", "Process") }
  if (!$Repo) { $Repo = "designer9999/monolith" }

  if (!$Version) {
    $package = Get-Content (Join-Path $Root "package.json") -Raw | ConvertFrom-Json
    $Version = $package.version
  }
  if ($Version.StartsWith("v")) { $Version = $Version.Substring(1) }
  if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
    throw "Version must be SemVer, for example 0.1.1. Got: $Version"
  }
  $packageVersion = (Get-Content (Join-Path $Root "package.json") -Raw | ConvertFrom-Json).version
  if ($Version -ne $packageVersion) {
    throw "Release version $Version does not match package.json version $packageVersion. Run npm run release:set-version -- $Version first."
  }
  if (!$AllowDirty) {
    $dirty = (& git status --porcelain --untracked-files=no)
    if ($LASTEXITCODE -ne 0) { throw "git status failed with exit code $LASTEXITCODE" }
    if ($dirty) {
      throw "Working tree has tracked changes. Commit or stash before building a release, or pass -AllowDirty for a local-only test build."
    }
  }

  $keyPath = [Environment]::GetEnvironmentVariable("TAURI_SIGNING_PRIVATE_KEY", "Process")
  if ($keyPath -and !(Test-Path $keyPath)) {
    $keyPath = $null
  }
  if (!$keyPath) { $keyPath = [Environment]::GetEnvironmentVariable("TAURI_SIGNING_PRIVATE_KEY_PATH", "Process") }
  if (!$keyPath) { $keyPath = Join-Path $Root ".tauri\monolith.key" }
  if (!(Test-Path $keyPath)) {
    throw "Missing updater signing key at $keyPath. Run: npx tauri signer generate --ci -w .tauri\monolith.key"
  }
  $env:TAURI_SIGNING_PRIVATE_KEY = (Resolve-Path $keyPath).Path

  if (!$SkipChecks) {
    Invoke-External "npm.cmd" @("run", "check")
  }

  $tauriCli = Join-Path $Root "node_modules\.bin\tauri.cmd"
  if (!(Test-Path $tauriCli)) {
    throw "Missing local Tauri CLI at $tauriCli. Run npm install first."
  }
  $nsisDir = Join-Path $Root "src-tauri\target\release\bundle\nsis"
  if (Test-Path $nsisDir) {
    Get-ChildItem $nsisDir -Filter "*.exe" -ErrorAction SilentlyContinue | Remove-Item -Force
    Get-ChildItem $nsisDir -Filter "*.sig" -ErrorAction SilentlyContinue | Remove-Item -Force
  }
  Invoke-External $tauriCli @("build", "--bundles", "nsis", "--ci")

  $installer = Get-ChildItem $nsisDir -Filter "MONOLITH_${Version}_*setup.exe" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
  if (!$installer) { throw "No NSIS installer for version $Version was produced in $nsisDir" }
  $signaturePath = "$($installer.FullName).sig"
  if (!(Test-Path $signaturePath)) { throw "Missing updater signature: $signaturePath" }

  $tag = "v$Version"
  $releaseDir = Join-Path $Root "release\$tag"
  New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null

  $installerOut = Join-Path $releaseDir $installer.Name
  $sigOut = Join-Path $releaseDir ([IO.Path]::GetFileName($signaturePath))
  Copy-Item $installer.FullName $installerOut -Force
  Copy-Item $signaturePath $sigOut -Force

  $signature = (Get-Content $signaturePath -Raw).Trim()
  $assetUrl = "https://github.com/$Repo/releases/download/$tag/$($installer.Name)"
  $manifest = [ordered]@{
    version = $Version
    notes = $Notes
    pub_date = (Get-Date).ToUniversalTime().ToString("o")
    platforms = [ordered]@{
      "windows-x86_64" = [ordered]@{
        signature = $signature
        url = $assetUrl
      }
    }
  }
  $manifestPath = Join-Path $releaseDir "latest.json"
  Write-Utf8NoBom $manifestPath ($manifest | ConvertTo-Json -Depth 20)

  if ($Publish) {
    $release = $null
    try {
      $release = Invoke-GitHubJson Get "https://api.github.com/repos/$Repo/releases/tags/$tag"
    } catch {
      $release = Invoke-GitHubJson Post "https://api.github.com/repos/$Repo/releases" @{
        tag_name = $tag
        name = "MONOLITH $tag"
        body = $Notes
        draft = $false
        prerelease = $false
      }
    }

    Upload-ReleaseAsset $release.id $Repo $installerOut
    Upload-ReleaseAsset $release.id $Repo $sigOut
    Upload-ReleaseAsset $release.id $Repo $manifestPath
    Write-Host "Published $tag to https://github.com/$Repo/releases/tag/$tag"
  }

  Write-Host "Installer: $installerOut"
  Write-Host "Updater manifest: $manifestPath"
}
finally {
  Pop-Location
}
