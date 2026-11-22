<#
.SYNOPSIS
Installs, updates or removes Zombies for the current Windows user. No administrator rights needed.

.DESCRIPTION
  install   Download the latest release into %LOCALAPPDATA%\Zombies and add a Start Menu shortcut.
  update    Install the latest release over the current one. Refuses while the game is running.
  remove    Delete the install and the shortcut. Run Records and settings are kept.
  remove -Purge
            Also delete the Run Records and settings (%APPDATA%\zombies, and ~/.zombies if present).

The latest release's release.json (see .github/workflows/release.yml) names the game zip and the
assets zip with their SHA-256s. Every download is verified before anything in the install changes.
The assets zip is skipped when the installed assets hash already matches. An update downloads and
unpacks into a temp folder inside the install, then swaps; the previous zombies.exe stays as
zombies.exe.old (one generation only).

One line, from PowerShell:
  & ([scriptblock]::Create((irm https://raw.githubusercontent.com/jchevertonwynne/zombies/main/tools/zombies.ps1))) install

.PARAMETER Command
install, update or remove.

.PARAMETER Purge
With remove: also delete the saved Run Records and settings.

.PARAMETER Repo
GitHub owner/name to download releases from.

.PARAMETER InstallDir
Where to install. Default: %LOCALAPPDATA%\Zombies.

.PARAMETER BaseUrl
Testing hook: a folder or URL holding release.json and the zips, instead of the GitHub release.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet('install', 'update', 'remove')]
    [string]$Command,
    [switch]$Purge,
    [string]$Repo = 'jchevertonwynne/zombies',
    [string]$InstallDir,
    [string]$BaseUrl
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue' # Invoke-WebRequest is very slow with the progress bar in Windows PowerShell 5.1
try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }

$IsWin = [Environment]::OSVersion.Platform -eq 'Win32NT'

function Fail([string]$Message) { throw "zombies: $Message" }

if ($Purge -and $Command -ne 'remove') { Fail '-Purge only goes with remove.' }

if (-not $InstallDir) {
    if (-not $env:LOCALAPPDATA) { Fail 'LOCALAPPDATA isn''t set; pass -InstallDir.' }
    $InstallDir = Join-Path $env:LOCALAPPDATA 'Zombies'
}
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$ExePath = Join-Path $InstallDir 'zombies.exe'
$StatePath = Join-Path $InstallDir 'install.json'

function Get-ShortcutPath {
    if (-not $env:APPDATA) { return $null }
    Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Zombies.lnk'
}

function Assert-NotRunning {
    $running = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
            $_.ProcessName -eq 'zombies' -and $_.Path -and ($_.Path -eq $ExePath)
        })
    if ($running.Count -gt 0) { Fail 'Zombies is running. Close the game and try again.' }
}

function Get-InstalledState {
    if (-not (Test-Path -LiteralPath $StatePath)) { return $null }
    try { Get-Content -Raw -LiteralPath $StatePath | ConvertFrom-Json } catch { $null }
}

function Get-Remote([string]$Name, [string]$Dest) {
    if ($BaseUrl -and (Test-Path -LiteralPath $BaseUrl -PathType Container)) {
        Copy-Item -LiteralPath (Join-Path $BaseUrl $Name) -Destination $Dest
        return
    }
    $url = if ($BaseUrl) { "$($BaseUrl.TrimEnd('/'))/$Name" } else { "https://github.com/$Repo/releases/latest/download/$Name" }
    try { Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $Dest }
    catch { Fail "couldn't download $url ($($_.Exception.Message)). Is there a published release, and is the repository public?" }
}

function Get-Pinned([string]$Name, [string]$Tag, [string]$Dest) {
    # Once release.json says which build is latest, fetch that exact tag, so a release published
    # between our requests can't mix two builds.
    if ($BaseUrl) { Get-Remote $Name $Dest; return }
    $url = "https://github.com/$Repo/releases/download/$Tag/$Name"
    try { Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $Dest }
    catch { Fail "couldn't download $url ($($_.Exception.Message))." }
}

function Assert-Hash([string]$Path, [string]$Expected, [string]$What) {
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($actual -ne $Expected.ToLowerInvariant()) {
        Fail "$What failed its SHA-256 check (expected $Expected, got $actual). Nothing was changed."
    }
}

function Assert-ZipName([string]$Name) {
    # Names come from the release; refuse anything that isn't a plain file name.
    if ($Name -notmatch '^[A-Za-z0-9._+-]+\.zip$') { Fail "release.json has an odd file name '$Name'." }
}

function New-Shortcut {
    $path = Get-ShortcutPath
    if (-not $IsWin -or -not $path) { Write-Host 'Not Windows; skipping the Start Menu shortcut.'; return }
    New-Item -ItemType Directory -Force -Path (Split-Path $path) | Out-Null
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($path)
    $link.TargetPath = $ExePath
    $link.WorkingDirectory = $InstallDir
    $link.Description = 'Zombies'
    $link.Save()
}

