<#
.SYNOPSIS
Installs, updates or removes Zombies on Windows, from the game's latest GitHub release.

.DESCRIPTION
install  Downloads the latest release into %LOCALAPPDATA%\Zombies and adds a Start Menu shortcut.
update   Brings an install up to the latest release. Refuses while the game is running. The
         assets (about 300 MB) are downloaded only when they changed. The previous zombies.exe is
         kept once, as zombies.exe.old.
remove   Deletes the install and its shortcut. Saves, settings and Run Records (%APPDATA%\zombies)
         are kept unless -Purge is given.

Every download is checked against the SHA-256 in the release's release.json before anything in
the install is touched. No admin rights are needed.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File zombies.ps1 install
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('install', 'update', 'remove')]
    [string]$Command,

    # With remove: also delete saves, settings and Run Records.
    [switch]$Purge
)

Set-StrictMode -Version 3
$ErrorActionPreference = 'Stop'

# Where the game lives, and the repository whose latest release it is.
$InstallDir = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Zombies'))
$Repo = 'jchevertonwynne/zombies'

# This script's own text, whether it runs from a file or from the one-line install, so the install
# can keep a copy for later updates and removal.
$SelfText = $MyInvocation.MyCommand.ScriptBlock.ToString()

# --- Decisions (no side effects; tested in tests/zombies.Tests.ps1) ---------------------------

# What an update must download, given the installed release.json (or $null when there is no
# install), the latest one, and whether the install has its assets folder. The game is fetched
# whenever the build differs; the assets only when their hash does, or when they're missing.
function Get-UpdatePlan {
    param($Installed, [Parameter(Mandatory)]$Latest, [bool]$HasAssets = $true)
    if ($null -eq $Installed) {
        return [pscustomobject]@{ Game = $true; Assets = $true }
    }
    [pscustomobject]@{
        Game   = $Installed.build_id -ne $Latest.build_id
        Assets = (-not $HasAssets) -or ($Installed.assets.hash -ne $Latest.assets.hash)
    }
}

# Throws unless a release.json has the shape the release workflow writes: a plain version for a
# build id (it becomes part of a download URL) and bare zip names (they become paths here).
function Assert-ReleaseShape {
    param([Parameter(Mandatory)]$Release)
    if ($Release.build_id -notmatch '^[0-9A-Za-z][0-9A-Za-z.+-]*$') {
        throw "The release's build id '$($Release.build_id)' isn't a plain version."
    }
    foreach ($entry in $Release.game, $Release.assets) {
        if ($entry.file -notmatch '^[0-9A-Za-z][0-9A-Za-z._-]*\.zip$' -or $entry.file -match '\.\.') {
            throw "The release names a file '$($entry.file)' that isn't a plain file name."
        }
    }
}

# Throws unless the file's SHA-256 is the one expected.
function Assert-Sha256 {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Expected)
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash
    if ($actual -ne $Expected) {
        throw "$(Split-Path -Leaf $Path) is corrupt or was tampered with: its SHA-256 is $($actual.ToLowerInvariant()), the release says $($Expected.ToLowerInvariant())."
    }
}

# Whether a folder is a Zombies install, and so safe to update or delete: it holds the
# release.json this script writes once an install has finished.
function Test-ZombiesInstall {
    param([Parameter(Mandatory)][string]$Dir)
    Test-Path -LiteralPath (Join-Path $Dir 'release.json') -PathType Leaf
}

# What an install puts in its folder, and nothing else.
$InstallFiles = @('zombies.exe', 'zombies.exe.old', 'assets', 'assets.old', '.staging', 'release.json', 'zombies.ps1', 'LICENSE-MIT', 'LICENSE-APACHE', 'README.md')

# Whether a fresh install may go into a folder: one that is missing, or holds nothing but what an
# install puts there (an install that was interrupted). Install never mixes the game into
# someone's own files, which remove would later delete.
function Test-InstallTarget {
    param([Parameter(Mandatory)][string]$Dir)
    if (-not (Test-Path -LiteralPath $Dir)) { return $true }
    @(Get-ChildItem -LiteralPath $Dir -Force | Where-Object { $InstallFiles -notcontains $_.Name }).Count -eq 0
}

# --- Effects ------------------------------------------------------------------------------------

function Get-ShortcutPath {
    Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Zombies.lnk'
}

function Get-SaveDir {
    Join-Path $env:APPDATA 'zombies'
}

function Read-Json([string]$Path) {
    Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

# Downloads one file of a release: the latest one, or the one tagged `$Tag`.
function Save-Download([string]$Name, [string]$To, [string]$Tag) {
    $url = if ($Tag) { "https://github.com/$Repo/releases/download/$Tag/$Name" } else { "https://github.com/$Repo/releases/latest/download/$Name" }
    Write-Host "Downloading $Name"
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $To
    } catch {
        throw "Couldn't download $url ($($_.Exception.Message)). Check your connection, and that $Repo has a published release."
    }
}

function Get-GamePath {
    Join-Path $InstallDir 'zombies.exe'
}

# The installed game's processes.
function Get-RunningGame {
    $exe = Get-GamePath
    @(Get-Process -Name zombies -ErrorAction SilentlyContinue | Where-Object {
        try { [IO.Path]::GetFullPath($_.Path) -eq $exe } catch { $false }
    })
}

function Assert-NotRunning {
    if (@(Get-RunningGame).Count -gt 0) {
        throw 'Zombies is running. Close the game, then try again.'
    }
}

function New-Shortcut {
    $path = Get-ShortcutPath
    New-Item -ItemType Directory -Force (Split-Path $path) | Out-Null
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($path)
    $link.TargetPath = Get-GamePath
    $link.WorkingDirectory = $InstallDir
    $link.Description = 'Zombies'
    $link.Save()
}

