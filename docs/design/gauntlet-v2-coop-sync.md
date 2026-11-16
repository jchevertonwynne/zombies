# Gauntlet v2 in Co-op: what goes over the wire

Design for closing the Co-op parity gaps listed on [#161](https://github.com/jchevertonwynne/zombies/issues/161#issuecomment-6918544265), found by the audit in #390. Co-op is first-class, so every Gauntlet v2 feature has to work for every Player. This doc says, for each gap, the cheapest way to get the state to clients. Each one is one of these:

- **SEND**: new bytes, with where they go, how big they are and how often.
- **INFER**: the client works it out from what it already has, for 0 bytes.
- **COMPRESS**: packed into spare bits, sent at a lower rate, or moved off the snapshot.
- **DEFER/REJECT**: not done, with the reason.

It ends with the rules that were the user's call (now settled) and a split into tickets.

Terms are as in `GLOSSARY.md`. The netcode is as in ADR 0003.

## 1. What a client has today (checked against the code)

| Channel | What it carries | Where |
|---|---|---|
| Snapshot datagram, 32 Hz, one per Player, at most `SNAPSHOT_MAX_BYTES` = 1000 B | `tick`, `used`, `Own` (f32 position, f32 velocity, f32 stamina, bits, Points), a `Sight` per teammate (about 17 B, **including their Points as a `u32` varint**), up to `SNAPSHOT_MAX_CREATURES` = 80 `ZombieSight`s (at most 10 B each) split into `zombies`/`hellhounds`, and up to `SNAPSHOT_MAX_SPECIALS` = 12 `SpecialSight { id, body }` | `src/net/state.rs` |
| Reliable `ToPlayer::Event(WorldEvent)` | Rounds (`Round`, `RoundOver`, which the client turns into `RoundStarted`/`RoundEnded` messages), Buys (the client writes `Purchased` for its own buys), Perks, Status, `Health`, **the Player's own `Offer(OfferWire)`/`OfferClosed`**, their own `Boons`, everyone's `Pacts` with holder Seats, Sabotage and more. Catch-up for a returning Player: `catch_up_world`/`catch_up_player` | `src/net/world/mod.rs` |
| Reliable `ToPlayer::Launched`/`Burst` | Every `Explosion` on the Host goes out as `Burst { at, radius }`. That already covers a Volatile Elite's blast and a Supply Drop frag going off | `src/net/hits.rs` `announce` |
| Reliable `ToHost::Request` | `Use`, `Held`, `Revive`, `Paused`, `Pick { card }`. **There's no Reroll or Banish** | `src/net/world/mod.rs` |
| `ToHost::Hello`/`Unlocks` → Room | `Unlocks { unlocked, tracks: Vec<TrackState> }`. **Tracks arrive but aren't applied. There's no Kit** | `src/net/protocol.rs`, `src/run/party.rs` `remote_unlocks` |
| `ToPlayer::Room`/`Start(RoomView)` | `heat: u8`, which is only the Host's total | `src/net/protocol.rs` |

Facts from the code that the design depends on:

- **`SpecialSight.body` has spare bits.** Bits 0–2 hold the body code (1–4 are used, 5–7 are free) and bit 3 is the "marked" flag. **Bits 4–7 are free.** The `anim` byte has no spare bits that are always free (codes 10–15 are free, but bits 4–7 are in use). A Zombie id uses all 14 bits of its 2-byte varint.
- **Bit 3 is already sent every snapshot for the Sapper's helmet.** The client only acts on it when it goes from 0 to 1 (`show_zombies` calls `lose_helmet`).
- **A Turned ally keeps its `ZombieId`.** `Turn::apply` removes `Zombie` but not `ZombieId`, so the id stays valid. `describe_horde` queries `&Zombie`, so the ally drops out and the client despawns its puppet.
- **`Banishes` is already per Player** on the Host (a Component). Only its starting value is wrong for a remote Player: `give_gauntlet_state` adds the Banish Track only to the `LocalPlayer`.
- **The Banished list is never shown.** It only feeds the draw (`RunFacts::banished`), and the draw runs on the Host. The client doesn't need it.
- **A remote Player's Loadout on the Host is Classic's.** `spawn_player` gives only the `LocalPlayer` the `StartingLoadout`. A remote Player gets `Loadout::new(STARTING_WEAPON)` and the default Knife and Equipment, while their own machine starts them with their Kit's. So a Lead or Blade Kit already splits the two machines' view of that Player's Weapons, and Pack-a-Punch upgrades the Host's copy. This is a bug, not just something missing on screen.
- **A client applies its own save's Heat.** `set_run_modifiers` sets `RunHeat` from the local `GauntletFile` on every machine. A client therefore puts *its own* Inflation price entries on its own Player, which is wrong both ways.
- **Round rewards go only to the Host's Player.** `open_offer` reads `variety.reward_for` only `if local`.
- **`OfferWire` is `(round, pacts, ids)`**. It has no Reroll count, no Banish charges and no `picks`/`picked`. So once Round Choices are on in Co-op, a Boosted Offer's extra pick would be lost too. That's an adjacent gap.
- **Ammo Mod flashes are Host-only in Classic too.** `ammo_mod::Flashes` is filled by `HostSimulation` systems. The same goes for Turned allies, which vanish in Classic Co-op as well. The cosmetic channel and the ally fix below close both Modes at once.
- **The Host sees each remote Player's pose action** (reload, Knife, throw…) in every `Inputs` datagram (`pose: u16`). That's enough to judge Full Mag without a new message.

## 2. Snapshot byte ledger

Today's worst case, as `four_players_and_a_full_horde_fit_in_a_datagram` builds it, is **947 B**:

| Part | Bytes |
|---|---|
| `tick` + `used` at `u32::MAX` | 10 |
| `Own` | 34 |
| 3 teammates' `Sight`s plus the length byte (each 17 B, 5 B of it their Points) | 52 |
| 80 creatures plus 2 length bytes | 802 |
| 12 specials plus the length byte, **each at `id: u16::MAX` = 4 B** | 49 |
| **Total** | **947** (53 B headroom) |

Every gap except Turned allies can be closed for **0 snapshot bytes**. Allies have to be in the snapshot, because they move every frame. Making room for them:

| Change | Δ worst case |
|---|---|
| The specials test uses `u16::MAX` ids, but ids are always below `ZOMBIE_ID_SPAN`, so each special is 3 B, not 4. Fixing the test frees bytes the test only appears to use. | −12 B |
| Teammates' Points leave the snapshot for a reliable `WorldEvent::Points { seat, points }`, sent on change and at most 8 Hz per Seat. The receiver's own Points stay in `Own`. **This amends ADR 0003.** | −15 B |
| `Snapshot::allies: Vec<ZombieSight>` length byte | +1 B |
| `SNAPSHOT_MAX_CREATURES` goes from 80 to **87**, now counting allies. That's 7 more 10 B entries. | +70 B |
| **New worst case** | **991 B** (9 B headroom) |

Why 87: the Gauntlet's worst is 78 alive plus 7 allies. That's 3 under `GAUNTLET_TURNED_ALLY_CAP`, plus up to 4 Bodyguards (Grit 6, one per Player, `OutsideAllyCap`). Allies sit outside the Alive Cap, so the Horde refills behind them and they add to the count. Classic has no ally cap, but the 15 s Turned cooldown per Player against the 20 s lifetime keeps it to about 2 per Player, 8 in all. Classic's Horde is far below 79, so one shared cap of 87 covers both. `fit_horde` keeps its rule: when over, drop the dying first, then the highest ids, from all three lists.

Optional reserve if more is needed later. Neither is needed now, and both are in priority order:

1. `Own.velocity` as three `i16` at 1 cm/s (−6 B) and `stamina` as a `u16` (−2 B), giving 983 B. A 0.5 cm/s error replayed over about 150 ms is under 1 mm.
2. Delta-code the sorted ids within one snapshot. That saves about 1 B per creature in practice, but nothing in the worst case, so it never counts toward the guarantee.
3. The ADR's sanctioned fallback: split the creatures over two self-contained datagrams.
4. Raise `SNAPSHOT_MAX_BYTES` toward about 1100. QUIC guarantees 1200 B UDP payloads, but iroh relays add overhead. Only on the user's call (D9).

## 3. Per-gap decisions

### 3.1 Round Choices (Twists and Challenges): SEND (reliable) + INFER

The Host still draws the options and owns the result. Clients mirror `RoundVariety` from events, and the existing `draw_fog`, `update_hud`, `show_choice`, `light_panels` and twist stat entries then run on them unchanged. That matters for **Low Gravity**: `PlayerGravity` feeds the client's own movement prediction, so the client must have the entry, or every jump gets corrected.

New reliable events, all 0 snapshot bytes:

| Message | Contents | Size | When |
|---|---|---|---|
| `WorldEvent::RoundChoice { round, options: [(Option<u8>, Option<u8>); 2], lit }` | each option's Twist and Challenge as indices | ≈ 10 B | when drawn, and when `lit` changes |
| `WorldEvent::RoundVotes([Option<u8>; 4])` | each Seat's vote | ≈ 8 B | on change |
| `WorldEvent::RoundPicked { round, option }` | the result | 3 B | once |
| `WorldEvent::Challenge { standing, kills, blitz_left_ds: Option<u16> }` | the live Challenge | ≈ 5 B | on change, coalesced to at most 4 Hz |
| `ToHost::Request(Request::VoteRound { option })` | | 2 B | on click or key |

What the client works out for itself (INFER): the Twist banner and its timers, the Challenge HUD line from `Challenge`, the Fog distance, `RoundStartCue`, `ChallengeMet`/`ChallengeBreach` cues (from `standing` changing), the Blitz countdown between events, and `challenges_met`/`twists_survived` for its own Scrip and Run Record (from `RoundPicked` plus `RoundEnded` plus the last `standing`).

The Host judges remote Players' Challenge events, so no new bytes are needed for those:

- kills: `ZombieKilled.player`
- Tight Fist: `SpendPoints` on the Host
- Hold the Line: each Player's `PlayerZone` on the Host
- Full Mag: a remote Player's pose action turning to Reload in their `Inputs`. That's a reload **started**, where solo counts one finished, which only differs for a cancelled reload. An exact `Request::Reloaded` (1 B) is the fallback if that matters.
- Blitz: the Host's clock

Host changes:

- `has_round_choices` returns true in Co-op.
- `track_challenge` reads every Player, not only the `LocalPlayer`.
- The reward becomes a per-Player pending Boost (a Component), so every Player's next Boon or Boss Offer gets it. Today only `local` does.
- `OfferWire` carries `picks`/`picked` (see 3.4).
- Swarm's Alive Cap already fits (#390).

Rule defaults (D1, D2): **a vote**. The majority wins, a tie goes to the Host's vote, and no vote by the timeout picks the safe option. The timeout is `GAUNTLET_ROUND_CHOICE_COOP_SECS`, defaulting to `BOON_OFFER_COOP_TIMEOUT_SECS`. The world runs meanwhile, as Offers do. **Challenges are team-wide**: every Player's kills count, any Player's breach breaks it, and everyone gets the reward.

### 3.2 Elite traits and tints: SEND (reliable, once per Elite) + INFER

A trait is fixed at spawn and never changes, so it goes once over the reliable stream rather than every snapshot. Putting it in the specials list every snapshot would cost 5 × 3 = 15 B, which doesn't fit beside the allies (1006 B).

- **`WorldEvent::Tags(Vec<(u16, Tag)>)`**, batched per frame. `Tag::Elite(u8)` is about 4 B per Elite. Elites come at most a few per Round, so the rate is negligible.
- The Host keeps a table of live tags and replays it in `catch_up_world` for a returning Player.
- The client keeps tags by id in `Crowd` until the puppet exists, because the reliable stream can beat the datagram. It inserts `Elite(trait)` on the puppet, and `tint_elites` (which reacts to `Added<Elite>`) tints it. Tags are dropped when the id goes.
- **INFER:** `draw_rally_rings` runs as is on puppets with `Elite(Rallying)`. A Volatile's **fuse ring** is shown when a `Volatile` puppet starts dying: a local cosmetic `Fuse` at its feet for `GAUNTLET_ELITE_VOLATILE_FUSE_SECS`. The blast itself already comes as `Burst`.
- The `EliteSpawn` cue goes as a `Cue` event (3.6), or is inferred when the tag arrives. Inferring it is the default, at 0 extra bytes.

*Fallback (COMPRESS):* if the pending-tag handling proves fiddly, rotate one Elite per snapshot through the specials list (body code 0, trait in bits 4–6). That costs 3 B worst case and heals itself without catch-up.

### 3.3 Bosses

| Gap | Verdict | Bytes |
|---|---|---|
| Which puppet is the Boss | **COMPRESS**: `SpecialSight.body` bit 4 = Boss. The Pack Alpha, a hound, gets a specials entry with the new body code 5 (Hellhound). It takes the Boss slot already counted in the specials worst case of 9 (slot 1, Sapper 1, Boss 1, Nova 6). | 0 against the ledger |
| Scale | **INFER** from boss bit + body: `GAUNTLET_BLOATER_SCALE`, `GAUNTLET_PACK_ALPHA_SCALE`, `GAUNTLET_WARDEN_SCALE`, applied when the puppet spawns (the specials entry comes in the same snapshot as its first sight) | 0 |
| Warden's helmet re-forming | **INFER**: bit 3 is already sent every snapshot. `show_zombies` re-dresses the Sapper puppet when the bit clears (a `regain_helmet` beside `lose_helmet`), with a test beside the Sapper test in `net/movement.rs` | 0 |
| Boss HUD warning banner, phase marks, name | **INFER**: `boss_of(round)` from the `Round` event. `plan_boss` and the warning countdown move out of `HostSimulation` (they only set `BossPlan` and play a cue). Marks come from `BossKind::phase_marks` | 0 |
| Boss HUD health bar | **SEND** `WorldEvent::BossHealth(u8)`, the share in 1/255ths, when the byte changes, at most 10 Hz | ≈ 2 B per event, under about 20 B/s |
| `BossWarning` cue | **INFER** with the banner | 0 |
| `BossHowl` cue | **SEND** as a `Cue` event (3.6), every `GAUNTLET_PACK_ALPHA_HOWL_SECS` (20 s) | ≈ 9 B |
| Bloater burst ring | **SEND** as an `Fx::GrowingRing { at, radius, secs }` (3.6), at most one per phase mark | ≈ 10 B |

`update_boss_hud` gets a client source. A small `BossView { kind, share }` resource is filled by the Host from `Boss`/`Zombie` and by a client from the events above. The HUD reads only that.

### 3.4 Reroll, Banish and the Banished list: SEND (reliable) / REJECT the list

- `OfferWire` becomes a struct: `{ round, pacts, cards, rerolls: u8, charges: u8, picks: u8, picked: u8 }`. That's +4 B per Offer message, which is sent only on change. `tell_offers` already re-sends whenever the wire differs, so a Reroll (new cards, `rerolls + 1`) or a Banish (new cards, `charges − 1`) reaches the client without new code paths.
- `Request::Reroll` (1 B) and `Request::Banish { card: u8 }` (2 B). The Host turns them into `RerollOffer`/`BanishCard` for that Seat's Player. `apply_reroll`/`apply_banish` already work for any Player: `local` only gates the cue. The Points come through `SpendPoints` as today.
- `click_tool` on a client forwards instead of writing the message locally, as `forward_picks` does.
- The client's price, `reroll_price_from(heat.reroll_base_price(), rerolls)`, is right once 3.7 gives the client the Host's Degrees.
- **The Banished list: REJECT.** The client never reads it. All draws happen on the Host, and nothing shows it. If a "Banished" panel is ever added, it can be a one-off `WorldEvent::Banished(Vec<String>)` sent with the Offer.
- `ToolUsed` on the client (for `win_no_tools`) is **INFERRED** when the Offer comes back with `rerolls` raised or `charges` lowered.

### 3.5 Kit and Banish Track for remote Players

| Gap | Verdict | Bytes |
|---|---|---|
| Kit | **SEND** `kit: String` (its stable id) in `Unlocks`, through `Hello` and `ToHost::Unlocks`. The Host sets the remote Player's `PlayerKit` and, through a **per-Player `StartingLoadout`** (it's a Resource today), gives their body the Kit's Loadout at spawn and on Respawn. That fixes the M14/Bowie split. Spark, Gold, Grit and Ember entries and the Blade first-Offer boost then work for remote Players with no more changes. | ≤ 8 B once, reliable |
| Banish Track | **INFER on the Host** from `Unlocks.tracks`, which already arrives. It belongs with the first #161 comment's fix, applying remote Tracks: `give_gauntlet_state` reads the Seat's `TrackState`s from the Room, not only the local save's. | 0 |

