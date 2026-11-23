# Pester 5 tests for the decisions in zombies.ps1. The end-to-end run against a real release is
# tests/zombies.e2e.ps1. Run with: Invoke-Pester tests/zombies.Tests.ps1

BeforeAll {
    . (Join-Path $PSScriptRoot '..\zombies.ps1')

    function Release([string]$Build, [string]$AssetsHash) {
        [pscustomobject]@{ build_id = $Build; assets = [pscustomobject]@{ hash = $AssetsHash } }
    }
}

Describe 'Get-UpdatePlan' {
    It 'downloads everything when nothing is installed' {
        $plan = Get-UpdatePlan -Installed $null -Latest (Release '0.2.0' 'aaa')
        $plan.Game | Should -BeTrue
        $plan.Assets | Should -BeTrue
    }

    It 'downloads nothing when the install is the latest release' {
        $plan = Get-UpdatePlan -Installed (Release '0.2.0' 'aaa') -Latest (Release '0.2.0' 'aaa')
        $plan.Game | Should -BeFalse
        $plan.Assets | Should -BeFalse
    }

    It 'skips the assets when only the build changed' {
        $plan = Get-UpdatePlan -Installed (Release '0.1.0' 'aaa') -Latest (Release '0.2.0' 'aaa')
        $plan.Game | Should -BeTrue
        $plan.Assets | Should -BeFalse
    }

    It 'downloads the assets when the install has lost them, even if the hash matches' {
        $plan = Get-UpdatePlan -Installed (Release '0.2.0' 'aaa') -Latest (Release '0.2.0' 'aaa') -HasAssets $false
        $plan.Game | Should -BeFalse
        $plan.Assets | Should -BeTrue
    }

    It 'downloads both when the assets changed too' {
        $plan = Get-UpdatePlan -Installed (Release '0.1.0' 'aaa') -Latest (Release '0.2.0' 'bbb')
        $plan.Game | Should -BeTrue
        $plan.Assets | Should -BeTrue
    }
}

Describe 'Assert-ReleaseShape' {
    BeforeAll {
        function Shaped([string]$Build, [string]$Game, [string]$Assets) {
            [pscustomobject]@{
                build_id = $Build
                game     = [pscustomobject]@{ file = $Game; sha256 = 'a' * 64 }
                assets   = [pscustomobject]@{ file = $Assets; sha256 = 'b' * 64; hash = 'c' * 64 }
            }
        }
    }

    It 'accepts a release as the workflow writes it' {
        { Assert-ReleaseShape (Shaped '0.1.0' 'zombies-0.1.0-windows.zip' 'assets-35850ed06f4c900d.zip') } | Should -Not -Throw
    }

    It 'refuses a build id that is not a plain version' {
        { Assert-ReleaseShape (Shaped '0.1.0/../x' 'zombies-0.1.0-windows.zip' 'assets-1.zip') } | Should -Throw '*build id*'
    }

    It 'refuses a file name that leaves the download folder' {
        { Assert-ReleaseShape (Shaped '0.1.0' '..\zombies.exe.zip' 'assets-1.zip') } | Should -Throw '*file name*'
        { Assert-ReleaseShape (Shaped '0.1.0' 'zombies-0.1.0-windows.zip' 'assets/../../x.zip') } | Should -Throw '*file name*'
    }
}

Describe 'Assert-Sha256' {
    BeforeAll {
        $file = Join-Path $TestDrive 'hello.txt'
        [IO.File]::WriteAllText($file, 'hello')
        # SHA-256 of the five bytes "hello", from a known-good source (sha256sum).
        $helloSha = '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824'
    }

    It 'accepts the right hash, in either case' {
        { Assert-Sha256 -Path $file -Expected $helloSha } | Should -Not -Throw
        { Assert-Sha256 -Path $file -Expected $helloSha.ToUpperInvariant() } | Should -Not -Throw
    }

    It 'refuses a wrong hash, naming the file' {
        { Assert-Sha256 -Path $file -Expected ('0' * 64) } | Should -Throw '*hello.txt is corrupt*'
    }
}

Describe 'Test-ZombiesInstall' {
    It 'is true for a folder with release.json' {
        $dir = New-Item -ItemType Directory (Join-Path $TestDrive 'install')
        New-Item (Join-Path $dir 'release.json') | Out-Null
        Test-ZombiesInstall $dir | Should -BeTrue
    }

    It 'is false for a folder without release.json' {
        $dir = New-Item -ItemType Directory (Join-Path $TestDrive 'exe-only')
        New-Item (Join-Path $dir 'zombies.exe') | Out-Null
        Test-ZombiesInstall $dir | Should -BeFalse
    }

    It 'is false for a folder that does not exist' {
        Test-ZombiesInstall (Join-Path $TestDrive 'nowhere') | Should -BeFalse
    }
}

Describe 'Test-InstallTarget' {
    It 'accepts a folder that does not exist' {
        Test-InstallTarget (Join-Path $TestDrive 'fresh') | Should -BeTrue
    }

    It 'accepts an empty folder' {
        Test-InstallTarget (New-Item -ItemType Directory (Join-Path $TestDrive 'empty')) | Should -BeTrue
    }

    It 'accepts what an interrupted install left behind' {
        $dir = New-Item -ItemType Directory (Join-Path $TestDrive 'interrupted')
        New-Item -ItemType Directory (Join-Path $dir 'assets') | Out-Null
        New-Item -ItemType Directory (Join-Path $dir '.staging') | Out-Null
        Test-InstallTarget $dir | Should -BeTrue
    }

    It "refuses a folder holding someone's own files" {
        $dir = New-Item -ItemType Directory (Join-Path $TestDrive 'desktop')
        New-Item (Join-Path $dir 'holiday.jpg') | Out-Null
        New-Item (Join-Path $dir 'zombies.exe') | Out-Null
        Test-InstallTarget $dir | Should -BeFalse
    }
}
