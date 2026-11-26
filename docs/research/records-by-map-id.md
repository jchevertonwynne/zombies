# Research: Records keyed by Map id and creator (#561)

Question (#561, map #530): how should Run Records, Records and the Gauntlet's file be laid out once a Map is a (Map id, creator) pair rather than a `MapId`, with Custom Maps keeping their own Records and only Shipped Maps' hashes earning Scrip and Commendations?

All references are to `origin/main` at `050ee8c`. Source only: nothing was built or run. Spec: [ADR 0011](../adr/0011-maps-are-data-files.md), "Identity, Forks and Records". Related: [ADR 0002](../adr/0002-gauntlet-layers-on-classic.md) (Classic never mentions Scrip), [ADR 0010](../adr/0010-developer-mode-is-a-layer-with-a-run-taint.md) (the Run taint and the four write sites), and the earlier note `research/run-persistence` (`docs/research/run-persistence.md` on that branch), which lists every write a Run's end makes.

## 1. Short answer

- **Keep one `records.ron`**, bumped to version 2. Each Run Record's `map: MapId` becomes `map: MapKey { id: Uuid, creator: String }`, and the Record gains `map_name: String` and `hash: Option<MapHash>`. `BestRound` takes the same `MapKey`. Boards stay "(Map, Mode, Player count)", with Map now meaning `MapKey`. No per-Map files.
- **`gauntlet.ron` does not change shape.** It holds no Map at all (`GauntletFile`, `src/run/gauntlet/progress.rs:258-285`). What changes is *who may write it*: the three Gauntlet earning sites that already check the taint (`pay_scrip`, `earn_commendations`, the `has_won` set in `log_run`) also check that the Run is on a Shipped Map.
- **Migration** is a `SaveFile::migrate` on `RecordsFile`: v1 text is parsed with a frozen private `LegacyMapId` enum, and each name maps to a fixed Shipped Map UUID with the Shipped creator. `hash` is `None` and `map_name` is the old name. A frozen v1 sample goes under `tests/`.
- **A Fork's Records start empty.** A Fork keeps the Map id but takes a new creator, so its key is new. Editing your *own* Map keeps the key. Its Records carry on, and each Record's `hash` tells versions apart.
- **"Shipped" is decided on each machine** by matching the Run's Map hash against the Shipped Maps this build loaded from `assets/maps/`. This happens in Co-op too, so no new wire field is needed for it.

## 2. What is there today

### 2.1 The files

| File | Type | Version | Map-keyed? | Source |
|---|---|---|---|---|
| `records.ron` | `RecordsFile { version, records: Records { runs: Vec<RunRecord> }, lifetime: LifetimeStats }` | 1 | yes, `RunRecord.map` and `BestRound.map` are `MapId` | `src/storage/records.rs:28-31, 206-210, 267-275, 337-354` |
| `gauntlet.ron` | `GauntletFile` (Scrip, Unlocks, Tracks, Commendations, Kit, Heat, `has_won`, `elites_killed`) | 2 (migrates v1) | no | `src/run/gauntlet/progress.rs:258-317` |
| `settings.ron` | `Settings` (holds the Player's `name`, at most 12 chars, may be empty) | n/a | no | `src/storage/settings.rs:47-50, 282-286` |

`MapId` is a `Copy` unit enum serialized by variant name, so a v1 Record reads `(map: Greybox, mode: Classic, ...)` (`src/maps/map/mod.rs:63-91`, and the RON text in the tests at `records.rs:876, 886, 901`).

The save layer (`src/storage/save.rs`) has one file per `SaveFile` type with a `const FILE_NAME`. `parse` accepts `version == VERSION` or `T::migrate(text)`. Anything else counts as corrupt: the file is renamed to `<name>.bak` and defaults are used (`save.rs:54-89`). There is no per-instance file name, so per-Map files would need a new save-layer API.

### 2.2 Map Files (#545, landed)

`src/storage/map_file/` gives each Map `id: Uuid`, `creator: String` and `lineage: Vec<LineageEntry { id, creator, name }>` (`model.rs:24-40, 117-123`). It also provides `MapHash`, a SHA-256 over `map.ron` plus the sorted asset hashes, serialized as 64 hex chars (`hash.rs:9-105`), and `LoadedMap { map, hash, dir, source: MapSource::{Shipped, Player, Cache} }` (`load.rs:35-91`). A Player's Maps live in `~/.zombies/maps/<map-id>/` and must sit in the folder named for their id (`load.rs:357-395`). Received Maps live in `~/.zombies/cache/<hash>/` (`load.rs:397-404`). There is no `assets/maps/` on `main` yet: the Shipped Maps get ported by #551-#559.

### 2.3 Who writes what at a Run's end

The writes are unchanged since `research/run-persistence`. Each machine writes its own files for its own `LocalPlayer`:

- `record_run` (`records.rs:519-582`) reads `Res<MapId>` (`records.rs:521`), builds the `RunRecord` and calls `RecordsFile::add`. That adds to Lifetime Stats always and to Records if the Run ranks. A Tainted Run writes nothing.
- `pay_scrip` (`src/run/gauntlet/mod.rs:1396-1437`) pays Scrip, raises `file.elites_killed` and stores `gauntlet.ron`. It returns early if Tainted.
- `log_run` sets `file.has_won` on the first Win unless Tainted (`src/run/gauntlet/commendations.rs:312-315`).
- `earn_commendations` pays newly met Commendations unless Tainted (`commendations.rs:325-343`).

ADR 0010 names these four sites as the ones the taint gates. The Shipped-Map gate in this note needs three of them: everything except `record_run`.

## 3. The 19 `MapId` uses in `src/storage/records.rs`

There is one `records.rs` under `src/` (`find src -name records.rs`). It names `MapId` 26 times on 25 lines. One is the import (`:14`) and six are in the code. The **19** in the test module are the ones the ticket counts:

| # | Line | Where | Use |
|---|---|---|---|
| 1 | 647 | `run()` test helper | `map: MapId::Greybox` |
| 2 | 670 | `rounds()` test helper | `.board(MapId::Greybox, mode, 1)` |
| 3 | 713 | `won_runs_rank_first_then_by_heat_then_round` | `board(...)` |
| 4-5 | 820, 821 | `lifetime_keeps_the_highest_round_per_map_and_mode` | `best_round(...)` x2 |
| 6 | 827 | `an_unplayed_map_and_mode_has_no_best_round` | `best_round(...)` |
| 7 | 881 | `a_record_without_extras_still_loads` | `board(...)` (v1 RON text at 876) |
| 8-10 | 891, 894, 895 | `an_old_save_loads_as_one_player_records` | `board` x2, `best_round` (v1 RON at 886) |
| 11 | 906 | `an_old_record_reads_as_the_standard_kit_with_no_elites_...` | `board(...)` (v1 RON at 901) |
| 12-14 | 924, 925, 926 | `records_split_by_player_count` | `board(...)` x3 |
| 15-17 | 935, 936, 937 | `lifetime_best_rounds_split_by_player_count_and_revives_add_up` | `best_round(...)` x3 |
| 18 | 965 | `a_run_saves_one_record_however_many_times_it_ends` | `.insert_resource(MapId::Greybox)` |
| 19 | 990 | `a_tainted_run_writes_no_record_but_still_shows_its_round` | `.insert_resource(MapId::Greybox)` |

The code uses (types and signatures):

| Line | Item |
|---|---|
| 30 | `RunRecord.map: MapId` |
| 215 | `Records::board(&self, map: MapId, ...)` |
| 255 | `Records::keep_best(&mut self, map: MapId, ...)` |
| 270 | `BestRound.map: MapId` |
| 330 | `LifetimeStats::best_round(&self, map: MapId, ...)` |
| 521 | `record_run`'s `map: Res<MapId>` |

These places compare or copy the Map without naming `MapId`, and rely on it being `Copy`. They break when the key becomes a `MapKey` holding a `String`:

- `same_board` (`:202-204`)
- `board`'s filter (`:218`)
- `add`'s `let (map, mode, players) = (record.map, ...)` (`:234`)
- `tidy`'s `seen` tuple (`:245-246`)
- `keep_best`'s filter (`:258`)
- `LifetimeStats::add` (`:316-322`)
- `best_round` (`:333`)

Outside the file, the Records page and Game Over read `MapId`: `src/ui/menu.rs:590-594` (a board per `MapId::ALL`), `:668-670` (Highest Round line) and `:757` (`record.map.name()` on Game Over).

The three v1-text tests (rows 7, 8-10, 11) are worth keeping as migration tests. They are exactly "an old save still loads".

## 4. Options for the on-disk shape

| | A. One `records.ron`, flat list, key on each Record (**chosen**) | B. One file, nested `maps: Vec<MapRecords { key, name, runs }>` | C. One file per Map, `records/<uuid>/<creator-hash>.ron`, Lifetime in `records.ron` |
|---|---|---|---|
| Change to ranking code (`add`/`tidy`/`keep_best`/`board`) | Swap the key type, `Copy` becomes `clone` | Rewrite around a per-Map vec | Rewrite, plus load/store per file |
| Save layer | Unchanged (`SaveFile` + `migrate`) | Unchanged | Needs a dynamic file name. `SaveFile::FILE_NAME` is a `const` |
| A Record alone says its Map (Game Over `LastRun`, menu) | Yes | No, needs the parent | No |
| Size | About 150 B more per Record (uuid, creator, name, hash) | Smallest | Smallest per write |
| Writes per Run end | Whole file | Whole file | One small file plus `records.ron` |
| Creator as a file name | n/a | n/a | Must be encoded: a Name is free text, may be empty, and may hold `/` |

Sizing for A: today a board keeps 10 Records (`RECORDS_KEPT`), and boards are per (Map, Mode 2, Players 1-4), so at most 80 Records per Map. A Record with three teammates is about 400-500 B of pretty RON. A heavy player with 100 Maps that each have a few boards played holds about 2,000-3,000 Records, which is 1-1.5 MB. That is read once at Startup and written once per Run end, well inside what `ron` parses in tens of milliseconds. A Player cannot be flooded either: Records come only from Runs they played. We pick A because it changes the least code, keeps the one-file save layer, and leaves each Record self-describing. If the file ever grows past a few MB, B is a format bump away and needs no new save API.

## 5. Decisions

### 5.1 The key, the name and the hash

```rust
/// Which Map a Record is for: its Map id and creator (ADR 0011). A Fork has its own.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MapKey { pub id: Uuid, pub creator: String }

pub struct RunRecord {
    pub map: MapKey,
    /// The Map's name when this Run was played, so a Map that's gone can still be listed.
    pub map_name: String,
    /// The Map File's hash this Run was played on. `None` for Records from before Map Files.
    #[serde(default)]
    pub hash: Option<MapHash>,
    // ... unchanged fields
}
```

`BestRound.map` becomes `MapKey` as well. `Records::board` and `LifetimeStats::best_round` take `&MapKey`.

- A board is still (Map, Mode, Player count). The hash is *noted*, not part of the key, so a Map's Records survive edits by its creator and changes to a Shipped Map across game updates (ADR 0011: "Records are kept per (Map id, creator), each Run Record noting the hash it was played on").
- `map_name` is the name at the time of play. The page shows the name from the newest Record on a key, or from the loaded Map File when it is present.

### 5.2 Shipped Maps' fixed identity

Migration and the Shipped-Map ports both need each Shipped Map's UUID and creator fixed *now*, written once and never changed. The proposal is a `const SHIPPED_MAPS: [(&str, Uuid); 9]` table in `src/storage/records.rs` (or `map_file`), keyed by the v1 variant name. Each port (#551-#559) writes the same id into its `map.ron`, and a test checks that each ported Shipped Map File's `(id, creator)` matches the table.

| Map | Map id |
|---|---|
| Greybox | `c34a0e6d-d934-452a-a5b5-38cf3ab5f8e9` |
| Quarry | `a2ac0731-4d2b-4793-b511-ec95f1b5d694` |
| Village | `7b05c906-d5cd-4570-a4b0-b393e82c5697` |
| Dam | `f76ddb27-3328-4ed5-b015-edb67062ca43` |
| Cove | `88d0c21b-5e12-4af8-aa7d-72f45b6ee4e7` |
| Polder | `b49e6df2-649f-4510-a6fd-71d78343f37b` |
| Sanatorium | `f85e4c73-6bb0-47b1-a9b8-fe84bfcdc02a` |
| Archive | `92594752-41d8-47f1-be6f-b5ccb2b82cb3` |
| Refinery | `23b9534c-fd62-48ae-85d7-5eef72bbddca` |

The UUIDs were generated once for this note (v4). The **Shipped creator** is `"The Zombies team"`. At 16 characters it is longer than `PLAYER_NAME_MAX_CHARS` (12, `src/tuning.rs:2848`), so no Player's Name can equal it, and a Player's Fork of a Shipped Map can never share a Shipped Map's key. On screen a Shipped Map shows its name alone, with no "by ...".

### 5.3 Migration of `~/.zombies/records.ron`

- `RecordsFile::VERSION` becomes 2. `RecordsFile::migrate(text)` parses a private frozen `V1RecordsFile` whose Records and `BestRound`s use a private `LegacyMapId` enum. That enum is a copy of today's nine variant names, so the migration keeps working after `MapId` is deleted. Each Record maps as follows:
  - `map` becomes `MapKey { id: SHIPPED_MAPS[name], creator: SHIPPED_CREATOR }`
  - `map_name` becomes the old name
  - `hash` becomes `None`
  - every other field carries over as is, including the existing `#[serde(default)]`s for old fields
- `tidy()` runs after loading as now (`load_records`, `records.rs:497-504`).
- A frozen v1 sample (`tests/saves/records_v1.ron`, holding a Classic and a Gauntlet Record, a Co-op Record with teammates, and a `best_rounds` entry) must keep loading. Use the frozen-sample pattern #545 set for `tests/map_files/`.
- Every machine migrates only its own file. No Host is involved.
- Known limit (unchanged from every earlier bump): an *older* build that meets a v2 file treats it as corrupt. It keeps it as `records.ron.bak` and starts empty. The data survives in the `.bak` until a second corrupt load overwrites it. This is the save layer's existing rule (`save.rs:54-89`), so we accept it and don't add a special case.

### 5.4 Run Records note the hash, and the current Run's Map

There is a new Classic-owned resource, set when a Run starts (`OnEnter(Playing)`, alongside `RunPlayers`) and read by `record_run` in place of `Res<MapId>`:

```rust
/// The Map the current (or just-ended) Run is on. `shipped` is true when its hash is one of the
/// Shipped Maps this build loaded; a layer may give Shipped-only rewards on it.
#[derive(Resource)]
pub struct RunMap { pub key: MapKey, pub name: String, pub hash: Option<MapHash>, pub shipped: bool }
```

- Until the ports land, it is filled from `MapId` through `SHIPPED_MAPS` with `hash: None, shipped: true`, so nothing changes for Players today. Once a Map is a loaded `LoadedMap`, it is filled from `map.id`, `map.creator`, `map.name` and `hash`, with `shipped = (source == Shipped)`.
- The name stays neutral: Classic says only "this is a Shipped Map", never "this earns Scrip", which keeps to ADR 0002.

### 5.5 Scrip and Commendations: Shipped Maps only

`gauntlet.ron` keeps its shape and version. The gate goes next to the taint at three sites:

| Site | Today | Add |
|---|---|---|
| `pay_scrip` (`run/gauntlet/mod.rs:1407`) | `if tainted.0 { return; }` | Also return early when `!run_map.shipped`, so no Scrip and no `elites_killed` raise. Push the Game Over line "Custom Map: no Scrip" instead of "Scrip earned 0" |
| `earn_commendations` (`commendations.rs:333`) | `unlock_all.0 \|\| tainted.0` | `\|\| !run_map.shipped` |
| `log_run`'s `has_won` (`commendations.rs:313`) | `!tainted.0` | `&& run_map.shipped`. A Win on a Custom Map doesn't unlock Heat: it is meta-progress, like Scrip |

These three stay in the Run (still recorded, not Tainted):

- Run Records and Records: the Run goes onto its Custom Map's own boards.
- Lifetime Stats: GLOSSARY defines them as "running totals across every Run the Player has finished". They are totals, not rewards, so a Custom Map Run counts. Highest Round is per `MapKey`, so it doesn't mix.
- Heat, Kits and Upgrade Tracks *apply* on a Custom Map, because they are inputs to the Run, not earnings.

"Shipped" is a hash test, not an id test. A Fork of the Greybox keeps the Greybox's UUID, and a hand-edited `assets/maps/greybox/map.ron` changes the hash only if the loader re-hashes. The loader does: `LoadedMap.hash` is computed from disk at load (`load.rs:301-331`). A build-time table of Shipped hashes was considered and rejected. `gauntlet.ron` is plain RON that a Player can already edit, so an embedded table would guard nothing new, and it would add a step to every Shipped Map edit.

### 5.6 What a Fork's Records start with

Nothing. A Fork's key is `(parent id, new creator)`, which has no Records. The parent's Records stay with the parent. The Lineage credits earlier creators in the picker and on the page header, but no Records are copied: Runs on the parent were played on a different Map. Further cases:

- **Editing your own Map** is not a Fork. The key is unchanged and Records continue, each Record noting the hash it was played on.
- **Forking a Shipped Map** (which #567 makes the only way to start from one) gives `(shipped uuid, your Name)`. That key starts empty, and the hash never matches a Shipped hash, so the Fork earns no Scrip.
- **Two Players with the same Name** forking the same Map produce the same key. A third machine that plays both Maps merges their boards. The hash on each Record still tells them apart. This follows from ADR 0011's choice of key and is accepted, not fixed here.

### 5.7 The Records page for a Custom Map

- The tabs stay the same: Mode, then Player count (`menu.rs:567-588`).
- **Shipped Maps** come first, in their fixed order, exactly as now. Each always shows, with "No Records yet." when it has none (`menu.rs:590-626`).
- **Custom Maps** follow, but only those with at least one Record on the current tab, newest first, so the page doesn't fill with empty boards. The heading is "**Fort** by Ana". When the Map File is present (`~/.zombies/maps/` or the cache), the Lineage line from #567 appears under it ("from **Keep** by Bo"). Otherwise only the stored name and creator show. If the creator is empty (a Player who never set a Name), the heading is just the name.
- A Record whose `hash` is known and differs from the Map's current hash gets a grey "earlier version" tag. Records migrated from v1 (`hash: None`) get no tag.
- The Lifetime "Highest Round (nP)" line stays Shipped-only, as now (`menu.rs:668-680`). A Custom Map's best is its board's top Record.
- The Map pickers in #567 show the same board, through `RecordsFile::board(&key, mode, players)`.
- Game Over's Map name (`menu.rs:757`) reads `record.map_name`.
- Deleting a Custom Map (#567) leaves its Records. They are the Player's history, and the page can list them from `map_name` alone. Forgetting Records is out of scope.

### 5.8 Co-op on a Custom Map

Each machine writes only its own saves, as today. There is no Host-side write for a client (`research/run-persistence` §1).

- **Host:** `RunMap` comes from its loaded Map, which is in its own `maps/` or its cache. The Host writes its Run Record under the Map's key on its own `records.ron`. Gauntlet earns nothing unless the hash is a Shipped one.
- **Client:** runs the same Map File, received by #569 or already cached, checked by hash. It builds `RunMap` from that same `map.ron`, so the key, name and hash agree with the Host's. `shipped` is decided **on the client** from its own Shipped table:
  - On a Shipped Map both builds have, the hashes match, both machines earn, and nothing is downloaded.
  - On a Custom Map neither earns, and both Game Overs say "Custom Map: no Scrip".
  - If the Host's Shipped Map hash differs from the client's (different builds), the client is sent the Map as a received Map, so it earns nothing while the Host does. That is the rule applied consistently, not a gap.
- **Rejoiner:** already has the Map (#569). It rebuilds `RunMap` the same way, and `RunRecorded` still guards the one write.
- **Taint** is unchanged and composes: a Tainted Run on any Map writes nothing.
- **Wire:** no new field for Records. The Room and `Hello` already gain the Map's hash in #569, and `RoomView.map` (`src/net/protocol.rs:115`, today a `MapId`) becomes the Map's key, name and hash there. The client's Room screen should show "**Fort** by Ana" like the picker, which is #567/#569's concern.

So Records keep feature parity: every Player in a Co-op Room on a Custom Map gets a Run Record on that Map's own board on their own machine, and nobody gets Scrip or Commendations.

## 6. Facts for the map (#530)

- **The ports (#551-#559)** must write the Map ids in §5.2 and the creator `"The Zombies team"` into each Shipped `map.ron`. Without that, v1 Records would not line up with the ported Maps. A test should check this.
- **#550 Map validation** could refuse a Player or received Map whose creator is the Shipped creator. That stops a hand-edited `map.ron` from writing onto a Shipped Map's board. It's cheap, but optional, since `records.ron` is hand-editable anyway.
- **#563 / #567:** "is this Map yours" is `creator == Settings.name`. A Player who renames themselves will Fork their own older Maps on edit, and a Player with an empty Name would create Maps with creator `""`. Consider requiring a Name before the Map Editor opens. Neither is decided here.
- **#567** depends on the build ticket below for "its own Records" in the picker (it is already blocked by #561).
- **#569** changes `RoomView.map` from `MapId`, and the client's `RunMap` is built from the received `map.ron`.
- **ADR 0010's** "four places a Run's end writes" gain a second gate at three of them. The ticket below should add a line to ADR 0011's Records paragraph (or a short follow-on ADR) saying a Custom Map Win doesn't unlock Heat, and that Lifetime Stats count every Map. Neither is stated today.
- **GLOSSARY** should add **Map id** and **creator** (used in ADR 0011 but not defined there) and note under **Run Record** that it records the Map's hash.

## 7. Draft build ticket

**Title:** Records keyed by Map id and creator; Scrip and Commendations on Shipped Maps only

**Body:**

> Part of #530. Spec: [ADR 0011](https://github.com/jchevertonwynne/zombies/blob/main/docs/adr/0011-maps-are-data-files.md), Identity, Forks and Records. Research: #561 (`docs/research/records-by-map-id.md` on `research/records-by-map-id`).
>
> ## What to build
>
> Records keyed by (Map id, creator) instead of `MapId`, in one `records.ron` bumped to version 2, with old saves migrated, and Gauntlet's earnings gated to Shipped Maps.
>
> - `MapKey { id: Uuid, creator: String }`. `RunRecord.map` and `BestRound.map` become `MapKey`. `RunRecord` gains `map_name: String` and `hash: Option<MapHash>` (the hash it was played on; `None` before Map Files). `Records::board` and `LifetimeStats::best_round` take `&MapKey`. Boards stay (Map, Mode, Player count), and the hash is noted on the Record, not part of the key.
> - A fixed `SHIPPED_MAPS` table: each Shipped Map's UUID (as listed in the research note) and the Shipped creator `"The Zombies team"`, which is longer than any Player Name can be.
> - `RecordsFile` v2 with `migrate` from v1. A private frozen `LegacyMapId` reads the old names, each mapping to its Shipped key, with `map_name` set to the old name and `hash: None`. Add a frozen v1 sample under `tests/`.
> - A Classic-owned `RunMap { key, name, hash, shipped }` resource set on entering Playing, read by `record_run` in place of `Res<MapId>`. Until Maps load from files it comes from `MapId` through `SHIPPED_MAPS` with `shipped: true`; from then on, from the loaded Map, with `shipped` meaning its hash is a Shipped Map's. Each machine decides `shipped` for itself, in Co-op too.
> - `pay_scrip`, `earn_commendations` and the first-Win `has_won` set skip a Run whose `RunMap` isn't shipped, as they skip a Tainted Run. A Gauntlet Game Over on a Custom Map says "Custom Map: no Scrip". Run Records, Records and Lifetime Stats still count it.
> - Records page: the Shipped Maps as now, then each Custom Map with Records on the open tab, headed "**Name** by Creator" (with the Lineage line if the Map File is here), and a grey "earlier version" tag on a Record whose known hash isn't the Map's current one. Game Over reads `record.map_name`.
> - Add **Map id** and **creator** to GLOSSARY, note the hash under **Run Record**, and add to ADR 0011 that a Custom Map Win doesn't unlock Heat and that Lifetime Stats count every Map.
>
> ## Acceptance criteria
>
> - [ ] The 19 `MapId` uses in `records.rs`'s tests move to `MapKey`, and the three v1-text tests become migration tests that still pass
> - [ ] Tests: `a_v1_save_migrates_to_shipped_keys`, `a_fork_starts_with_no_records`, `editing_your_own_map_keeps_its_records_and_notes_each_hash`, `a_custom_map_run_records_but_pays_no_scrip_or_commendation`, `a_custom_map_win_does_not_unlock_heat`
> - [ ] A headless two-machine Co-op test on a non-shipped `RunMap`: each machine writes one Record under the same key and neither pays Scrip
> - [ ] A real `~/.zombies/records.ron` copied into a scratch `HOME` loads with every Record on its Shipped Map's board (never the real `~/.zombies`)
> - [ ] `cargo test` and `cargo clippy` pass
>
> Blocked by: #545 (done). Blocks: #567.
