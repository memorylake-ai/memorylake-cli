<#
.SYNOPSIS
    Install the memorylake CLI on Windows.

.DESCRIPTION
    Downloads the release archive for this machine's architecture, verifies it
    against the published SHA-256, and installs the binary.

        irm https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.ps1 | iex

    Credentials can be supplied so the install finishes without prompting — what
    a web console hands out, and what CI needs. `irm | iex` cannot pass
    parameters to a script, so these come from the environment:

        $env:MEMORYLAKE_API_KEY='sk-…'; $env:MEMORYLAKE_WORKSPACE='ws-…'; irm … | iex

    Environment overrides:
        MEMORYLAKE_API_KEY       log in with this key, no prompting
        MEMORYLAKE_WORKSPACE     remember this workspace, no prompting
        MEMORYLAKE_BASE_URL      endpoint to log in to (default: app.memorylake.ai)
        MEMORYLAKE_VERSION       release tag to install (default: latest)
        MEMORYLAKE_INSTALL_DIR   where to put the binary (default: %LOCALAPPDATA%\memorylake\bin)
        MEMORYLAKE_INSTALL_NAME  name to install it as, without .exe (default: memorylake)
        MEMORYLAKE_NO_SETUP      skip the guided setup entirely

    Note that a key assigned on the command line is recorded in PowerShell's
    history. Prefer a short-lived key where that matters.
#>

# Stop on the first error. Note this does not cover native executables, whose
# failures surface through $LASTEXITCODE, so the download path checks explicitly.
$ErrorActionPreference = 'Stop'

# Windows PowerShell 5.1 on older systems still defaults to TLS 1.0/1.1, which
# GitHub refuses. Additive so nothing already enabled is turned off.
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {
    # PowerShell 7 manages this itself and may not expose the setting.
}

$Repo = 'memorylake-ai/memorylake-cli'
$BinName = 'memorylake'

function Get-EnvOrDefault([string]$Name, [string]$Default) {
    $value = [Environment]::GetEnvironmentVariable($Name)
    if ([string]::IsNullOrWhiteSpace($value)) { return $Default }
    return $value
}

# Set by Invoke-Setup when the install already has usable credentials, so the
# closing message does not tell someone to log in seconds after they just did.
$script:SetupDone = $false

$Version = Get-EnvOrDefault 'MEMORYLAKE_VERSION' 'latest'
$InstallDir = Get-EnvOrDefault 'MEMORYLAKE_INSTALL_DIR' (Join-Path $env:LOCALAPPDATA 'memorylake\bin')
$InstallName = Get-EnvOrDefault 'MEMORYLAKE_INSTALL_NAME' $BinName
$ApiKey = Get-EnvOrDefault 'MEMORYLAKE_API_KEY' ''
$Workspace = Get-EnvOrDefault 'MEMORYLAKE_WORKSPACE' ''
$BaseUrl = Get-EnvOrDefault 'MEMORYLAKE_BASE_URL' 

function Write-Info([string]$Message) {
    Write-Host $Message
}

function Stop-WithError([string]$Message) {
    Write-Error $Message
    exit 1
}

