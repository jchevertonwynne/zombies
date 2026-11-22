# Research: everything a finished Run persists (#446)

Question (#446, map #444, idea #391): list every write a finished or abandoned Run makes, solo and Co-op, including where each Player's write happens when a client didn't launch with the Host's settings; and what single gate would make a developer-mode Run write nothing.

All references are to `origin/main` at `64e0e99`. Source only; nothing was run.

## 1. Short answer

- The save directory holds exactly three files, each written through `save::store` (`src/storage/save.rs`): `settings.ron`, `records.ron`, `gauntlet.ron`. Nothing else in `src/` writes there (the other `fs::write`s are `bench.rs` and `tour.rs`, to env-chosen paths, and a test-only SVG in `maps/map/plan.rs`).
- A Run's end writes to two of them: `records.ron` (Run Record, Records, Lifetime Stats) from `record_run`, and `gauntlet.ron` (Scrip, all-time Scrip, Elites count) from `pay_scrip`. During the Run, `gauntlet.ron` is also written by Commendation earning (Scrip, the Commendation list, Family Kit) and by the first Win (`has_won`, which unlocks Heat). `settings.ron` is not touched by a Run ending.
- Every machine writes its own files for its own `LocalPlayer`. There is no Host-side write on behalf of a client.
- There is no single existing gate. Four systems mutate the in-memory resources, and two change-driven systems flush them. The cleanest single gate is a Classic-owned "this Run is not a real Run" resource, read at the four mutation sites (section 5). Silencing `save::store` or `SaveDir` alone is not enough.

## 2. The three files

| File | Resource | Type | Version | Source |
|---|---|---|---|---|
| `settings.ron` | `Settings` | `SaveFile` | n/a | `src/storage/settings.rs:219` |
| `records.ron` | `RecordsFile` | `SaveFile` | 1 | `src/storage/records.rs:342` |
| `gauntlet.ron` | `GauntletFile` | `SaveFile` | 2 (migrates v1) | `src/run/gauntlet/progress.rs:304` |

Save dir is `~/.zombies/` (`%APPDATA%\zombies\` on Windows), from `save_dir()` (`src/storage/save.rs:36`), held in the `SaveDir(Option<PathBuf>)` resource (`src/storage/settings.rs:377`). `None` means "run without saving". `save::store` writes a temp file and renames it (`save.rs:98`).

## 3. Writes a Run makes

### 3.1 `records.ron`: Run Record, Records, Lifetime Stats

One system, `record_run` (`src/storage/records.rs:494`):

- Guarded once per Run by `RunRecorded` (`records.rs:512`), reset on `OnEnter(Playing)`. A drop and Rejoin therefore saves only the real end.
- Triggers (`records.rs:454-474`): `OnEnter(GameOver)`; `OnTransition Playing -> MainMenu` (Leave Run, quit to menu); `Last` on `AppExit` while `Playing` (window close); and `Update` on the `SaveRecord` message, which only the Rejoin screen's "Leave" button sends (`src/net/away.rs:164`).
- Effect: builds a `RunRecord` from the `LocalPlayer`'s `RunStats`/`Points`, then `file.add(record)` (`records.rs:545`), which does both `lifetime.add` (always) and `records.add` (top 10 per Map/Mode/Player count). It then calls `store` immediately (`records.rs:547`), and inserts `LastRun` (not persisted) for Game Over.
- `save_records` (`records.rs:591`) also flushes on `resource_changed::<RecordsFile>`, skipping the freshly-added load. Both paths end in `save::store`.
- Content: Map, Mode, date, Round, kills, headshots, downs, Points earned, time, Player count, revives, teammates (from the Host's `Scoreboard`, or the live Players if it hasn't come), `won`, Heat and Conditions, Kit, Elites killed, and `extras` (Boons, Pacts, Ranks, plus one line per Special Zombie type switched off).
- Lifetime Stats additions (`LifetimeStats::add`, `records.rs:~226`): runs, kills, headshots, downs, points, time, revives, elites, wins, highest Heat won, best Round per board.

### 3.2 `gauntlet.ron`: Scrip, Elites, Commendations, Win

Only Gauntlet Runs (`run_if(gauntlet)`). Classic never touches this file.

- `pay_scrip` (`src/run/gauntlet/mod.rs:1316`): `OnExit(Playing)` and `Last` on `AppExit` (`mod.rs:332-339`). Unlike `record_run` it has no once-per-Run flag, but `OnExit(Playing)` fires once per Run. It does `file.pay(scrip)` (adds to `scrip` and `all_time_scrip`), raises `elites_killed`, and `store_progress` immediately (`mod.rs:1329-1331`). Scrip is Rounds completed triangular sum plus Act bonuses, times Heat. It also pushes the Game Over lines.
- `earn_commendations` (`commendations.rs:325`): during the Run, each newly met Commendation calls `file.earn(id, scrip)` (`progress.rs:428`), which pays its Scrip, records the ID, and unlocks a Family Kit. The flush is `save_progress` on `resource_changed::<GauntletFile>` (`mod.rs:345`).
- `log_run` (`commendations.rs:263`): first Win sets `file.has_won = true` (`commendations.rs:313`), which is the sole unlock for Heat. Same change-driven flush.
- Between Runs (not Run-end writes, listed for completeness): Armory purchases, Rank toggles, Kit pick, Heat Degrees, and "Reset Gauntlet Progress" all mutate `GauntletFile` and flush via `save_progress` (`armory.rs:~455`, `mod.rs:1265`).

### 3.3 `settings.ron`

Written on any `Settings` change (`settings.rs:410`). A Run's end does not write it. A Run does touch `Settings` mid-Run (Pause menu toggles, including Special Zombie switches), which saves like any other change, and the last Mode chosen is remembered there. This is player preference, not Run outcome, so it is arguably outside "a dev Run writes nothing"; flagged for the grilling.

### 3.4 Not persisted

`LastRun`, `GameOverLines`, `RunCommendations`, `RunLog`, `RunWon`, `RunKit`, `RunHeatRecord`, `RunElitesKilled`, `RecordExtras`, `RunSpecialZombies`: all in-memory per-Run resources, reset on `OnEnter(Playing)` (`records.rs:~417-440`). There are no mid-Run saves; a crash loses the Record (module doc, `records.rs:1-6`).

## 4. Solo versus Co-op: where each Player's write happens

- Solo: both files are written by the one machine, as above.
- Co-op: every machine runs `record_run`, `pay_scrip`, `log_run` and `earn_commendations` for its own `LocalPlayer`, against its own `SaveDir`. The Host writes nothing for clients. `log_run`/`earn_commendations` are explicitly "not `HostSimulation`" so each machine pays its own Commendations (`mod.rs:480-490`; `docs/design/gauntlet-v2-coop-sync.md` section 3.9).
- Glossary agrees: each Player saves their own Run Record, earns their own Scrip (GLOSSARY.md, Run Record and Scrip entries).
- Ending paths in Co-op: Game Over reaches every machine (`src/run/party.rs:160` `run_over`, then each goes to `GameOver`); a client who leaves uses `Leave` (`SaveRecord`) or the menu transition; a Host leaving closes the Room, and the joiners' Run ends for them. Window close is the `Last`/`AppExit` path on each machine.
- Inputs a client takes from the Host: Mode, Map and Heat come from `Party` (`party.rs:42-51`, applied in `launch`, `party.rs:138-150`), and `set_run_modifiers` uses `party.heat()` instead of the local save when a Party exists (`mod.rs:1288-1297`). Teammate rows come from the Host's `Scoreboard`. Elite kills come from the Host (`tell_elites_killed`, `net/world/mod.rs:1127`; `hear_elites_killed`, `:2125`).
- Inputs a client does not take from the Host: its own `Settings`. `RunSpecialZombies` is initialised from the local `Settings.special_zombies` on `OnEnter(Playing)` (`records.rs:~437`) and tracks the local settings during the Run. Spawning reads `Settings.special_zombies` on the simulating Host (`run/round.rs:329`, `enemies/special.rs`). So a client whose Special Zombie switches differ from the Host's writes a Record whose `extras` describe the client's switches, not the Run it actually played. This is an existing gap of exactly the "client didn't launch with the Host's settings" kind, and a developer-mode flag held in `Settings` would hit the same problem.
- Inputs: `Party` carries no per-Run flag beyond mode, map, heat and seats. There is no wire field for anything like "tainted".

## 5. What a single gate looks like

Constraint from section 3: the mutations are in-memory first (`file.add`, `file.pay`, `file.earn`, `has_won`), flushed by `store` or by change detection. Consequences:

1. Blocking `save::store`, or setting `SaveDir(None)` for the Run, is not sufficient. The in-memory `RecordsFile`/`GauntletFile` would still hold the cheated Run, and the next legitimate Run's `store` (or any Armory purchase) would write it out. A snapshot-and-restore would work but is more machinery than gating the four sites.
2. The gate belongs at the four mutation sites, with the persistence layer untouched:
   - `record_run`: skip `file.add` and `store`, but still build and insert `LastRun` (rank `None`) so Game Over renders, and still set `RunRecorded`.
   - `pay_scrip`: skip `file.pay`, the `elites_killed` raise and `store_progress`; still push the Game Over lines (and a "not saved" line).
   - `earn_commendations`: already has the shape (`if unlock_all.0 || ...`, `commendations.rs:332`).
   - `log_run`: skip `file.has_won = true` (`commendations.rs:313`); otherwise a cheated Win unlocks Heat.
3. The gate: one Classic-owned resource, for instance `RunCounts(bool)` or `RunTainted(bool)`, reset to its clean value on `OnEnter(Playing)` next to the other Run resources (`records.rs:~417`), set one-way during the Run by developer mode. It should live with the storage/Classic side so the Gauntlet reads it the same way it already reads `RunElitesKilled` and `RunKit`, which satisfies ADR 0002/0007 (developer mode writes to it; no developer-mode branch appears in Classic).
4. Precedent: the existing debug `UnlockAll` (F9, `mod.rs:133`) is a half-gate. It blocks only `earn_commendations`, not `pay_scrip`, `record_run` or `has_won`, and it is Gauntlet-only, solo-only, and not synced. Other `src/debug.rs` hotkeys (grant weapon, drop Power-up, monkey bombs) taint nothing today.
5. Sticky rule: the taint should be one-way for the Run (turning developer mode off mid-Run must not clean it), and must be evaluated at write time, since writes happen at Run end and at `AppExit`.

### Co-op requirements for the gate

- "Every Player in it": a Host's cheats change every client's Run (Points, ammo, Rounds), so the Host's taint must reach every client, at start if on, and mid-Run when first used. The `Party` start message is the existing place for Run-start facts (like Heat); there is no mid-Run equivalent for a Run-wide flag, so a new message or snapshot field is needed (feeds #448/#447).
- Rejoin: a Player who drops and rejoins must come back tainted. The Rejoin path restores state from the Host, so the flag must be in whatever the Host sends on rejoin.
- A client must not be able to decide its own taint: it should only be set by the Host's say-so (ADR 0003, host-authoritative), with a client's own local cheats (the "On (every Player)" setting) requested through the Host so the Host can taint everyone.
- A client's own `Settings` must not be the source of the dev flag (see the Special Zombies gap in section 4).
- Every ending path must see the gate: Game Over, Leave (`SaveRecord`), menu transition and window close all funnel through the same two systems (`record_run`, `pay_scrip`), so gating those covers them. A crash writes nothing anyway.

## 6. Findings that bear on later tickets

- Records, Lifetime Stats, Scrip, Commendations and the Heat-unlocking Win are the complete set; the Armory (Unlocks, Ranks, Kit, Heat Degrees) is not written by a Run ending, only spent from earlier earnings, so cheated Runs reach it only via Scrip and Commendations.
- `elites_killed` lives in both `gauntlet.ron` (all-time, for a Commendation) and `records.ron` (per Run and Lifetime); both must be gated.
- `RunRecorded` guards `record_run` once per Run, but `pay_scrip` has no equivalent guard. A gate on `pay_scrip` is safe, but a taint that arrives between `OnExit(Playing)` and an `AppExit` `Last` pass is not a case to worry about, as the two paths cannot both fire for one Run in practice.
- Open question for the grilling: whether a tainted Run's Game Over still shows its rank line ("New best!") or says nothing, since `LastRun` is built from the same `RunRecord`.
