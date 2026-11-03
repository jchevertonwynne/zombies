# Single-Player assumptions in `src/`

For [Task: list every single-Player assumption](https://github.com/jchevertonwynne/zombies/issues/116), on the map [Classic co-op: 2–4 Players online](https://github.com/jchevertonwynne/zombies/issues/112). Taken from `main` at `fdfb5d4`.

**Size key:** **S** is mechanical: swap a `Single` for a `Query` or an entity lookup and change nothing else. **M** needs a design choice, such as which Player, or what is local and what is remote. **L** is a structural rework.

## Headline numbers

- **112 `Single<…>` system params** across 27 files, plus **13 `.single()` / `.single_mut()` calls** (7 of those are menu or UI and safe). The ones that matter are about **60 params that name `Player` or `PlayerCamera`**, or a Player sibling such as `Loadout`, `Modifiers`, `Equipment`, `Health`, `Perks` or `LastStand`.
- By file: `hud.rs` 21, `weapon.rs` 13, `equipment.rs` 13, `player.rs` 9, `special.rs` 8, then 1 to 4 each in 22 more.
- **Per-Player state sits in global `Resource`s** in about 14 places (see below). That's a bigger job than the `Single`s, because each one has to move onto the Player entity, or be keyed by Player.
- **Nothing tells a local Player from a remote one.** Input, the camera, the view model, audio and the HUD all assume that the one Player is the person at the keyboard.

## 1. The Player entity and its bundle — M

`src/player.rs:174` `spawn_player` spawns one `Player` at `PLAYER_START`, with `Health`, `Locomotion`, `Loadout`, `Modifiers` (copied from `RunModifiers`), `Knife` and `Equipment`, and a child `PlayerCamera` carrying `Camera3d`, `Eye`, `Projection`, `ColorGrading` and `DistanceFog`.
- Co-op needs N Players, a spawn point for each (WaW/BO1 use spawn structs per player index), a Player id or seat, and a **local** marker. The camera, the `Eye` and the view model should be children of the local Player only.
- The sibling components already belong to the entity (`Health`, `Loadout`, `Modifiers`, `Perks`, `Equipment`, `Knife`, `LastStand`, `Drinking`), and that part is ready for co-op.

## 2. Input — M

`src/controls.rs:329` `Controls` is a `SystemParam` that reads `ButtonInput<KeyCode/MouseButton>` and `Settings` bindings directly. Its users are `player.rs` (5), `weapon.rs` (8), `equipment.rs` (4), `buy.rs` (2), `barrier.rs` (1) and `attachment.rs` (1). Mouse look and sway are in `player.rs` and `sway.rs`.
- Every gameplay system reads the keyboard itself. For the host to simulate remote Players, input needs to become **per-Player input state**, a component or an input-buffer resource keyed by Player, filled from local devices or the network. This is the seam the network ADR depends on most. The headless test drivers (`src/player/headless.rs`, `src/zombie/headless.rs`) already inject input and teleports, so they show the right direction.

## 3. Camera, view model and first-person effects — M (local only)

- `PlayerCamera` `Single`s: `weapon.rs:976,1172` use it to aim the hitscan and Knife; `player.rs:253` for mouse look; `special.rs:838` for the scream shake; `hellhound.rs:381` for fog; and FOV in `player.rs:237` with `FovWarp` at `player.rs:86`.
- `ViewModel`, `Glint`, `Eye`, `hands.rs`, `gun_model.rs`, `muzzle.rs:272`, `sway.rs`, `last_stand.rs:165` (`lower_eye`) and `shellshock.rs` (screen effect).
- **Hit aiming** (`weapon.rs:1093`, a ray from the camera) has to come from each Player's own aim on the host, or from the shooter's report, since the shooter judges hits. Everything else on this list is cosmetic and should just filter to `LocalPlayer`. Remote Players need a third-person body and gun model, and nothing like that exists today.

## 4. Audio — S/M

- `audio.rs:184` puts the `SpatialListener` on the camera. `audio.rs:571` `camera.single()` for the ears. `audio.rs:434` `loadout.single()`. `audio.rs:729` `player.single()` for the low-health and hot cues.
- Filtering these to the local Player is **S**. Remote Players' gunfire and footsteps as spatial sounds don't exist yet (**M**).

## 5. HUD — M

`hud.rs` has 21 `Single`s: Points, the ammo counter, `EquipmentText`, `PerkRow`, `PopupColumn`, `Banner`, `PointsFlash` and `LastBalance`, plus `hud/overlay.rs`. All of them read "the" Player.
- Pointing them at the local Player is **S**. Teammates' names and Points, revive markers and Downed icons are new work, and they sit in the map's co-op HUD fog.

## 6. Per-Player state held in global Resources — M/L (the main work)

| Resource | Where | Today | Co-op |
|---|---|---|---|
| `Points` (balance, earned, Double Points flag) | `points.rs:41` | one balance | **per Player** (L: every award site credits the resource; `Buyer` at `buy.rs:141` copies `points` from it) |
| `RunStats` (kills, headshots, downs, survived) | `stats.rs:10` | one set | per Player, plus team totals for Records |
| `Shellshocked` | `shellshock.rs:29` | one timer | per Player |
| `ammo_mod::Cooldowns` | `ammo_mod.rs:185` | the doc comment says "Solo play has one Player, so a cooldown is shared" | per Player |
| `buy::Focus`, `HoldTimer<T>` | `buy.rs:137`, `buy.rs` | the one Prompt showing | per Player (each looks at their own Buyable, and holds Use alone) |
| `PlayerZone` | `zone.rs:159`, written by `track_player` at `zone.rs:196` | one Zone | per Player. `ActiveZones` (`zone.rs:163`) becomes the union over Players (M, a rules question) |
| `Teleport` | `player.rs:99` | moves the Player | per Player, or carry an entity |
| `FovWarp` | `player.rs:86` | camera factor | local only |
| `wall_weapon::AmmoBuy` | `wall_weapon.rs:96` | the held Weapon's pose | per Player |
| `last_stand::SparesUsed` | `last_stand.rs:51` | spare Self-Revives (Gauntlet) | per Player |
| `ActivePowerUps.extra_secs` | `power_up.rs:89` | "resolved from the Player's stat entries" | shared timers. Whose Modifiers count? (a rules question) |
| `state::Freeze` / `Pause::Frozen` | `state.rs:39,102` | pause and Gauntlet offers stop the world | co-op can't pause the world. Pause becomes a local menu (M). Gauntlet freezing is out of scope |
| `RunModifiers` | `modifier.rs` | copied into the one Player's `Modifiers` | fine for Classic, where it's empty. Gauntlet-only |
| `records::LastRun`, `menu::GameOverLines` | `records.rs:248`, `menu.rs` | one Run Record | depends on co-op Records (fog) |

Team-wide and already correct: `Power`, `Round`, `BoxRun`, `Barriers`, `Teleporter`, `NavGraph`, `SpecialSlot` and `SapperCadence`, and the special-Zombie plans and `DropMeter`. Whether the Drop Meter scales with Player count is a question for the rules research.

## 7. Zombie and special targeting — M

All of these take the one Player's `Transform` as the target:
- `zombie.rs:843` (the chase, with `Has<LastStand>` and `Modifiers`), `zombie.rs:981`, `zombie.rs:1030` (the attack, skipping a Player in `LastStand`).
- `special.rs:411, 526, 566, 632, 751, 822, 874` (Napalm, Shrieker, fire pools, and `SpecialWatch` spawning near the Player), `sapper.rs:660, 869`, `hellhound.rs`, `barrier.rs:527` (swiping through the window), `explosive.rs:287` (self-damage), `trap.rs:449` (a Trap Downing the Player) and `teleporter.rs:606`.
- Co-op needs **target choice** (the nearest Player who isn't Down, as in WaW/BO1's `get_closest_valid_player`) and damage to whichever Player was hit. That's one shared helper, plus about 15 call sites.

## 8. Down, Last Stand and Game Over — L

- `last_stand.rs:118` handles a Down on the one Player. `down_outcome` (`last_stand.rs:40`) returns `GameOver` without Quick Revive, and `last_stand.rs:139` sets `GameState::GameOver` straight away. `last_stand.rs:150` handles Self-Revive.
- `round.rs:348` `alive.single_mut()`, `round.rs:230` (the Player's `Transform` and `Modifiers` for spawning), `round.rs:382`.
- `points.rs:116` blocks earning while in `LastStand`. `perk.rs:209` `hands_free`, `perk.rs:320, 364, 384`.
- Co-op needs a Down that doesn't end the Run, bleed-out, revive by another Player, and Game Over only when every Player is Down. This waits on [Grilling: co-op Downs, revives and Game Over](https://github.com/jchevertonwynne/zombies/issues/117).

## 9. Buyables and pickups — S each, M as a whole

`buy.rs` (4 `Single`s plus `Focus` and `Buyer`), `mystery_box.rs:415, 461, 579`, `wall_weapon.rs` (3), `pack_a_punch.rs` (2), `perk.rs` (3), `power_up.rs:364, 405, 479` (the pickup gives the one Player ammo and Equipment), `attachment.rs` (2), `ammo_mod.rs` (2), `wonder.rs` (2), `nova.rs` (2), `trap.rs:302, 322` (Modifiers for the Trap timing), `teleporter.rs:606`.
- Each becomes "the Player who pressed Use" or "the Player who touched it". The plumbing is S each, once input is per Player (section 2). Who pays and who gets the credit is decided in [Grilling: what's shared and what each Player owns](https://github.com/jchevertonwynne/zombies/issues/118).

## 10. Weapons, Equipment and the Knife — S/M

`weapon.rs` (13: fire, Knife, reload, slots, view model), `equipment.rs` (13), `explosive.rs` and `attachment.rs`. The state is on the entity already. The `Single`s become per-Player iteration driven by per-Player input. The view-model parts are local only (section 3).

## 11. Gauntlet — out of scope

`gauntlet.rs:241, 344`, `gauntlet/armory.rs:121`, `HeldBoons`, `HeldPacts`, `Offer` and `PendingOffer` are all global, one-Player state. Co-op Gauntlet is out of scope. Co-op Runs only need to refuse Gauntlet.

## 12. Settings, Controls, Records and Lifetime Stats — S/M

- `Settings` and the bindings are per machine, and stay local. That part is fine.
- Records (`records.rs:313` `record_run` on entering `GameOver`) and Lifetime Stats (`stats.rs`) write one Run Record from the global `RunStats`. Co-op Records are fog on the map. Each machine probably saves its own copy.

## 13. Safe (UI and singletons that stay single)

`menu.rs:407, 838, 953, 960, 978`, `debug.rs` (4: `ZoneDebugText`), `CursorOptions` (2), `hud` UI-node `Single`s for local widgets, and the `muzzle.rs:272` flash.

## What this means for the network layer ADR

1. **Per-Player input is the keystone seam.** Until gameplay systems read input from the Player entity and not from `ButtonInput`, the host can't simulate a remote Player. It's also the seam that makes solo a host with one local Player, with no special path.
2. **A `LocalPlayer` marker** splits about 30 `Single`s (camera, view model, HUD, audio, screen effects) from the gameplay ones. Those stay `Single<…, With<LocalPlayer>>`, and that's the cheap half.
3. **The global per-Player Resources** (`Points`, `RunStats`, `Focus` and `HoldTimer`, `PlayerZone`, `Shellshocked`, `Cooldowns`, `Teleport`, `AmmoBuy`) should become components on the Player. That's a refactor we can do now, behaviour-neutral, and it can land before any netcode.
4. **Target choice for Zombies** is one helper with about 15 call sites.
5. **Pause and Freeze** can't stop the world in co-op.
6. **Down → Game Over** is the one piece that changes behaviour. It waits on the Downs grilling.

Rough total: most of the ~125 sites are S once 1–3 exist. The M/L work is input (2), the Resources (6), targeting (7) and Down/Game Over (8). A behaviour-neutral **"N-Player-ready solo"** refactor covering 1–4 could be built and tested in single-player before any networking.