# Map the OS architecture onto the Rust target triple the release is built for.
#
# Two sources, in that order, because neither works everywhere:
#
# * `RuntimeInformation::OSArchitecture` is the accurate one — it reports the
#   *OS* architecture even from an emulated process — but it is a .NET Core API.
#   Windows PowerShell 5.1 runs on .NET Framework, where the type is missing and
#   the expression yields nothing at all.
# * The environment variables always exist. `PROCESSOR_ARCHITECTURE` alone is
#   the process architecture, which is wrong for a 32-bit shell on a 64-bit OS —
#   but that is exactly when `PROCESSOR_ARCHITEW6432` is set, and it holds the
#   OS architecture. Preferring it when present covers the gap.
function Get-Target {
    $arch = $null
    try {
        $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    } catch {
        # No such type on this runtime; fall through to the environment.
    }

    if ([string]::IsNullOrWhiteSpace($arch)) {
        $arch = if ($env:PROCESSOR_ARCHITEW6432) {
            $env:PROCESSOR_ARCHITEW6432
        } else {
            $env:PROCESSOR_ARCHITECTURE
        }
    }

    # `switch` is case-insensitive by default, so one label covers X64/x64 and
    # ARM64/Arm64. AMD64 is the environment variable's spelling of X64.
    switch ($arch) {
        'X64' { return 'x86_64-pc-windows-msvc' }
        'AMD64' { return 'x86_64-pc-windows-msvc' }
        'ARM64' { return 'aarch64-pc-windows-msvc' }
        default {
            Stop-WithError "unsupported architecture: '$arch'. This CLI ships x64 and ARM64 Windows builds; see https://github.com/$Repo/releases"
        }
    }
}

# Resolve `latest` to a concrete tag.
#
# Reads the releases API rather than the redirect on /releases/latest. The
# redirect is cheaper, but getting the Location header out of it differs by
# runtime — Windows PowerShell 5.1 hands back a Dictionary where `.Location`
# is not a property, PowerShell 7 an HttpResponseMessage — and quietly yielding
# nothing is worse than one extra request. `Invoke-RestMethod` behaves the same
# on both.
function Resolve-Version([string]$Requested) {
    if ($Requested -ne 'latest') { return $Requested }

    $url = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $release = Invoke-RestMethod -Uri $url -UseBasicParsing -Headers @{
            'Accept'     = 'application/vnd.github+json'
            'User-Agent' = 'memorylake-cli-installer'
        }
    } catch {
        Stop-WithError "could not reach the releases API ($($_.Exception.Message)).`n  Set MEMORYLAKE_VERSION to a tag such as v20260818.1 to skip this lookup."
    }

    $tag = $release.tag_name
    if ([string]::IsNullOrWhiteSpace($tag)) {
        Stop-WithError 'the releases API returned no tag; set MEMORYLAKE_VERSION to a tag such as v20260818.1'
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
        Invoke-Setup $destination
        Show-NextStep
    } finally {
        Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Walk a fresh install through logging in and picking a workspace.
#
# `irm | iex` leaves stdin usable for prompts, unlike the Unix `curl | sh` case,
# so the CLI's own interactive commands are simply invoked. When the host is not
# interactive — a CI runner, a provisioning script — the commands are printed
# instead, so nothing blocks on a prompt nobody can answer.
#
# Skipped entirely when MEMORYLAKE_NO_SETUP is set.
function Invoke-Setup([string]$BinPath) {
    if (-not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable('MEMORYLAKE_NO_SETUP'))) {
        return
    }

    # Credentials supplied by the caller finish the setup without prompting.
    #
    # Checked before the already-logged-in test below, and deliberately so: a
    # supplied key is an instruction to use *that* key, not a coincidence to be
    # skipped because some other one is already stored. It also runs before the
    # interactivity test, so a console-generated command or a CI job configures
    # the install completely with no terminal at all.
    if (-not [string]::IsNullOrWhiteSpace($ApiKey)) {
        Write-Info ''
        $loginArgs = @('auth', 'login', '--api-key', $ApiKey)
        if (-not [string]::IsNullOrWhiteSpace($BaseUrl)) {
            $loginArgs += @('--base-url', $BaseUrl)
        }
        # `auth login` validates against the API before storing anything, so a
        # key that will not work fails here rather than on first use.
        & $BinPath @loginArgs
        if ($LASTEXITCODE -ne 0) {
            Write-Info ''
            Write-Info 'the supplied API key was not accepted; nothing was stored.'
            Write-Info "run '$InstallName auth login' to enter one interactively."
            return
        }
        Set-DefaultWorkspace $BinPath
        $script:SetupDone = $true
        return
    }

    # Already logged in? Then this is an upgrade, not a first install.
    #
    # Checked before the interactivity test, because it needs no terminal: an
    # upgrade run from CI or a provisioning script would otherwise be told that
    # setup was skipped and to go log in, when it is already configured.
    #
    # Read the reported state rather than the exit status: `auth status`
    # succeeds either way, because answering "not logged in" is a successful
    # query. A CLI test pins this output so the check cannot rot silently.
    $status = (& $BinPath auth status 2>$null) -join "`n"
    if ($status -match 'Logged in:\s*yes') {
        Write-Info ''
        Write-Info 'already logged in; leaving your credentials and workspace as they are'
        $script:SetupDone = $true
        return
    }

    # $Host.UI.RawUI is unavailable in non-interactive hosts, which is the most
    # reliable signal available here that there is nobody to prompt.
    $interactive = $true
    try { $null = $Host.UI.RawUI.KeyAvailable } catch { $interactive = $false }
    if ([Environment]::UserInteractive -eq $false) { $interactive = $false }

    if (-not $interactive) {
        Write-Info ''
        Write-Info 'no interactive terminal, so setup was skipped. To finish:'
        Write-Info "  $InstallName auth login       # store your API key"
        Write-Info "  $InstallName workspace use    # pick a default workspace"
        return
    }

    Write-Info ''
    Write-Info "Let's get you set up. Ctrl-C to skip — you can run these later."
    Write-Info ''

    # `auth login` prompts for the key and validates it before storing anything.
    & $BinPath auth login
    if ($LASTEXITCODE -ne 0) {
        Write-Info ''
        Write-Info "login did not complete. Run '$InstallName auth login' when ready."
        return
    }

    Set-DefaultWorkspace $BinPath

    # Logged in either way: a skipped workspace is a preference, not a failure.
    $script:SetupDone = $true
}

