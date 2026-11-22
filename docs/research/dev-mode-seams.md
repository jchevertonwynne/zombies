# Developer mode: which existing seams drive each cheat

Research for [#445](https://github.com/jchevertonwynne/zombies/issues/445), under the map [#444](https://github.com/jchevertonwynne/zombies/issues/444) (original idea [#391](https://github.com/jchevertonwynne/zombies/issues/391)). Read-only on source; every claim below cites the file that owns it, at `origin/main` 64e0e99.

Rules the answers are measured against:

- ADR 0002 (`docs/adr/0002-gauntlet-layers-on-classic.md`): a layer reaches Classic through (1) stat-modifier entries, (2) messages Classic already sends, (3) reading Classic state. A missing hook becomes a new seam, never a layer branch in Classic.
- ADR 0007 (`docs/adr/0007-gauntlet-asks-classic-through-generic-requests.md`): a layer may also (4) send request messages Classic handles and (5) set generic components or resources Classic reads. Each must make sense without the layer (neutral default).
- ADR 0003 (`docs/adr/0003-coop-network-layer.md`): the Host simulates everything; a client predicts only its own movement and owns its own Weapon state; anything touching shared state is a client `Request` the Host judges; rare changes go down as `WorldEvent`s.
- `CLAUDE.md`: Co-op is first-class, with feature parity.

## Short answer

Seven of the nine cheats are reachable today through an existing message, stat entry or component. Only four need a new seam, and one of those (noclip) also needs a wire change.

| Cheat | Existing seam to use | New seam needed | Where it runs in Co-op |
|---|---|---|---|
| Invincibility | `PlayerHit`/`take_hits` and `DownGuard` exist, but neither is enough (see below) | A generic "can't be harmed" marker read by `take_hits` and `catch_out_of_bounds` | Host (Host judges all harm) |
| Infinite Points | `Stat::Price` entry (`Mult(0)`), or `BonusPoints` top-up | None | Host (Points are Host-judged) |
| Infinite ammo | `Stat::ReserveFeed` plus `Stat::ReserveSize`/`MagazineSize` entries; `RefillMagazine` | None for the base cut; Equipment has no refill message | Client (the owner's Loadout) |
| Round jump | None: `Round::begin_in` is private and no message asks for a Round | A `Round` request message | Host, by client request |
| Kill all | `GivePowerUp(PowerUp::Nuke, None)` | None (or a dedicated kill-all request if Paid kills are wanted) | Host, by client request |
| Spawn a Zombie kind | `SpawnZombie` (ADR 0007) | More `RequestedKind` variants for Shrieker, Nova Crawler, Boss, Elite, Sprinter | Host, by client request |
| Grant Power-up | `GivePowerUp` | None | Host, by client request |
| Grant Weapon | None (`debug.rs` mutates `Loadout` directly) | A `GiveWeapon` message, like `GiveLethals` | Host edits its copy; the existing `Hands` event reaches the client |
| Grant Boon / Pact / Kit | `PickOffer` partly (needs an open `Offer`) | A "grant" message in the Gauntlet layer (not Classic) | Host, by client request |
| Open all Doors and Zones | `Purchased` (fake a buy) or the `open()` helper | A request to open Doors (Zones follow from Doors) | Host, by client request |
| Round-hold off | `IntermissionHold` resource exists | A way to stop the Round ending or the Horde spawning (the Intermission hold is the smaller reading) | Host, by client request |
| Noclip | None | A `MoveInput` flag, a `Step` button bit and a branch in `step_move` | Predicted on the client, replayed by the Host |

"New seam" always means a Classic-neutral message, component or stat, never a dev-mode branch.

## What `src/debug.rs` already does

`DebugPlugin` is added unconditionally in `src/main.rs` (no `cfg(debug_assertions)`, no flag), so every build ships it live. It adds avian's `PhysicsDebugPlugin` and these hotkeys:

| Key | Does | How it reaches Classic | Co-op behaviour |
|---|---|---|---|
| F1 | `DebugView`: physics gizmos, Zone volumes, nav graph | Reads state | Local only, fine |
| F2 | `Loadout::give` a debug Weapon | Mutates the `LocalPlayer`'s `Loadout` directly | On a client this edits the client's own Loadout, which the Host never hears about as a Host op; the Host's copy of that Player's Weapons then differs |
| F3 | `Equipment::take_monkey_bombs` | Direct mutation | Same gap |
| F6 | `give_copy(WeaponCopy::new(id).upgrade())` | Direct mutation | Same gap |
| F7 | Writes `DropPowerUp` | Existing message (`economy/power_up.rs`) | The handler is Host-side, so on a client this writes a message nothing there handles (needs a request) |
| F10 | Writes `SummonSapper` | Existing message (`enemies/sapper.rs`) | Same shape as F7 |
| F4 | `spawn_debug_player`: a second Player in Seat 1 | Direct spawn (`players/player/mod.rs`) | Solo only; makes no sense in a Room |
| F8 | Pulses an `Overlay` | Local HUD | Local only, fine |
| `[` | `claim_every_generator` | Mutates `Generators`, `in_set(HostSimulation)` | Host only; a client's press does nothing |
| `]` | `lose_nearest_generator` | Same | Same |
| `\` | `shifting::debug_swing` | Host set | Host only |

Hotkeys that live outside `debug.rs` and will also need gating:

- **F9**: `toggle_unlock_all` in `src/run/gauntlet/mod.rs` (a `UnlockAll` resource that makes every Boon and Pact count as Unlocked in a Gauntlet Run; its doc comment calls it "Debug").
- **`\` again**: `turn_now` in `src/maps/tide.rs` forces the Tide to turn. It shares the key with `debug_swing`, so one press does both.
- **F4 collision**: `src/players/controls.rs` lists F4, F5, F11, F12, `[`, `]` and `\` as rebindable key names, so a Player can bind a game action onto a debug key.

The F-key constraint from the map (a Mac intercepts them) means the gating ticket should expect to move F1 to F10 off F-keys as well as behind the switch. That is #449's concern; this note only records the inventory.

## Cheat by cheat

### Invincibility

How harm reaches a Player: every source writes `PlayerHit { player, damage, from }` (`players/player/mod.rs`); `take_hits` subtracts it (skipping Players in Last Stand or awaiting Respawn); `go_down` (`players/last_stand.rs`) turns Health at zero into a Down, a Last Stand or Game Over.

Two existing hooks, neither sufficient:

- `DownGuard { charges }` (ADR 0007's "would Down" guard) turns a hit that would Down into 1 Health and spends a charge. A huge `charges` looks infinite, but (a) Gauntlet's Grit set bonus does `commands.entity(player).insert(DownGuard { charges: SET_GRIT_4_GUARDS_PER_ROUND })` each Round (`run/gauntlet/sets.rs`), and an `insert` replaces the component, so dev mode and Grit would clobber each other; (b) it leaves the Player at 1 Health, not full; (c) `catch_out_of_bounds` (`players/out_of_bounds.rs`) writes `health.current = 0.0` directly, which skips `take_hits` and so skips the guard.
- A `Stat::MaxHealth` entry only makes the Player tougher.

So invincibility needs a new generic seam. Shape: a marker component (neutral default is absent) that `take_hits` checks next to `last_stand`/`awaiting`, and that `catch_out_of_bounds` checks before zeroing Health. Both are Host systems (`go_down` is `in_set(HostSimulation)`), so Co-op needs no client work: the Host skips the hit, and `tell_health` (`net/world/mod.rs`) never sends a fall. Whether it should also stop Out of Bounds Downs, drowning-style Teleports and the Sapper's effects is a behaviour question for #448; the seam must at least cover the two paths above.

### Infinite Points

`Points` (`economy/points.rs`) is a per-Player component with `award`, `spend`, `refund`. Existing routes:

- `Stat::Price` (`modifier.rs`): "every price the purchase flow charges", resolved by `resolve_points(Stat::Price, price, player)` in `economy/buy.rs`. A `Mult(0.0)` entry makes purchases free. Gauntlet's Inflation Heat Condition uses the same stat (`run/gauntlet/heat.rs`), so this is the intended route. Spending then still "shows" (balance unchanged, price 0).
- `BonusPoints { player, amount }` tops up the balance, awarded like any Points (not in Last Stand).
- `SpendPoints` is the non-purchase spend path (Reroll and Banish tools use it). It is not a Buyable, so a `Price` entry may not cover it; check when the ticket is built.

Co-op: Points are judged on the Host, and the Host sends a client its own balance in the snapshot (`snapshot.own.points`, a `u32`) and teammates' via `WorldEvent::Points`. A top-up must therefore stay below `u32::MAX`, and a `Price` entry must sit on the Host's copy of that Player's `Modifiers`. Both reach the client with no wire change.

### Infinite ammo

Existing stats: `Stat::ReserveFeed` ("Weapons fire straight from Reserve Ammo with no Reloads", base 0), `ReserveSize`, `MagazineSize`, `Stat::SprintDrain` (sprint is separate). `RefillMagazine { player, bullets }` (`weapons/weapon.rs`) refills the held Magazine. `Loadout::max_ammo`/`refill` do what Max Ammo and buying ammo do.

Co-op: the `Loadout` is the owning client's (ADR 0003: "owns its own Weapon state"), and shots and Reloads resolve on that client, so the entry has to be on the client's own `Modifiers`. That means the Host's decision about permission has to reach the client (ticket #447's "what a Player who joins or returns sees"), and the client applies the entry locally. Equipment (frags, Claymores, Monkey Bombs) has `GiveLethals` (`weapons/equipment.rs`) but nothing for Claymores or Monkey Bombs, so "infinite Equipment" is a gap; Pack-a-Punch cost is a Buyable, so `Stat::PackAPunchPrice` or `Price` covers it.

Unverified: exactly which machine resolves each ammo-related stat (client vs Host copy of the `Loadout`) should be confirmed in the build ticket before choosing where the entry is installed.

### Round jump

`Round` (`run/round.rs`) is a plain `Resource` with a private constructor `Round::begin_in(number, schedule, player, hounds_on, players, reserved)`. The only writers are `start_first_round` and `run_intermission`. There is no request message for a Round, so this needs a new seam: a Classic-neutral "begin Round N now" message handled in `round.rs` on the Host. Points the handler must settle (for #448): live Zombies of the old Round (`ZombieCleanedUp` re-queues into the Horde; `CarriesOver` Zombies survive), the Hellhound schedule and `ReservedRounds`, and what Gauntlet keeps keyed to Round numbers (`RoundVariety`, Boss Rounds, Offers). Gauntlet reacts to `RoundStarted`/`RoundEnded`, so a message that goes through `begin_in` and writes `RoundStarted` keeps ADR 0002's rule.

Co-op: the Host's `Mirror` diffs `Round` and tells clients with `WorldEvent::Round { number, hellhound }`, so the visible effect needs no wire change. The trigger from a client is a new `Request`.

### Kill all

`GivePowerUp(PowerUp::Nuke, None)` (`economy/power_up.rs`) is the existing, already-used (Lucky Strike uses `GivePowerUp`) route: the Nuke damages every Zombie with `Attack::Nuke`, whose kills are Unpaid. The Nuke also plays its announcer line and flash, and `WorldEvent::PowerUp` tells clients. If the cheat should be silent or should hit Bosses and the special slot, check `Nuked`; otherwise a direct `DamageZombie { player: None, damage: f32::INFINITY, attack: ... }` per Zombie is possible, but `Attack` has no "cheat" variant, so that would need a new neutral variant. Prefer the Nuke unless #448 wants otherwise.

### Spawn a Zombie kind

`SpawnZombie` (`enemies/request.rs`, ADR 0007) is the right seam. `RequestedKind` today has only `Napalm`, `Sapper`, `Hellhound` and `Walker`. Missing: Shrieker, Nova Crawler, Boss, Elites and the Sprinter/Runner Speed Tiers (a request takes `health_factor`, `scale`, `holds_round`, `SpawnAt::Point` or `BarrierInActiveZone`, and a `SpawnProfile`). Adding variants is "a new seam" in the ADR 0007 sense: each must still mean something without dev mode. Co-op: Host-only, by client request; clients see the Zombie through the normal snapshot and `Tags`.

### Grant Power-up, Weapon, Boon, Pact, Kit

- **Power-up**: `GivePowerUp(PowerUp, Option<Entity>)`, existing.
- **Weapon**: no message. `debug.rs` calls `Loadout::give` on the local Player. For Co-op parity a new Classic message `GiveWeapon { player, copy }` is needed, handled on the Host, which edits its copy of the target Player's `Loadout`. That records a `HostOp::Put`, and the existing `tell_hands` path (`net/world/mod.rs`) sends `WorldEvent::Hands` to the client, exactly as a buy does. So the wire already supports it.
- **Boon / Pact**: Gauntlet-owned. `PickOffer` needs an open `Offer`; `apply_pick` (`run/gauntlet/mod.rs`) is the only place that adds to `HeldBoons`/`HeldPacts`. `apply_entries` then rebuilds the Player's Modifiers from them, and `boons_wire`/`pacts_wire` tell clients. A grant seam belongs inside the Gauntlet layer (dev mode may depend on Gauntlet; Classic must not name Boons). Whether Gauntlet things are offered in Classic is #448's question; in Classic they have nothing to attach to.
- **Kit**: chosen before the Run (`ChosenKit`, `PlayerKit`, `kits.rs`) and fixed as the Run begins; there is no mid-Run swap, so a Kit grant is a larger change than the rest (the Kit sets the opening Loadout and one rule).

### Open all Doors and Zones

`Door { open, price, joins }` (`maps/door.rs`). Doors open from `Purchased` (via `open_bought_doors`), from Power (`open_power_doors`) and from Links (`open_linked_doors`), all through the `pub(crate) fn open`. Zones have no switch of their own: `update_active_zones` (`maps/zone.rs`) builds `ActiveZones` from `PlayerZone`, `fixed_joins` and open Doors' `joins`. So "open all Zones" is "open all Doors", and dev mode should reuse the `open` path (a message that sets Doors open, or writing `Purchased` for each). Co-op: `WorldEvent::Door` is already sent when a Door opens; the client opens its copy on that event. Host-only, by client request.

### Round-hold off

`IntermissionHold(pub bool)` (`run/round.rs`) is a Classic resource: while true the Intermission stays at its full length. Gauntlet's `hold_intermission` rewrites it every frame (`if hold.0 != held`), so a dev write to it would be overwritten in a Gauntlet Run. The two readings of "Round-hold off" need #448 to choose:

1. Hold the Intermission: add a second input to the hold (a generic "also held" flag or a combining rule), so Gauntlet and dev mode do not fight.
2. Stop the Round from ending or the Horde from spawning: no existing hook (`end_round` and `spawn_horde` read `Round` directly), so a new neutral seam would be needed.

Co-op: Host-only, by client request. The Host's `Mirror` already tells the Intermission and Round state.

### Noclip

Movement is the one place a client predicts (ADR 0003). The step is `step_move` (`players/player/mod.rs`), a pure function of position, `Locomotion`, `MoveInput` and a `MoveContext`, called by the local `move_player`, by the client's replay in `net/play.rs`, and by the Host for each remote Player (`net/play.rs`, same function). The wire carries `Step { axes, buttons, yaw, pitch }` (`net/state.rs`); `buttons` uses bits 0 to 5 (Jump, Sprint, Aim, Crouch, Prone, Grab), so bits 6 and 7 are free.

New seam, in the shape ADR 0003 already prescribes: a `noclip` field on `MoveInput`, a `NOCLIP = 64` bit in `Step`, and a branch in `step_move` that moves the position by the wish direction without `move_and_slide` and without gravity. The Host replays the same bit, so client and Host agree and no correction is needed. Because the Host steps each remote Player from the bit, it is also where permission is enforced (ignore the bit for a Player not allowed), so a modified client cannot fly without the Host's access setting. Adding a bit does not change the packet size, but `Step`'s layout is part of the wire, so ADR 0003 says to raise the ALPN (`zombies/1`) and `Hello` version.

Interactions to settle in #448:

- `catch_out_of_bounds` would Down a flying Player (it reads `layout.out_of_bounds`), and `Footing` tracks the last legal spot; noclip needs to be exempt there (the same marker as invincibility could serve) and needs a "get back to a legal spot" rule.
- `track_player` updates `PlayerZone` from the Transform, so a noclipping Player still wakes the Zones they pass through, which spawns Zombies there.
- Other Players see the pose through the snapshot; the Host fills stance and airborne from its own step, which will be right.
- Collision layers (`layers.rs`): the body stays in `GameLayer::Player`, so Zombies still hit a flying Player unless invincibility is also on.

## Cross-cutting findings

1. **Three routes cover Co-op, and each cheat fits one.** (a) Host-side effect, triggered by a client `Request` and carried out through an existing message; the existing `Mirror` diffs then tell everyone (Doors, Round, Power-ups, Health, Points). (b) A stat or component on a machine that resolves it: Host-resolved stats (Price, HordeSize, ZombieSpeed) go on the Host's copy of the Player's `Modifiers`; client-resolved stats (ReserveFeed, SprintDrain, Damage) go on that client's. (c) Predicted movement (noclip only). Do not invent a fourth.
2. **There is one new wire variant.** Cheats from a client need a new `Request` variant in `net/world/mod.rs` (for example `Request::Cheat(...)`), plus a `Step` bit for noclip. Both break the format, so bump the ALPN and `Hello` version per ADR 0003.
3. **Permission is checked where the effect is judged.** Host-judged cheats are refused in the Host's request handler; the noclip bit is dropped in the Host's step; client-resolved stats need the Host's access setting delivered to the client (#447).
4. **Never share a component with a layer.** `DownGuard` and `IntermissionHold` are both rewritten by Gauntlet; dev mode must use its own marker or a combining rule, or the layers will undo each other.
5. **Classic-neutral, always.** Each new seam above (Round request, `GiveWeapon`, invincibility marker, more `RequestedKind`s, noclip flag) must have a neutral default and meaning outside dev mode, per ADR 0007. The `Stat` list has no dev-mode entry and should not get one.
6. **The "writes nothing" gate is #446's.** `record_run` (`storage/records.rs`) runs on `OnEnter(GameOver)`, on leaving `Playing`, on `AppExit` and on `SaveRecord`; `earn_commendations` (`run/gauntlet/commendations.rs`) earns Scrip. Resources like `RunRecorded` already say "this Run's Record is saved", so a single generic "this Run doesn't count" resource read by those systems is the likely shape, and every cheat above would set it. In Co-op each client writes its own, so it must reach every machine (a `WorldEvent`), not only the Host.
7. **Existing `debug.rs` hotkeys are not Co-op safe.** F2, F3, F6 desync the Host's copy of a client's Weapons; F7 and F10 write Host-only messages from a client; F4 is solo-only. Re-homing them as commands (a later ticket) should route them through the same three routes.

## Open questions left for the grilling tickets

- Round jump: what happens to the live Horde, Hellhound schedule, Offers and Gauntlet arc state (#448).
- Invincibility: does it cover Out of Bounds, Teleporter and Sapper effects (#448)?
- Round-hold off: which of the two readings (#448).
- Infinite Points: does `SpendPoints` bypass `Stat::Price` (verify in the build ticket).
- Which machine resolves each ammo stat (verify in the build ticket).
- Grants of Gauntlet-only things in a Classic Run (#448).
