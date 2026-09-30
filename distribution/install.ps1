# selfhost installer for Windows — https://cli.selfhost.dev
#
# Usage (PowerShell):
#   irm https://cli.selfhost.dev/install.ps1 | iex
#
# Environment overrides:
#   SELFHOST_INSTALL_DIR   install directory (default: %LOCALAPPDATA%\Programs\selfhost)
#
# Downloads the zip for the detected architecture from the release manifest,
# verifies its SHA-256, extracts selfhost.exe, and adds the install dir to
# the user PATH.

$ErrorActionPreference = "Stop"

$Bin = "selfhost"
$ManifestUrl = $env:SELFHOST_MANIFEST_URL
if (-not $ManifestUrl) { $ManifestUrl = "https://cli.selfhost.dev/latest.json" }
$InstallDir = $env:SELFHOST_INSTALL_DIR
if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\selfhost" }

function Fail($message) {
    Write-Host "  ✗ $message" -ForegroundColor Red
    exit 1
}
function Info($message) {
    Write-Host "  > $message" -ForegroundColor Green
}
function Warn($message) {
    Write-Host "  ! $message" -ForegroundColor Yellow
}

# detect architecture
$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64" }
    "ARM64" { "aarch64" }
    default { Fail "unsupported architecture: $($env:PROCESSOR_ARCHITECTURE)" }
}

Info "detected windows/$arch"

Info "fetching release manifest..."
$manifest = $null
foreach ($attempt in 1..3) {
    try {
        $manifest = Invoke-RestMethod -Uri $ManifestUrl -TimeoutSec 20
        break
    } catch {
        if ($attempt -eq 3) { Fail "can't reach $ManifestUrl. Please try again later." }
        Start-Sleep -Seconds $attempt
    }
}

$assetKey = "windows-$arch"
$asset = $manifest.assets.$assetKey
$sha = $manifest.sha256.$assetKey
$version = $manifest.version

if (-not $asset) { Fail "release manifest does not include a binary for $assetKey" }
# Origin pin: the manifest decides *which* release, never *where from*.
if ($asset -cnotlike "https://cli.selfhost.dev/*") {
    Fail "manifest points at an unexpected location: $asset"
}
if (-not ($sha -cmatch '^[0-9a-fA-F]{64}$')) { Fail "release manifest does not include a valid SHA-256 checksum for $assetKey" }

if ($version) { Info "downloading v$version..." } else { Info "downloading latest release..." }

$tmp = New-Item -ItemType Directory -Force -Path (Join-Path $env:TEMP ('selfhost-install-' + [guid]::NewGuid().ToString('N')))
try {
    $zipPath = Join-Path $tmp "selfhost.zip"
    try {
        Invoke-WebRequest -Uri $asset -OutFile $zipPath -TimeoutSec 120
    } catch {
        Fail "download failed from $asset"
    }

    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $zipPath).Hash.ToLowerInvariant()
    if ($actual -cne $sha.ToLowerInvariant()) {
        Fail "downloaded selfhost checksum did not match"
    }
    Info "checksum verified"

    Expand-Archive -LiteralPath $zipPath -DestinationPath $tmp -Force
    $exe = Join-Path $tmp "selfhost.exe"
    if (-not (Test-Path $exe)) { Fail "archive did not contain selfhost.exe" }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $dest = Join-Path $InstallDir "selfhost.exe"
    Move-Item -LiteralPath $exe -Destination $dest -Force

    Info "installed $Bin to $dest"

    # add to user PATH if missing
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = $userPath -split ";" | Where-Object { $_ }
    if ($parts -notcontains $InstallDir) {
        $newPath = ($parts + $InstallDir) -join ";"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        $env:Path = "$env:Path;$InstallDir"
        Warn "$InstallDir was added to your user PATH"
        Write-Host "  restart your terminal for it to take effect" -ForegroundColor Yellow
    }

    # verify
    $versionOutput = & $dest --version 2>$null
    if ($LASTEXITCODE -eq 0 -and $versionOutput) {
        Write-Host ""
        Info "ready. run 'selfhost' to get started."
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
Write-Host ""
