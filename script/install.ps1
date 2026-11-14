<#
.SYNOPSIS
    SDWAN Windows Installer

.DESCRIPTION
    Download SDWAN from the self-hosted release channel and install it.
    Copies binaries to the install directory and updates the system PATH.

    Release channel layout (any plain static file host works):
      <BaseUrl>/VERSION                                  -> plain text tag, e.g. v2.6.4
      <BaseUrl>/<tag>/sdwan-windows-<arch>-<tag>.zip    -> archive containing sdwan-core.exe / sdwan-cli.exe

    If <BaseUrl> is https://github.com/<owner>/<repo>, that repository Releases is used:
      /releases/latest/download/VERSION
      /releases/download/<tag>/sdwan-windows-<arch>-<tag>.zip
    For a private repository set SDWAN_RELEASE_TOKEN (or GITHUB_TOKEN).

.PARAMETER Version
    Target version: "latest", "stable", or a specific tag like "v2.6.2".
    "latest"/"stable" resolve <BaseUrl>/VERSION. Default: "latest"

.PARAMETER InstallDir
    Directory to install SDWAN binaries.
    Default: "$env:ProgramFiles\SDWAN"

.PARAMETER ReleaseBaseUrl
    Root URL of the self-hosted release channel.
    Defaults to the SDWAN_RELEASE_BASE_URL environment variable.

.EXAMPLE
    .\install.ps1
    .\install.ps1 -Version v2.6.2
    .\install.ps1 -InstallDir "C:\SDWAN" -ReleaseBaseUrl "https://github.com/zzxym/sdwan"

.NOTES
    Administrator privileges are required.
    After installation, run: sdwan-cli service install
    to register SDWAN as a system service.
#>
param(
    [Parameter(Position = 0)]
    [ValidatePattern('^(stable|latest|v?\d+\.\d+\.\d+(-[^\s]+)?)$')]
    [string]$Version = 'latest',

    [Parameter(Position = 1)]
    [string]$InstallDir = "$env:ProgramFiles\SDWAN",

    [Parameter(Position = 2)]
    [string]$ReleaseBaseUrl = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# Force TLS 1.2+ for release channel requests
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
if ([string]::IsNullOrWhiteSpace($ReleaseBaseUrl)) {
    $ReleaseBaseUrl = [Environment]::GetEnvironmentVariable('SDWAN_RELEASE_BASE_URL')
}
if ([string]::IsNullOrWhiteSpace($ReleaseBaseUrl)) {
    Write-Error 'Self-hosted release channel is not configured. Set SDWAN_RELEASE_BASE_URL or pass -ReleaseBaseUrl.'
    exit 1
}
$RELEASE_BASE_URL = $ReleaseBaseUrl.TrimEnd('/')

$releaseToken = $env:SDWAN_RELEASE_TOKEN
if ([string]::IsNullOrWhiteSpace($releaseToken)) { $releaseToken = $env:GITHUB_TOKEN }
$requestHeaders = @{ 'User-Agent' = 'SDWAN-Installer/1.0' }
if (-not [string]::IsNullOrWhiteSpace($releaseToken)) {
    $requestHeaders['Authorization'] = "Bearer $releaseToken"
}

# A base URL such as https://github.com/<owner>/<repo> maps to that
# repository Releases; any other URL is treated as a plain static host.
if ($RELEASE_BASE_URL -match '^https://github[.]com/[^/]+/[^/]+/?$') {
    $RELEASE_INDEX_URL = "$RELEASE_BASE_URL/releases/latest/download/VERSION"
    $RELEASE_ASSET_BASE = "$RELEASE_BASE_URL/releases/download"
}
else {
    $RELEASE_INDEX_URL = "$RELEASE_BASE_URL/VERSION"
    $RELEASE_ASSET_BASE = $RELEASE_BASE_URL
}

# ---------------------------------------------------------------------------
# Administrator check
# ---------------------------------------------------------------------------
$currentPrincipal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error 'Please run this script as Administrator.'
    exit 1
}

# ---------------------------------------------------------------------------
# Architecture detection
# ---------------------------------------------------------------------------
# Check PROCESSOR_ARCHITEW6432 first to correctly identify 64-bit OS when
# running under 32-bit PowerShell (WoW64), where PROCESSOR_ARCHITECTURE
# reports 'x86' even on a 64-bit machine.
$cpuArch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
switch ($cpuArch) {
    'AMD64' { $arch = 'x86_64' }
    'ARM64' { $arch = 'arm64'  }
    'x86'   { $arch = 'i686'   }
    default {
        Write-Error "Unsupported processor architecture: $cpuArch"
        exit 1
    }
}
$assetBaseName = "sdwan-windows-$arch"

Write-Host ''
Write-Host '  ===============================================' -ForegroundColor Cyan
Write-Host '        SDWAN Windows Installer                 ' -ForegroundColor Cyan
Write-Host '  ===============================================' -ForegroundColor Cyan
Write-Host ''
Write-Host "  Architecture : $arch" -ForegroundColor White
Write-Host "  Channel      : $RELEASE_BASE_URL" -ForegroundColor White
Write-Host ''

