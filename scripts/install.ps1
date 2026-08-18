<#
.SYNOPSIS
    Install the memorylake CLI on Windows.

.DESCRIPTION
    Downloads the release archive for this machine's architecture, verifies it
    against the published SHA-256, and installs the binary.

        irm https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.ps1 | iex

    Environment overrides:
        MEMORYLAKE_VERSION       release tag to install (default: latest)
        MEMORYLAKE_INSTALL_DIR   where to put the binary (default: %LOCALAPPDATA%\memorylake\bin)
        MEMORYLAKE_INSTALL_NAME  name to install it as, without .exe (default: memorylake)
#>

# Stop on the first error. Note this does not cover native executables, whose
# failures surface through $LASTEXITCODE, so the download path checks explicitly.
$ErrorActionPreference = 'Stop'

$Repo = 'memorylake-ai/memorylake-cli'
$BinName = 'memorylake'

function Get-EnvOrDefault([string]$Name, [string]$Default) {
    $value = [Environment]::GetEnvironmentVariable($Name)
    if ([string]::IsNullOrWhiteSpace($value)) { return $Default }
    return $value
}

$Version = Get-EnvOrDefault 'MEMORYLAKE_VERSION' 'latest'
$InstallDir = Get-EnvOrDefault 'MEMORYLAKE_INSTALL_DIR' (Join-Path $env:LOCALAPPDATA 'memorylake\bin')
$InstallName = Get-EnvOrDefault 'MEMORYLAKE_INSTALL_NAME' $BinName

function Write-Info([string]$Message) {
    Write-Host $Message
}

function Stop-WithError([string]$Message) {
    Write-Error $Message
    exit 1
}

# Map the process architecture onto the Rust target triple the release is built
# for. PROCESSOR_ARCHITECTURE is deliberately not used: it reports the *process*
# architecture, so an x86 PowerShell on an ARM64 machine would pick the wrong
# build.
function Get-Target {
    switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        'X64' { return 'x86_64-pc-windows-msvc' }
        'Arm64' { return 'aarch64-pc-windows-msvc' }
        default {
            Stop-WithError "unsupported architecture: $([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)"
        }
    }
}

# Resolve `latest` to a concrete tag by reading the redirect GitHub serves from
# /releases/latest, rather than the JSON API, which rate-limits unauthenticated
# callers far more aggressively.
function Resolve-Version([string]$Requested) {
    if ($Requested -ne 'latest') { return $Requested }

    $url = "https://github.com/$Repo/releases/latest"
    try {
        $response = Invoke-WebRequest -Uri $url -MaximumRedirection 0 -ErrorAction SilentlyContinue
        $location = $response.Headers.Location
    } catch {
        # PowerShell 5.1 throws on a 3xx when redirects are disabled; the
        # response still carries the header we need.
        $location = $_.Exception.Response.Headers.Location
    }

    if (-not $location) {
        # Last resort: follow redirects and read where we landed.
        $final = Invoke-WebRequest -Uri $url -UseBasicParsing
        $location = $final.BaseResponse.ResponseUri
    }

    $tag = ([string]$location -split '/tag/')[-1]
    if ([string]::IsNullOrWhiteSpace($tag)) {
        Stop-WithError 'could not resolve the latest release; set MEMORYLAKE_VERSION to a tag such as v20260818'
    }
    return $tag.Trim()
}

function Save-File([string]$Url, [string]$Path) {
    try {
        Invoke-WebRequest -Uri $Url -OutFile $Path -UseBasicParsing
    } catch {
        Stop-WithError "could not download $Url`n  $($_.Exception.Message)"
    }
}

# Verify the archive against the .sha256 published beside it.
#
# The file is in `shasum` format ("<hex>  <filename>"), so only the first field
# is compared.
function Test-Checksum([string]$Archive, [string]$SumsFile) {
    $expected = ((Get-Content $SumsFile -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash -Path $Archive -Algorithm SHA256).Hash

    if ($expected -ine $actual) {
        Stop-WithError "checksum verification failed for $(Split-Path $Archive -Leaf); refusing to install`n  expected $expected`n  actual   $actual"
    }
}

function Install-Memorylake {
    $target = Get-Target
    $version = Resolve-Version $Version

    $stem = "$BinName-$version-$target"
    $archiveName = "$stem.zip"
    $baseUrl = "https://github.com/$Repo/releases/download/$version"

    Write-Info "installing $BinName $version ($target)"

    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
    New-Item -ItemType Directory -Path $tmp -Force | Out-Null
    try {
        $archive = Join-Path $tmp $archiveName
        Save-File "$baseUrl/$archiveName" $archive
        Save-File "$baseUrl/$archiveName.sha256" "$archive.sha256"

        Test-Checksum $archive "$archive.sha256"
        Expand-Archive -Path $archive -DestinationPath $tmp -Force

        $unpacked = Join-Path $tmp "$stem\$BinName.exe"
        if (-not (Test-Path $unpacked)) {
            Stop-WithError "the archive did not contain $BinName.exe as expected"
        }

        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        $destination = Join-Path $InstallDir "$InstallName.exe"

        # Windows locks a running executable, so a plain copy over one fails.
        # Move the old file aside first: the rename succeeds even while it runs,
        # and the leftover is cleaned up on the next install.
        if (Test-Path $destination) {
            $stale = "$destination.old"
            Remove-Item $stale -Force -ErrorAction SilentlyContinue
            try {
                Move-Item $destination $stale -Force
            } catch {
                Stop-WithError "$destination is in use and could not be replaced; close any running $InstallName and try again"
            }
        }
        Copy-Item $unpacked $destination -Force

        Write-Info "installed $destination"
        Show-PathHint
    } finally {
        Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Tell the user how to reach the binary, and add it to the user PATH when it is
# not already there. Only the user-scoped PATH is touched — a machine-scoped
# change would need elevation and affect every account.
function Show-PathHint {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @()
    if ($userPath) { $entries = $userPath -split ';' | Where-Object { $_ } }

    if ($entries -contains $InstallDir) {
        Write-Info ''
        Write-Info "run '$InstallName auth login' to get started"
        return
    }

    $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    # The change lands in the registry, so it reaches new shells only; update
    # this session too, otherwise the very next command would fail.
    $env:Path = "$env:Path;$InstallDir"

    Write-Info ''
    Write-Info "added $InstallDir to your user PATH"
    Write-Info "open a new terminal, then run '$InstallName auth login' to get started"
}

Install-Memorylake
