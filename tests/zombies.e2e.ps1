# End-to-end test of zombies.ps1 against the repo's real latest release, on Windows, in scratch
# folders: install (as the README's one-liner does), smoke-check the installed game, update when
# up to date, a game-only update, update refused while the game runs, and remove with and
# without -Purge. Run it from pwsh, naming the shell to test the script under, e.g.
#   pwsh -File tests/zombies.e2e.ps1 -Shell powershell
param([ValidateSet('powershell', 'pwsh')][string]$Shell = 'powershell')

$ErrorActionPreference = 'Stop'
$script = (Resolve-Path (Join-Path $PSScriptRoot '..\zombies.ps1')).Path

# Everything the script touches lives under one scratch folder.
$scratch = Join-Path ([IO.Path]::GetTempPath()) "zombies-e2e-$([guid]::NewGuid().ToString('n'))"
$env:LOCALAPPDATA = Join-Path $scratch 'Local'
$env:APPDATA = Join-Path $scratch 'Roaming'
New-Item -ItemType Directory -Force $env:LOCALAPPDATA, $env:APPDATA | Out-Null
$install = Join-Path $env:LOCALAPPDATA 'Zombies'
$shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Zombies.lnk'
$saves = Join-Path $env:APPDATA 'zombies'

function Step([string]$Name) { Write-Host "`n=== $Name" }
function Check([bool]$Ok, [string]$What) {
    if (-not $Ok) { throw "FAILED: $What" }
    Write-Host "ok: $What"
}
# Runs the script in a fresh shell; returns its output and exit code.
function Run([string[]]$Arguments, [switch]$OneLiner) {
    $out = if ($OneLiner) {
        & $Shell -NoProfile -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((Get-Content -Raw '$script'))) $($Arguments -join ' ')" 2>&1
    } else {
        & $Shell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $install 'zombies.ps1') @Arguments 2>&1
    }
    $text = ($out | Out-String)
    Write-Host $text
    [pscustomobject]@{ Text = $text; Code = $LASTEXITCODE }
}
function Start-Game([hashtable]$Env = @{}) {
    $info = New-Object Diagnostics.ProcessStartInfo (Join-Path $install 'zombies.exe')
    $info.UseShellExecute = $false
    $info.WorkingDirectory = $scratch
    $info.RedirectStandardOutput = $true
    $info.EnvironmentVariables['WGPU_BACKEND'] = 'dx12'
    $info.EnvironmentVariables['WGPU_FORCE_FALLBACK_ADAPTER'] = '1'
    foreach ($key in $Env.Keys) { $info.EnvironmentVariables[$key] = $Env[$key] }
    [Diagnostics.Process]::Start($info)
}

try {
    Step 'install, as the one-liner'
    $r = Run @('install') -OneLiner
    Check ($r.Code -eq 0) 'install succeeds'
    foreach ($file in 'zombies.exe', 'release.json', 'zombies.ps1', 'LICENSE-MIT', 'assets\manifest.json') {
        Check (Test-Path (Join-Path $install $file)) "$file is installed"
    }
    Check (Test-Path $shortcut) 'the Start Menu shortcut exists'
    $installed = Get-Content -Raw (Join-Path $install 'release.json') | ConvertFrom-Json
    $latest = Invoke-RestMethod -UseBasicParsing "https://github.com/jchevertonwynne/zombies/releases/latest/download/release.json"
    Check ($installed.build_id -eq $latest.build_id) "the installed build is the latest ($($latest.build_id))"
    Check ((Get-Content -Raw (Join-Path $install 'zombies.ps1')).Trim() -eq (Get-Content -Raw $script).Trim()) 'the install keeps a copy of the script'

    Step 'the installed game starts and finds its assets'
    $game = Start-Game @{ ZOMBIES_SMOKE = '1' }
    $output = $game.StandardOutput.ReadToEnd()
    if (-not $game.WaitForExit(300000)) { $game.Kill(); throw 'FAILED: the smoke check hung' }
    Write-Host $output
    Check ($game.ExitCode -eq 0 -and $output -match 'smoke: ok') 'the smoke check passes'
    Check ($output -match "smoke: build $([regex]::Escape($latest.build_id)),") 'the game reports the release build id'

    Step 'update when up to date'
    $r = Run @('update')
    Check ($r.Code -eq 0 -and $r.Text -match 'up to date') 'update says it is up to date'
    Check (-not (Test-Path (Join-Path $install 'zombies.exe.old'))) 'no zombies.exe.old yet'

    Step 'update from an older build with the same assets'
    $json = Join-Path $install 'release.json'
    $older = Get-Content -Raw $json | ConvertFrom-Json
    $older.build_id = '0.0.0-older'
    $older | ConvertTo-Json -Depth 4 | Set-Content $json
    $marker = Join-Path $install 'assets\e2e-marker.txt'
    Set-Content $marker 'kept'
    $r = Run @('update')
    Check ($r.Code -eq 0 -and $r.Text -match 'game only') 'update fetches only the game'
    Check (Test-Path (Join-Path $install 'zombies.exe.old')) 'the previous exe is kept as zombies.exe.old'
    Check (Test-Path $marker) 'the assets were left in place'
    Check ((Get-Content -Raw $json | ConvertFrom-Json).build_id -eq $latest.build_id) 'release.json records the new build'

    Step 'update refused while the game runs'
    $older | ConvertTo-Json -Depth 4 | Set-Content $json
    $game = Start-Game
    Start-Sleep -Seconds 5
    try {
        Check (-not $game.HasExited) 'the game is running'
        $r = Run @('update')
        Check ($r.Code -ne 0 -and $r.Text -match 'Zombies is running') 'update refuses'
        Check ((Get-Content -Raw $json | ConvertFrom-Json).build_id -eq '0.0.0-older') 'nothing was changed'
        $r = Run @('remove')
        Check ($r.Code -ne 0 -and (Test-Path $install)) 'remove refuses too'
    } finally {
        if (-not $game.HasExited) { $game.Kill(); $game.WaitForExit() }
    }

    Step 'remove keeps saves'
    New-Item -ItemType Directory -Force $saves | Out-Null
    Set-Content (Join-Path $saves 'settings.ron') '()'
    $r = Run @('remove')
    Check ($r.Code -eq 0) 'remove succeeds'
    Check (-not (Test-Path $install)) 'the install is gone'
    Check (-not (Test-Path $shortcut)) 'the shortcut is gone'
    Check (Test-Path (Join-Path $saves 'settings.ron')) 'saves are kept'

    Step 'install and remove leave a folder that is not an install alone'
    New-Item -ItemType Directory -Force $install | Out-Null
    Set-Content (Join-Path $install 'precious.txt') 'mine'
    $r = Run @('install') -OneLiner
    Check ($r.Code -ne 0 -and @(Get-ChildItem $install).Count -eq 1) 'install refuses and adds nothing'
    $r = Run @('remove') -OneLiner
    Check ($r.Code -ne 0 -and (Test-Path (Join-Path $install 'precious.txt'))) 'remove refuses and deletes nothing'
    Remove-Item -Recurse -Force $install

    Step 'remove -Purge deletes saves'
    $r = Run @('remove', '-Purge') -OneLiner
    Check ($r.Code -eq 0 -and -not (Test-Path $saves)) 'saves are deleted'

    Write-Host "`nAll end-to-end checks passed under $Shell."
} finally {
    Remove-Item -Recurse -Force $scratch -ErrorAction SilentlyContinue
}
