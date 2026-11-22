# Zombies

A single-player and Co-op first-person zombies game, inspired by the round-based zombies modes of the late 2000s. Built with [Bevy](https://bevyengine.org) and [avian3d](https://github.com/Jondolf/avian).

## Install (Windows)

In PowerShell, no administrator rights needed:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/jchevertonwynne/zombies/main/tools/zombies.ps1))) install
```

That downloads the latest release, checks its SHA-256, installs it under `%LOCALAPPDATA%\Zombies` and adds a Start Menu shortcut. Run the same line with `update` to get a newer release (it refuses while the game is running, and keeps the previous `zombies.exe` as `zombies.exe.old`), or with `remove` to uninstall. `remove -Purge` also deletes your Run Records and settings; without `-Purge` they are left alone. The script is [`tools/zombies.ps1`](tools/zombies.ps1).

## Build and run

You need a stable Rust toolchain.

```sh
cargo run --release
```

Run Records and settings are saved under `~/.zombies` (`%APPDATA%\zombies` on Windows).

## Licence

The code is dual-licensed under either of

- MIT licence ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

The assets under `assets/` are CC0 (public domain). [`assets/manifest.json`](assets/manifest.json) lists the source and licence of every one.

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in this work, as defined in the Apache-2.0 licence, is dual-licensed as above, without any additional terms or conditions.