# Downloads, verifies and unpacks what the plan asks for into a staging folder inside the install
# (the same volume, so the swap is a rename), then swaps it in: the assets first, the game next,
# and release.json last. An interrupted update is retried from what release.json still says, and
# the game is only swapped once, so zombies.exe.old stays the build before.
function Install-Latest {
    $installedJson = Join-Path $InstallDir 'release.json'
    $installed = if (Test-Path -LiteralPath $installedJson) { Read-Json $installedJson } else { $null }

    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    $temp = Join-Path $InstallDir '.staging'
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    New-Item -ItemType Directory $temp | Out-Null
    try {
        Save-Download 'release.json' (Join-Path $temp 'release.json')
        $latest = Read-Json (Join-Path $temp 'release.json')
        Assert-ReleaseShape $latest
        $plan = Get-UpdatePlan -Installed $installed -Latest $latest -HasAssets (Test-Path -LiteralPath (Join-Path $InstallDir 'assets'))
        if (-not ($plan.Game -or $plan.Assets)) {
            Write-Host "Zombies $($latest.build_id) is already installed and up to date."
            return
        }

        # Download and check everything before touching the install.
        $parts = @()
        if ($plan.Game) { $parts += [pscustomobject]@{ Folder = 'game'; Entry = $latest.game } }
        if ($plan.Assets) { $parts += [pscustomobject]@{ Folder = 'assets'; Entry = $latest.assets } }
        foreach ($part in $parts) {
            $zip = Join-Path $temp $part.Entry.file
            # From the release just read, by its tag, so one published meanwhile can't be mixed in.
            Save-Download $part.Entry.file $zip -Tag "v$($latest.build_id)"
            Assert-Sha256 -Path $zip -Expected $part.Entry.sha256
            Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $temp $part.Folder)
            Remove-Item -LiteralPath $zip
        }

        if ($plan.Assets) {
            $assets = Join-Path $InstallDir 'assets'
            $old = Join-Path $InstallDir 'assets.old'
            if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Recurse -Force }
            if (Test-Path -LiteralPath $assets) { Move-Item -LiteralPath $assets $old }
            Move-Item -LiteralPath (Join-Path $temp 'assets\assets') $assets
            if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Recurse -Force }
        }
        if ($plan.Game) {
            $exe = Get-GamePath
            if (Test-Path -LiteralPath $exe) {
                Move-Item -LiteralPath $exe "$exe.old" -Force
            }
            Get-ChildItem -LiteralPath (Join-Path $temp 'game') | Move-Item -Destination $InstallDir -Force
        }
        Copy-Item -LiteralPath (Join-Path $temp 'release.json') $installedJson -Force

        $from = if ($null -eq $installed) { '' } else { " (was $($installed.build_id))" }
        $what = if ($plan.Assets) { 'game and assets' } else { 'game only; assets unchanged' }
        Write-Host "Installed Zombies $($latest.build_id)$from in $InstallDir ($what)."
    } finally {
        Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Keeps a copy of this script in the install, so update and remove also work offline.
function Copy-Self {
    [IO.File]::WriteAllText((Join-Path $InstallDir 'zombies.ps1'), $SelfText, (New-Object Text.UTF8Encoding $true))
}

function Invoke-Install {
    if (Test-ZombiesInstall $InstallDir) {
        Write-Host "Zombies is already installed in $InstallDir; updating it."
        Invoke-Update
    } elseif (Test-InstallTarget $InstallDir) {
        Install-Latest
        Copy-Self
    } else {
        throw "$InstallDir already exists and isn't a Zombies install, so nothing was installed into it. Move or delete it, then try again."
    }
    New-Shortcut
    Write-Host 'Start it from the Start Menu: Zombies.'
}

function Invoke-Update {
    if (-not (Test-ZombiesInstall $InstallDir)) {
        throw "Zombies isn't installed in $InstallDir. Run install first."
    }
    Assert-NotRunning
    Install-Latest
    Copy-Self
}

function Invoke-Remove {
    if (Test-ZombiesInstall $InstallDir) {
        Assert-NotRunning
        Remove-Item -LiteralPath $InstallDir -Recurse -Force
        Write-Host "Removed $InstallDir."
    } elseif (Test-Path -LiteralPath $InstallDir) {
        throw "$InstallDir doesn't look like a Zombies install (it has no release.json), so it was left alone."
    } else {
        Write-Host "Zombies isn't installed in $InstallDir."
    }
    $shortcut = Get-ShortcutPath
    if (Test-Path -LiteralPath $shortcut) { Remove-Item -LiteralPath $shortcut -Force }
    $saves = Get-SaveDir
    if ($Purge) {
        if (Test-Path -LiteralPath $saves) {
            Remove-Item -LiteralPath $saves -Recurse -Force
            Write-Host "Deleted saves, settings and Run Records in $saves."
        }
    } elseif (Test-Path -LiteralPath $saves) {
        Write-Host "Kept saves, settings and Run Records in $saves (remove -Purge deletes them)."
    }
}

# Dot-sourcing (as the tests do) only defines the functions.
if ($MyInvocation.InvocationName -ne '.') {
    if (-not $Command) { throw 'Say what to do: install, update or remove.' }
    # Windows PowerShell 5.1 may not offer TLS 1.2 by default, and its progress bar slows downloads to a crawl.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $ProgressPreference = 'SilentlyContinue'
    switch ($Command) {
        'install' { Invoke-Install }
        'update' { Invoke-Update }
        'remove' { Invoke-Remove }
    }
}