# Remember a default workspace: the supplied one, or one the user picks.
#
# A missing workspace never fails the install — the CLI works without one, it
# just wants `--workspace` on every call.
function Set-DefaultWorkspace([string]$BinPath) {
    if (-not [string]::IsNullOrWhiteSpace($Workspace)) {
        & $BinPath workspace use $Workspace
        if ($LASTEXITCODE -ne 0) {
            Write-Info ''
            Write-Info "could not use workspace '$Workspace'; run '$InstallName workspace use'"
            Write-Info 'to pick one from your account.'
        }
        return
    }

    # `workspace use` with no argument lists the account's workspaces and lets
    # the user choose — nobody has a workspace id memorised on day one.
    Write-Info ''
    & $BinPath workspace use
    if ($LASTEXITCODE -ne 0) {
        Write-Info ''
        Write-Info "no workspace selected. Run '$InstallName workspace use' to pick one,"
        Write-Info 'or pass --workspace <id> to each command.'
    }
}

# Tell the user how to reach the binary, and add it to the user PATH when it is
# not already there. Only the user-scoped PATH is touched — a machine-scoped
# change would need elevation and affect every account.
function Show-PathHint {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @()
    if ($userPath) { $entries = $userPath -split ';' | Where-Object { $_ } }

    if (-not ($entries -contains $InstallDir)) {
        $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
        # The change lands in the registry, so it reaches new shells only; update
        # this session too, otherwise the very next command would fail.
        $env:Path = "$env:Path;$InstallDir"

        Write-Info ''
        Write-Info "added $InstallDir to your user PATH (new terminals will see it)"
    }
}

# Say what to do next. Kept apart from the PATH note: whether the binary is
# reachable and whether it is configured are different questions, and telling
# someone to log in seconds after they did reads like the setup failed.
function Show-NextStep {
    Write-Info ''
    if ($script:SetupDone) {
        Write-Info "you're set. Try '$InstallName workspace current' or '$InstallName project list'."
    } else {
        Write-Info "run '$InstallName auth login' to get started"
    }
}

Install-Memorylake