function Install-Release([bool]$IsUpdate) {
    if ($IsUpdate -and -not (Test-Path -LiteralPath $ExePath)) { Fail "Zombies isn't installed at $InstallDir. Run install first." }
    if (-not $IsUpdate -and (Test-Path -LiteralPath $ExePath)) { Fail "Zombies is already installed at $InstallDir. Run update instead." }
    Assert-NotRunning

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $temp = Join-Path $InstallDir ".tmp-$([Guid]::NewGuid().ToString('N').Substring(0, 8))"
    New-Item -ItemType Directory -Path $temp | Out-Null
    try {
        $manifestPath = Join-Path $temp 'release.json'
        Get-Remote 'release.json' $manifestPath
        $release = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
        foreach ($field in 'build_id', 'game', 'assets') {
            if (-not $release.PSObject.Properties[$field]) { Fail "release.json has no '$field'." }
        }
        Assert-ZipName $release.game.file
        Assert-ZipName $release.assets.file
        if ([string]$release.build_id -notmatch '^[0-9A-Za-z.+-]+$') { Fail "release.json has an odd build id '$($release.build_id)'." }
        $tag = "v$($release.build_id)"

        $state = Get-InstalledState
        if ($IsUpdate -and $state -and $state.build_id -eq $release.build_id -and
            $state.assets_hash -eq $release.assets.hash) {
            Write-Host "Zombies $($release.build_id) is already the latest."
            return
        }
        $needAssets = -not ($state -and $state.assets_hash -eq $release.assets.hash -and
            (Test-Path -LiteralPath (Join-Path $InstallDir 'assets') -PathType Container))

        Write-Host "Downloading Zombies $($release.build_id)..."
        $gameZip = Join-Path $temp $release.game.file
        Get-Pinned $release.game.file $tag $gameZip
        Assert-Hash $gameZip $release.game.sha256 $release.game.file
        $assetsZip = $null
        if ($needAssets) {
            $assetsZip = Join-Path $temp $release.assets.file
            Get-Pinned $release.assets.file $tag $assetsZip
            Assert-Hash $assetsZip $release.assets.sha256 $release.assets.file
        } else {
            Write-Host 'Assets unchanged; not downloading them.'
        }

        $new = Join-Path $temp 'new'
        New-Item -ItemType Directory -Path $new | Out-Null
        Expand-Archive -LiteralPath $gameZip -DestinationPath $new
        if ($assetsZip) { Expand-Archive -LiteralPath $assetsZip -DestinationPath $new }
        if (-not (Test-Path -LiteralPath (Join-Path $new 'zombies.exe'))) { Fail 'the game zip has no zombies.exe.' }
        if ($assetsZip -and -not (Test-Path -LiteralPath (Join-Path $new 'assets') -PathType Container)) { Fail 'the assets zip has no assets folder.' }

        # Everything is downloaded, verified and unpacked. Now swap, rolling back if a move fails.
        $oldExe = "$ExePath.old"
        $assetsDir = Join-Path $InstallDir 'assets'
        $assetsBackup = Join-Path $temp 'assets-previous'
        $movedExe = $false; $movedAssets = $false
        try {
            if ($needAssets -and (Test-Path -LiteralPath $assetsDir)) {
                Move-Item -LiteralPath $assetsDir -Destination $assetsBackup
                $movedAssets = $true
            }
            if (Test-Path -LiteralPath $ExePath) {
                if (Test-Path -LiteralPath $oldExe) { Remove-Item -LiteralPath $oldExe -Force }
                Move-Item -LiteralPath $ExePath -Destination $oldExe
                $movedExe = $true
            }
            foreach ($item in Get-ChildItem -LiteralPath $new -Force) {
                $target = Join-Path $InstallDir $item.Name
                if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
                Move-Item -LiteralPath $item.FullName -Destination $target
            }
        } catch {
            $err = $_
            Write-Warning "swap failed ($($err.Exception.Message)); restoring the previous install."
            if ($movedAssets) {
                if (Test-Path -LiteralPath $assetsDir) { Remove-Item -LiteralPath $assetsDir -Recurse -Force -ErrorAction SilentlyContinue }
                Move-Item -LiteralPath $assetsBackup -Destination $assetsDir -ErrorAction SilentlyContinue
            }
            if ($movedExe) {
                Remove-Item -LiteralPath $ExePath -Force -ErrorAction SilentlyContinue
                Move-Item -LiteralPath $oldExe -Destination $ExePath -ErrorAction SilentlyContinue
            }
            Fail "the swap failed and was rolled back: $($err.Exception.Message)"
        }

        @{ build_id = [string]$release.build_id; assets_hash = $release.assets.hash } |
            ConvertTo-Json | Set-Content -Encoding UTF8 -LiteralPath $StatePath
        New-Shortcut
        Write-Host "Zombies $($release.build_id) is installed at $InstallDir."
    } finally {
        Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Remove-Install {
    if (-not (Test-Path -LiteralPath $InstallDir)) {
        Write-Host "Zombies isn't installed at $InstallDir."
    } else {
        # Only ever delete a folder that is recognisably ours.
        if (-not (Test-Path -LiteralPath $ExePath) -and -not (Test-Path -LiteralPath $StatePath)) {
            Fail "$InstallDir doesn't look like a Zombies install (no zombies.exe or install.json); not deleting it."
        }
        Assert-NotRunning
        Remove-Item -LiteralPath $InstallDir -Recurse -Force
        Write-Host "Removed $InstallDir."
    }
    $shortcut = Get-ShortcutPath
    if ($shortcut -and (Test-Path -LiteralPath $shortcut)) { Remove-Item -LiteralPath $shortcut -Force }

    if ($Purge) {
        $dirs = @()
        if ($env:APPDATA) { $dirs += Join-Path $env:APPDATA 'zombies' }
        $userHome = if ($env:USERPROFILE) { $env:USERPROFILE } elseif ($env:HOME) { $env:HOME } else { $null }
        if ($userHome) { $dirs += Join-Path $userHome '.zombies' }
        foreach ($dir in $dirs) {
            if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force; Write-Host "Purged $dir." }
        }
    } else {
        Write-Host 'Run Records and settings were kept (use remove -Purge to delete them).'
    }
}

switch ($Command) {
    'install' { Install-Release $false }
    'update' { Install-Release $true }
    'remove' { Remove-Install }
}