### 3.6 Proc, Set and Duo visuals: SEND cosmetic events (reliable), INFER where possible

Audit of what is Host-only (filled only by `HostSimulation` systems):

| Visual | Source | Verdict |
|---|---|---|
| Ammo Mod flashes (lines, bursts, rings; Ball Lightning's Shock chains go through these). **Classic too** | `ammo_mod::Flashes` | SEND `Fx` |
| Proc Blast spheres, Frost bursts, accent rings | `procs::ProcFlashes` | SEND `Fx`. The rate is already bounded by `VisualGate` (8 alive per 0.25 s) |
| Frosted Zombie ring and sphere | `procs::Frosted` on Host Zombies | SEND `Tag::Frosted { tenths: u8 }` in the 3.2 batch, only when the frost lasts past what was last told by more than 0.5 s |
| Ricochet tracer, Cleave arc, Jackpot burst, Scorched disc | `sets::SetFlashes` | SEND `Fx` |
| Cleave frost patches | `sets::FrostPatch` | SEND `Fx::Ring { secs }`. The client fades it locally |
| Storm Aura (Spark 6) ring round a teammate | `SetBonuses` from `HeldBoons`. The client has only its own | SEND `WorldEvent::Capstones { seat, families: u8 }`, a bitmask of 6-Boon Sets, on change (≈ 3 B, rare) |
| Supply Drop shell arc | `duos::SupplyShell` | SEND `Fx::Arc { from, to }` (≈ 14 B). The client flies `shell_arc` locally. The blast already comes as `Burst` |
| Set and Duo banners and tier cues | `HeldBoons` changing | INFER. They already run from the client's own `Boons` event |
| Host-local `PlayCue`s clients miss (`BossHowl`, proc cues already thinned by `Thinning`) | | SEND `WorldEvent::Cue { cue: u8, at: Option<[u8; 6]> }` (≈ 9 B) |

The wire shape is one batched `ToPlayer::Fx(Vec<Fx>)` per frame to every peer.

- Each `Fx` is a tag byte, positions as `pack_position` 6 B triples, a radius as a `u8` in decimetres, a palette index `u8` and a life as a `u8` in 10 ms steps. That's 10–15 B each.
- On a client, each `Fx` is pushed into the same `Flashes`, `ProcFlashes` or `SetFlashes` resource, and the existing `draw_*` systems (already outside `HostSimulation`) draw it.
- A per-frame cap (`NET_FX_MAX_PER_FRAME`, about 24) bounds bursts. Over the cap, the newest are dropped, because they're cosmetic.
- Expected load is under about 1 KB/s per peer. None of it touches the snapshot.

### 3.7 Heat Conditions, Degrees and the Reroll price: SEND (Room)

- `RoomView.heat: u8` becomes `heat: [u8; 8]`: +7 B on a reliable message sent only on change. Packing the Degrees (all ≤ 3) into 2 bits each would make it a `u16`, but isn't worth the code.
- The lobby listing keeps the total.
- On `Start`, a client sets `RunHeat` from the Room, not its save. That gives the right Reroll price, starting Banishes (Tight Offers), the Iron Bosses and Elite Surge text, the Scrip multiplier, `RunHeatRecord` for its Game Over and Run Record, and its own stat entries: Inflation prices in its prompts.
- The client's Armory Heat tab shows "Set by the Host" and is read-only while in a Room. This is UI only, 0 bytes.

### 3.8 Turned allies: SEND (snapshot), COMPRESS the marker

- `Snapshot::allies: Vec<ZombieSight>`. Being on this list *is* the ally marker, just as the `hellhounds` list marks hounds. Budget as in section 2.
- The Host describes allies by `With<TurnedAlly>`, plus allies dying after they expire. Their activity comes from the `TurnedAlly` swing and `Dying`.
- The client keeps the same `Glimpses` entry, because the id doesn't change. When an id moves from `zombies` to `allies`, the puppet is restyled with the ally material and **loses its hit areas**, so the client's shots pass through it as on the Host and it never reports a hit on it.
- A Bodyguard's Grit tint is one rare fact, so it goes as `Tag::Bodyguard` (3.2).
- Galvanised Dead's Shocks and Conscripts' phantom guns are casts, so they arrive as `Fx` (3.6).

### 3.9 Commendations: INFER per machine

Each machine logs its own Player, the same way each machine pays its own Scrip. `log_run`/`earn_commendations` move out of `HostSimulation` to run for the `LocalPlayer` everywhere. Every fact in `RunLog` is already on the client, or comes from the sections above:

| `RunLog` fact | Where a client gets it |
|---|---|
| Most Boons of a Family, most Duos held | its own `Boons` event |
| `bought_juggernog` | its own `Purchased` |
| `used_tools` | inferred from the Offer (3.4) |
| `highest_round`, `cleared` | `RoundStarted`/`RoundEnded` with `Round` |
| `heat` | the Room's Degrees (3.7) |
| Win (`arc.won`) | INFER: `RoundEnded` at `win_round()` sets the client's `RunArc.won` |
| `held_pact` | the `Pacts` event with holder Seats |
| `tracks_off` | its own `ActiveRanks` |
| Elites killed | SEND `WorldEvent::ElitesKilled(u32)`, the Run's running count, on change (≈ 3 B, a few per Round). Counting dying tagged puppets would miss corpses `fit_horde` drops when full. |

0 snapshot bytes. Default rule (D6): every Elite killed in the Run counts for every Player, as solo counts every Elite death.

## 4. Decisions

**D1-D9 are settled at their defaults (the bold entries below).** The tickets state them as settled, not open.

| # | Question | Default | Why it matters |
|---|---|---|---|
| D1 | Who picks the Round Choice in Co-op? | **A vote: majority wins, a tie goes to the Host, no vote by the timeout picks the safe option.** Alternatives: the Host alone (cheaper, about 1 ticket less), or the first click wins | The wire shape of `RoundVotes` |
| D2 | Is a Challenge team-wide or per Player? | **Team-wide**: everyone's kills count, anyone's breach breaks it, and everyone gets the reward. Per Player means a `Challenge` event per Seat and a per-Player HUD | Whether reward and standing are per Seat |
| D3 | Banish charges: per Player or pooled? | **Per Player** (it's already a per-Player Component). Each Player gets the Boss Round charge and their own Track and Tight Offers start | |
| D4 | Kit: each Player's own? | **Each Player's own Kit**, as with Tracks (`kits.rs` doc) | |
| D5 | Is a Banish per Player's pool? | **Yes**: a Boon one Player Banishes can still be offered to a teammate (GLOSSARY: "from the Player's pool") | |
| D6 | Elites for `elites_100`: every Elite death, or only the Player's own kills? | **Every Elite death in the Run**, as solo. Own kills only would need the killer's Seat on each event | |
| D7 | `GAUNTLET_TURNED_ALLY_CAP`: shared by the Run or per Player? | **Shared by the Run (3)**. Per Player would mean up to 16 allies, which doesn't fit the snapshot without the two-datagram split | Sets `SNAPSHOT_MAX_CREATURES` = 87 |
| D8 | Teammates' Points off the snapshot (amends ADR 0003) | **Yes**: a reliable event at most 8 Hz per Seat. It frees 15 B, which the allies need | ADR 0003 has to be edited |
| D9 | Raise `SNAPSHOT_MAX_BYTES` above 1000? | **No**: keep the reserve in section 2 for later | Datagram safety over iroh relays |

## 5. Tickets

All are children of #161. Each is small enough for one agent. The order is the recommended build order.

| # | Title | Scope | Blocked by |
|---|---|---|---|
| T1 (#392) | Co-op snapshot: make room for allies | Teammates' Points move to `WorldEvent::Points`, sent on change and at most 8 Hz per Seat. The worst-case test uses specials ids below `ZOMBIE_ID_SPAN`. ADR 0003 gets the new numbers. Raise `PROTOCOL`. | D8 |
| T2 (#393) | Co-op Tags: Elite traits on puppets | `WorldEvent::Tags` and `Tag::Elite`. The Host keeps a live-tag table and replays it on catch-up. The client holds tags by id until the puppet exists. Elite tint and rally rings on puppets, the Volatile fuse ring inferred, `WorldEvent::ElitesKilled`. Two-machine test. | — |
| T3 (#394) | Co-op Heat: the Host's Degrees on every machine | `RoomView.heat: [u8; 8]`. The client sets `RunHeat` from `Start`. The Armory Heat tab is read-only in a Room. A test that the Reroll price and `RunHeatRecord` match the Host's. | — |
| T4 (#395) | Co-op cosmetic channel: Fx and Cue | `ToPlayer::Fx(Vec<Fx>)` with a per-frame cap, and `WorldEvent::Cue`. Ammo Mod flashes (Classic too), Proc flashes, Set flashes, frost patches, Supply Drop arcs, `Capstones` for Storm Aura, `Tag::Frosted`. A two-machine test that each shows on the client. | T2 (for `Tag::Frosted` only) |
| T5 (#396) | Co-op Bosses: identity, scale, helmet and HUD | The boss bit and body code 5 in `SpecialSight`. Puppet scale. The Warden's helmet re-forms on a puppet, tested beside the Sapper test. `BossView`. `plan_boss` and the warning run on clients. `WorldEvent::BossHealth`. The Bloater burst as `Fx`, the howl as `Cue`. | T4 |
| T6 (#397) | Co-op Turned allies | `Snapshot::allies`. `SNAPSHOT_MAX_CREATURES` = 87, counting allies. `fit_horde` over three lists. The ally puppet's look, with no hit areas. `Tag::Bodyguard`. The worst-case test at 87 plus the new ledger. Fixes Classic Co-op too. | T1, T2, D7 |
| T7 (#398) | Co-op Offer tools: Reroll and Banish for every Player | `OfferWire` becomes a struct (`rerolls`, `charges`, `picks`, `picked`). `Request::Reroll` and `Request::Banish`. Client `click_tool` forwards. `ToolUsed` inferred on the client. Two-machine test for price and charges. | T3 (for the right price) |
| T8 (#399) | Co-op Upgrade Tracks for remote Players, the Banish Track included | Apply `Unlocks.tracks` to the remote Player on the Host: Track entries, Banish Track charges. That covers the first #161 comment. | — |
| T9 (#400) | Co-op Kits for remote Players | `Unlocks.kit`. `StartingLoadout` per Player, at spawn and on Respawn. The Host sets the remote `PlayerKit`. A test that a remote Lead Kit Player holds the M14 on both machines. | D4 |
| T10 (#401) | Co-op Commendations on every machine | `log_run`/`earn_commendations` run for each machine's `LocalPlayer`. Client `RunArc.won` from `RoundEnded` at `win_round()`. Uses `ElitesKilled`, inferred `ToolUsed` and the Room Heat. | T2, T3, T7, D6 |
| T11 (#402) | Co-op Round Choice: the vote and the Twists | `RoundChoice`/`RoundVotes`/`RoundPicked` and `Request::VoteRound`. The client mirrors `RoundVariety`, so Fog, the banner and Low Gravity entries work there. A per-Player pending reward. `has_round_choices` stays off until T12. | T7, D1 |
| T12 (#403) | Co-op Challenges, and Round Choices on | `WorldEvent::Challenge`. The Host judges every Player (Full Mag from the pose action). HUD and cues on clients. Turn `has_round_choices` on for Co-op. Update the GLOSSARY's Round Choice entry ("Co-op has none yet"). | T11, D2 |

Not on this list, because #161's body covers them: Boss health × `BOSS_HEALTH_BY_PLAYERS`, and the per-Player Win choice. The Win needs no new bytes, since each client already infers it (T10).
