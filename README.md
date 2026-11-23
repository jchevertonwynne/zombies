# Zombies

A single-player and Co-op first-person zombies game, inspired by the round-based zombies modes of the late 2000s. Built with [Bevy](https://bevyengine.org) and [avian3d](https://github.com/Jondolf/avian).

## Install on Windows

Open PowerShell (no admin needed) and run:

```powershell
& ([scriptblock]::Create((irm https://github.com/jchevertonwynne/zombies/releases/latest/download/zombies.ps1))) install
```

This installs the latest release into `%LOCALAPPDATA%\Zombies` and adds Zombies to the Start Menu. Every download is checked against the SHA-256 published with the release. Windows may warn that the game is from an unknown publisher, because it isn't code-signed.

To update or uninstall, run the same line with `update` or `remove` in place of `install`. Update keeps your saves, skips the ~300 MB of assets when they haven't changed, and won't run while the game is open. Remove keeps your saves, settings and Run Records (in `%APPDATA%\zombies`) unless you add `-Purge`:

```powershell
& ([scriptblock]::Create((irm https://github.com/jchevertonwynne/zombies/releases/latest/download/zombies.ps1))) remove -Purge
```

Co-op players need the same release, so update before playing together.

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