# ---------------------------------------------------------------------------
# Step 1 - Resolve release version
# ---------------------------------------------------------------------------
Write-Host '[1/5] Querying self-hosted release channel...' -ForegroundColor Yellow

try {
    if ($Version -eq 'latest' -or $Version -eq 'stable') {
        $versionTag = (Invoke-WebRequest -Uri $RELEASE_INDEX_URL -UseBasicParsing `
            -Headers $requestHeaders -ErrorAction Stop).Content.Trim()
    }
    else {
        $versionTag = if ($Version -notmatch '^v') { "v$Version" } else { $Version }
    }
}
catch {
    Write-Error "Failed to read $RELEASE_INDEX_URL : $_`nPlease check the channel or download manually from $RELEASE_BASE_URL."
    exit 1
}

if ($versionTag -notmatch '^v?\d+\.\d+\.\d+') {
    Write-Error "Invalid version tag '$versionTag' returned by $RELEASE_INDEX_URL."
    exit 1
}

$releaseVersion = $versionTag
$assetZipName   = "$assetBaseName-$releaseVersion.zip"

Write-Host "  Version : $releaseVersion" -ForegroundColor Green

# ---------------------------------------------------------------------------
# Step 2 - Resolve download URL
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '[2/5] Resolving download URL...' -ForegroundColor Yellow
$downloadUrl = "$RELEASE_ASSET_BASE/$releaseVersion/$assetZipName"
Write-Host "  URL     : $downloadUrl" -ForegroundColor DarkGray

# ---------------------------------------------------------------------------
# Step 3 - Download ZIP
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host "[3/5] Downloading $assetZipName ..." -ForegroundColor Yellow

$tempDir = Join-Path $env:TEMP "sdwan-install-$(Get-Random)"
$zipPath = Join-Path $tempDir $assetZipName

New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -Headers $requestHeaders -ErrorAction Stop
    $sizeMB = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
    Write-Host "  Download complete ($sizeMB MB)" -ForegroundColor Green
}
catch {
    Write-Error "Download failed: $_`nExpected asset: $downloadUrl`nPlease check the release channel."
    Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    exit 1
}

# ---------------------------------------------------------------------------
# Step 4 - Extract & copy to install directory
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '[4/5] Extracting and copying files...' -ForegroundColor Yellow

$extractDir = Join-Path $tempDir 'extracted'
New-Item -ItemType Directory -Force -Path $extractDir | Out-Null

try {
    Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force -ErrorAction Stop
}
catch {
    Write-Error "Extraction failed: $_"
    Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    exit 1
}

# ZIP may contain a sub-directory; find exe files recursively and flatten
$exeFiles = Get-ChildItem -Path $extractDir -Filter '*.exe' -Recurse
if (-not $exeFiles) {
    Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    Write-Error 'No .exe files found after extraction. The ZIP may be malformed.'
    exit 1
}

$binSourceDir = $exeFiles[0].DirectoryName

try {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Get-ChildItem -Path $binSourceDir | Copy-Item -Destination $InstallDir -Force
}
catch {
    Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    Write-Error "Failed to copy files to install directory: $_"
    exit 1
}

Write-Host "  Installed to: $InstallDir" -ForegroundColor Green

# ---------------------------------------------------------------------------
# Step 5 - Update system PATH
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '[5/5] Updating system PATH...' -ForegroundColor Yellow

$systemPath = [Environment]::GetEnvironmentVariable('PATH', 'Machine')
# Split on ';' and normalize (trim trailing backslash, case-insensitive) for an exact match
$pathEntries = $systemPath -split ';' | ForEach-Object { $_.TrimEnd('\') }
$normalizedInstallDir = $InstallDir.TrimEnd('\')
if ($pathEntries -inotcontains $normalizedInstallDir) {
    [Environment]::SetEnvironmentVariable('PATH', "$systemPath;$InstallDir", 'Machine')
    $env:PATH = "$env:PATH;$InstallDir"
    Write-Host "  Added $InstallDir to system PATH" -ForegroundColor Green
}
else {
    Write-Host "  $InstallDir is already in PATH, skipping" -ForegroundColor DarkGray
}

# ---------------------------------------------------------------------------
# Cleanup
# ---------------------------------------------------------------------------
try {
    Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
}
catch {
    Write-Warning "Could not remove temp dir $tempDir : $_"
}

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host "  [OK] SDWAN $releaseVersion installation complete!" -ForegroundColor Green
Write-Host ''
Write-Host "  Install dir : $InstallDir" -ForegroundColor White
Write-Host "  Channel     : $RELEASE_BASE_URL" -ForegroundColor DarkGray
Write-Host ''
Write-Host '  NOTE: If PATH was just updated, please restart your terminal.' -ForegroundColor DarkYellow
Write-Host ''
