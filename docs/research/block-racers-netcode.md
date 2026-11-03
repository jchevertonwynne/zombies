# Research: what Block Racers' netcode brings over

Answers the question on [Research: what Block Racers' netcode brings over](https://github.com/jchevertonwynne/zombies/issues/113), for the map [Classic co-op: 2–4 Players online](https://github.com/jchevertonwynne/zombies/issues/112).

Source: [jchevertonwynne/blockracers](https://github.com/jchevertonwynne/blockracers) at commit
[`4c63ae5`](https://github.com/jchevertonwynne/blockracers/tree/4c63ae52c1a7739c234ce7897562ec2a1bbc9421) (2026-10-20). File links below point at that commit (`BR` = `https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421`).
Zombies facts are from this repo at `fdfb5d4`.

## TL;DR

- **Carries over almost as-is:** `link.rs` (the `Link` trait and the in-memory `Hub`), `transport.rs` (iroh), the `encode`/`decode` half of `protocol.rs`, and the reconnect-token mechanism in `mod.rs`. These files only depend on each other, plus `crate::meshgen::Rng` for the Hub's dice. Change the ALPN (`b"brick-racers/1"`) and swap the RNG for `rand`.
- **Same idea, rewritten:** every message type, `state.rs` (`State`/`Sight`), `host.rs`, `client.rs`, `display.rs`, `scene.rs`, `room.rs`, `modes.rs`. These are written around `Kart`, `Track` and racing modes.
- **The big change:** Block Racers rewinds and replays its own car by calling a plain method, `Kart::advance(&Controls, &Track, dt)`, in a loop. The Zombies Player moves inside a system (`move_player`) that reads `Controls`, `Single<…>` and avian's `MoveAndSlide`. Prediction needs that pulled out into a step function. The per-tick input also has to carry the look direction, which zombies applies every frame outside the fixed step.
- **Bevy 0.19 → 0.20:** nothing in the net code's Bevy API surface breaks. Zombies on 0.20 already uses `Time<Fixed>`, `RunFixedMainLoopSystems`, `TimeUpdateStrategy::ManualDuration` and `MessageReader`. `Query::get_many_mut` is still in 0.20. The 0.20 migration guide's ECS items (`iter_many*` yielding `Result`, `set_if_neq` → `set_if_different`, observer generics) aren't used in `src/net/`.
- **Versions:** Block Racers locks **iroh 1.3.0, postcard 1.1.3, tokio 1.53.2**. Latest on crates.io (2026-11-03): **iroh 1.4.1, postcard 1.1.3, tokio 1.54.1**. Zombies already locks tokio 1.54.1 through another dependency.

## Versions

| Crate | Block Racers `Cargo.toml` | Block Racers `Cargo.lock` | Latest on crates.io | Notes |
|---|---|---|---|---|
| iroh | `"1"` | 1.3.0 | 1.4.1 (2026-10-21) | MSRV 1.91 (crates.io `rust_version`); local rustc is 1.99.1. 1.4.0/1.4.1 are bug fixes plus a relay header rename, with no breaking API changes ([iroh CHANGELOG](https://github.com/n0-computer/iroh/blob/main/CHANGELOG.md)). Its QUIC stack is `noq` 1.3.0. |
| postcard | `"1"`, `default-features = false, features = ["use-std"]` | 1.1.3 | 1.1.3 (2025-07-24) | Unchanged. |
| tokio | `"1"`, `["rt-multi-thread", "sync", "time", "macros"]` | 1.53.2 | 1.54.1 (2026-10-21) | Zombies' `Cargo.lock` already has 1.54.1 (transitive). |
| bevy | `"0.19.1"`, `["wav", "serialize"]` | 0.19.1 | 0.20.1 | Zombies uses `bevy = "0.20"` with default features. Add `"serialize"` for serde on `Vec3`/`Quat`, as Block Racers does. |
| others | `serde 1`, `bytes 1`, `anyhow 1`, `thiserror 2`, `reqwest 0.13` (rustls, no provider) + `rustls 0.23` (`ring`) for the lobby's HTTPS, `arboard 3` for copying the host address | | | `reqwest`/`rustls` are only for the lobby list. `transport::runtime()` installs the ring crypto provider for them. |

Source: [BR `Cargo.toml`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/Cargo.toml), `Cargo.lock`, and the crates.io API (`/api/v1/crates/{iroh,postcard,tokio}`).

## Module sizes (lines, `wc -l`)

| File | Lines | What it is | Port verdict |
|---|---:|---|---|
| `src/net/link.rs` | 258 | `Link` trait, `Event`, `Quality`, test-only `Hub`/`Memory` | **As is** (swap `meshgen::Rng` for `rand`) |
| `src/net/transport.rs` | 486 | iroh endpoint, one reliable bi-stream + datagrams, tokio runtime | **As is** (new ALPN) |
| `src/net/protocol.rs` | 631 | `Tick`/`Peer`/`HOST`, constants, `encode`/`decode`, message enums, `Rules`, `Seat`, `Ride` | Keep header and codec (~50 lines); rewrite messages |
| `src/net/host.rs` | 649 | `Remote` jitter buffer, `drive_remotes`, race `flow`, snapshot `send` | Keep `Remote` almost as is; rewrite the rest |
| `src/net/client.rs` | 475 | `Prediction`: rewind and replay, correction smoothing, extrapolation of others, `bump` | Same idea, rewrite for the Player |
| `src/net/state.rs` | 442 | `State` (the driven car in full), `Sight` (packed view of any car) | Same packing tricks, new fields |
| `src/net/mod.rs` | 1631 | `Role`, `Wire`, `Clock`, `Pending`, `Session`, tokens, `pump`, session/room handling, plugin wiring | Keep `pump`, `Clock`/`Pending` and the token/Absent/Retry logic; the rest is racing-specific |
| `src/net/display.rs` | 90 | Blending between fixed steps for drawing | Not needed: zombies already uses `bevy_transform_interpolation` |
| `src/net/scene.rs` | 388 | Reliable bricks/power-ups/track events and sounds | Pattern for Doors, Power, Box, Power-ups |
| `src/net/room.rs` | 371 | Voting room, results | Lobby/menu ticket |
| `src/net/lobby.rs` | 297 | Game side of the HTTP lobby list | Lobby ticket |
| `src/net/modes.rs` | 1549 | Battle/tag/keep-away/hill modes | Not relevant |
| `src/net/tests.rs` | 2381 | Two headless `App`s joined by a `Hub` | Copy the harness pattern |
| `crates/lobby-api/src/lib.rs` | 81 | Lobby wire types, `PROTOCOL = 13`, `MAX_PLAYERS = 6` | Lobby ticket |
| `crates/lobby/src/{lib,main}.rs` | 865 + 28 | axum lobby server | Lobby ticket |
| **Total** | **10622** | | |

The game-agnostic core (`link.rs` + `transport.rs` + the codec) is about 800 lines with tests.

## Transport (iroh)

[BR `src/net/transport.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/transport.rs)

- `Endpoint::builder(presets::N0).alpns(vec![ALPN.to_vec()]).bind()` uses iroh's public n0 relays. You dial by `EndpointId` (the host's key, as a string), not by IP. No port forwarding. `transport::dialled(s)` checks that a string parses as an `EndpointId`.
- The host awaits `endpoint.online()` before publishing its address (`endpoint.id().to_string()`). Until it has a relay, nobody can dial it.
- **One bidirectional stream per player** carries reliable messages, each framed as a `u32` little-endian length followed by the bytes. `MAX_MESSAGE = 64 KiB` is enforced on read. The client writes a `GREETING` byte (1) first because "a stream isn't there for the other end until something has been sent down it".
- **QUIC datagrams** carry unreliable traffic (`connection.send_datagram`). A `SendDatagramError::TooLarge` is logged once and the datagram dropped. That's why snapshots must stay under `DATAGRAM`.
- iroh needs async and Bevy has no async runtime, so everything runs on a 2-thread tokio runtime (`OnceLock<Runtime>`, thread name `net`). It talks to the game over `tokio::sync::mpsc::unbounded_channel` (game → net, `Out::{Message, Datagram, Close}`) and `std::sync::mpsc` (net → game, `Event`). `Link::poll` is a non-blocking `try_recv`.
- Link quality (`ping` from the selected path's RTT, `direct` = `path.is_ip()` as opposed to relayed) is sampled every second (`MEASURED_EVERY`).
- **Leaving vs losing:** when the client's read ends, it checks `connection.close_reason()`. `ApplicationClosed` means `Event::Left(HOST)`, anything else `Event::Lost(HOST)`. `Lost` triggers redialling. `Transport::redial` just rebuilds the whole endpoint (`*self = Self::start(Some(host))`). The host numbers peers by accept order (`joined += 1`), so a redialled player gets a **new `Peer`**, and identity is restored by token (see below).
- `DIAL_WAIT = 20 s` timeout on `endpoint.connect`.
- An ignored test (`real_endpoints_find_each_other_and_talk`, needs the internet) exercises real endpoints, including a datagram of exactly `DATAGRAM` bytes.

**For zombies:** copy the file. Change `ALPN` (e.g. `b"zombies/1"`) so the two games can never connect to each other. Keep `presets::N0`. The only crate-internal imports are `link::{Event, Link, Quality}` and `protocol::{HOST, Peer}` (plus `DATAGRAM` in the test).

## The `Link` trait and the in-memory `Hub`

[BR `src/net/link.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/link.rs)

```rust
pub trait Link: Send + Sync + 'static {
    fn send(&mut self, to: Peer, bytes: Vec<u8>);      // reliable, ordered
    fn datagram(&mut self, to: Peer, bytes: Vec<u8>);  // unreliable, unordered
    fn poll(&mut self) -> Option<Event>;
    fn address(&self) -> Option<String> { None }
    fn quality(&self, _peer: Peer) -> Option<Quality> { None }
    fn close(&mut self, _peer: Peer) {}
    fn redial(&mut self) -> bool { false }
}
// Event = Joined(Peer) | Left(Peer) | Lost(Peer) | Message(Peer, Vec<u8>) | Datagram(Peer, Vec<u8>) | Failed(String)
```

The game holds it as `#[derive(Resource)] pub struct Wire(pub Box<dyn Link>)`.

**How the Hub simulates delay and loss** (`#[cfg(test)] mod memory`):

- `Hub { inner: Arc<Mutex<Inner>>, delay: u32, loss: f32, jitter: u32, stalls: Arc<Vec<(u32, u32)>> }`. `Inner` holds a clock `now`, a send counter `sent`, a queue per peer of `(due, order, Event)`, and a list of `cut` peers.
- **Time is the test's to advance**: `hub.step()` (+1) or `hub.pass(n)`. The unit is whatever the test chooses. Most tests use one unit per app update (one 60 Hz tick), and `drive_about_unevenly` uses microseconds. Note that `Memory::quality` reports `ping = delay * 2 * 1000 / 60`, which assumes the step unit.
- **Delay:** every post is due at `now + delay (+ late)`.
- **Loss:** each `datagram` rolls the link's own `Rng` (seeded `0x5eed` for the host, `0xfeed + peer` for a player, so runs are deterministic). If the roll is `< loss` the datagram is never queued. Reliable `send`s are never lost.
- **Jitter:** a datagram gets up to `jitter` extra units of delay. `poll` returns the *lowest send order among those due*, so reliable messages stay in order while jittered datagrams can arrive out of order.
- **Stalls:** a datagram sent during a `(from, until)` window is held until `until`, which models a connection freezing.
- **Cut:** `hub.cut(peer)` drops everything in flight from that peer and queues `Left(peer)` to the host and `Lost(HOST)` to the player. Nothing more gets through either way.
- **Redial:** `Memory::redial` calls `hub.join()`, which hands out a fresh `Peer`, the same as the real host.
- The test constants are `DELAY = 5` steps (≈80 ms each way at 60 Hz) and `LOSS = 0.05`. The `ROUGH` profile in `tests.rs` adds 4 ms jitter, ±2 ms uneven frames, 0.5 % 100 ms hitches and a 50–100 ms stall about every 3 s, and asserts at most 3 corrections, the worst under 0.5 units, over 30 s.

**Test harness pattern** ([BR `src/net/tests.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/tests.rs)): `Pair { hub, host: App, guest: App }` sets up two headless `App`s (`MinimalPlugins, StatesPlugin, net::plugin`) with `TimeUpdateStrategy::ManualDuration(Time::<Fixed>::from_hz(TICKS).timestep())`, so each `app.update()` is exactly one tick. `Pair::step()` calls `hub.step()` and then updates both apps. This fits zombies' existing headless tests (`src/zombie/headless.rs`, `src/player/headless.rs` already use `TimeUpdateStrategy::ManualDuration`).

## Protocol (postcard)

[BR `src/net/protocol.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/protocol.rs)

- `encode` is `postcard::to_stdvec(msg).expect(..)` and `decode` is `postcard::from_bytes`, wrapped in `WireError::Unreadable`. `pump` drops and `warn!`s any message that won't decode ("a game of another version, or not a game").
- Constants: `type Tick = u32`, `type Peer = u32`, `HOST = 0`, `TICKS = 60.0`, `SNAPSHOT_EVERY = 2` (30 Hz snapshots), `RESENT = 4` (each `Inputs` repeats the last 4 ticks of input, so a single lost datagram costs nothing), `DATAGRAM = 1000`.
- **Which message goes where is decided by channel and role, not by an envelope.** `pump` decodes a reliable `Message` as `ToHost` on the host and `ToPlayer` on a client, and a `Datagram` as `Inputs` on the host and `Snapshot` on a client. So there is exactly one datagram type per direction.
- Reliable messages: `ToHost::{Hello{protocol, name, password, car, back: Option<u64>}, Loaded, Vote, Ready, Back, Enter, Say, Ride, Side}` and `ToPlayer::{Welcome{you, token, title}, Refused(Version|Password|Full|Removed), Start{rules, seats, map}, Room{..}, Over, Scene(Scene), Events(..), Said, Rides, Circuits}`. Everything after `Hello`/`Welcome`/`Refused` is racing-specific.
- Input sanitising on the host: `Drive::controls()` clamps the analog inputs. "A player is trusted, but a pedal still only goes down so far."
- The protocol version is `lobby_api::PROTOCOL` (13), checked in `Hello` and used as the lobby list filter.

**For zombies:** keep the codec, the constants block, the channel-by-role decoding and the `Hello`/`Welcome`/`Refused` handshake. Replace everything else with the Zombies message set (the map's "Protocol message set" fog). Shooter-judged hits are new: there is no Block Racers precedent for a client reporting an outcome. That needs a reliable `ToHost` hit report (probably reliable, since a lost Headshot report is lost Points). Block Racers only ever sends inputs upstream.

## The fixed 60 Hz tick

[BR `src/net/mod.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/mod.rs) (`plugin`, `begin_tick`, `end_tick`, `latch`, `Clock`, `Pending`)

- `app.insert_resource(Time::<Fixed>::from_hz(TICKS))` with `TICKS = 60.0`. Online, the whole race runs in **`FixedUpdate`** as one `.chain()`, gated `.run_if(in_state(Screen::Race)).run_if(online)`. In order: `begin_tick` → `client::receive` (client) → host `receive / flow / ai_drive / drive_remotes` → `client::send` → items (host) → `kart_physics` → `client::bump` → collisions/modes/pickups (host) → `client::puppets` (client) → `host::send` + `scene::tell` (host) → `end_tick` + `display::note`.
- `Clock { tick: Tick }` is a resource that `begin_tick` increments. Each machine counts its own ticks. They aren't synchronised; the host maps client ticks through `Remote` (below).
- Edge-triggered buttons are read in `Update` and **latched** into `Pending` (`latch`) until the next fixed step takes them (`begin_tick`), then cleared (`end_tick`). A press is never missed on a frame without a step, or used twice on a frame with two.
- `pump` (`PreUpdate`) drains `Link::poll` into an `Inbox` resource. The fixed systems read from `Inbox`.
- Drawing between steps is `display::{unblend, blend, note}`, home-made interpolation in `RunFixedMainLoopSystems::BeforeFixedMainLoop` and `Update`.

**For zombies:** zombies already simulates in `FixedUpdate` (`move_player`, Zombie `rise/chase/tumble/sink`, barriers, specials) and smooths with `bevy_transform_interpolation`. It doesn't set `Time<Fixed>`, so it runs at Bevy's default **64 Hz** (`DEFAULT_TIMESTEP = 15625 µs` in `bevy_time-0.20.0/src/fixed.rs`). So `display.rs` isn't needed. Two things to decide: keep 64 Hz and put `TICKS = 64.0` in the protocol (and fix the Hub's `1000/60` ping formula), or set 60 Hz for everyone. The `Clock`/`Pending` latch carries over directly, keyed to zombies' `Action`s (fire, reload, use, knife, swap).

## Host: one remote input per host tick, no rewind

[BR `src/net/host.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/host.rs)

The host never rewinds. Each remote car has a `Remote` component, an **adaptive jitter buffer** of `(Tick, Drive)`, and each host tick consumes exactly one of the player's ticks (`Remote::next`):

- It starts with `DEPTH = 2` ticks in hand. A missing tick is driven as a repeat of the last input with no new button presses (`Next::Missing`). When it arrives later it is dropped, but any button pressed in it is **owed** to the next tick that is driven (`owe`), so a press is never lost.
- `watch` adapts the buffer over `WINDOW = 60` ticks. If short gaps (≤ `SHORT = 2`) happen twice, it keeps one more tick in hand (`hold += 1`). If more ticks were always waiting than needed, it skips ahead. If the newest input arrives too late `LATE = 6` times in a row, it re-syncs to the newest.
- After `STALLED = 30` ticks (0.5 s) with no input, AI takes over the car until input comes back. For zombies this becomes "the Player stands still" (or keeps the last movement and stops firing).
- `host::send` (every `SNAPSHOT_EVERY` ticks) builds **one snapshot per player**: `Snapshot { tick, used: remote.used(), stage, clocks, own: Some(State::of(their kart)), karts: Vec<Sight>, mark }`. `used` is the last of *that player's* ticks the host has applied, which is what the client rewinds to. Spectators get `own: None`.

**For zombies:** `Remote` (`hear`/`next`/`watch`/`owe`, about 200 lines with tests) carries over almost as is, with `Drive` replaced by the Zombies input. Owed button presses map onto fire, reload, use and knife.

## Client: prediction with rewind and replay

[BR `src/net/client.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/client.rs)

- `client::send` pushes `(clock.tick, Drive::of(controls))` into `Prediction.sent` (capped at `REMEMBERED = 120` ticks = 2 s) and sends `Inputs { last: tick, drives: last RESENT drives }` as a datagram **every tick**.
- `client::receive` ignores snapshots older than `latest`, sorts the new ones and records every car's `Sight` pose. It applies only the newest. If `snapshot.used < clock.tick` it then:
  1. remembers where the car was drawn, and stashes one-shot sound cues so they don't play twice,
  2. `state.put(&mut kart)` (snap to the host's state at `used`),
  3. drops `sent` ticks `<= used`,
  4. **replays** each remaining sent tick with `kart.advance(&drive.controls(), &track, dt)` and re-runs collisions against the other cars, offset to where they were at that tick (`meet_ahead`, `Shunt` knocks),
  5. sets the visual offset to `was − now`. If the correction is over `JUMP = 12` units it snaps; otherwise `smooth` decays it at `SMOOTHING = 12/s`.
- Other cars (`puppets`) are shown `BEHIND = 6` host ticks behind the newest snapshot, interpolated between two poses, then **extrapolated forward** by the player's lead (unacked inputs plus snapshot age, capped `LEAD = 0.35 s`). Jumps are smoothed out (`CATCH_UP = 10/s`).
- `bump` predicts the player's car hitting another car and shows the other car knocked (`Shunt`) until the host's snapshot reflects it.
- If no snapshot arrives for `SILENT = 5 s`, the host counts as lost.

**What changes for zombies (the main porting cost):**

1. **A pure step function.** Replay depends on `Kart::advance(&mut self, &Controls, &Track, dt)` being a method on a plain struct ([BR `src/kart.rs` L1808](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/kart.rs#L1808)). The Zombies Player moves in the `move_player` system (`src/player.rs`). It reads `Single<(Entity, &mut Transform, &mut Locomotion, &Collider, &Loadout, &Modifiers, Has<Drinking>, Has<LastStand>), With<Player>>`, the `Controls` SystemParam and avian's `MoveAndSlide`, and takes `dt` from `Res<Time>`. That needs pulling out into something like `fn step(&mut PlayerMotion, input: &PlayerInput, &MoveAndSlide, &SpatialQueryFilter, dt)`, so the system and the replay loop share it. `MoveAndSlide` can be called repeatedly inside one system, because it is a spatial query against the current collider positions. Replay against walls is therefore cheap and exact.
2. **Look goes in the input.** `look` turns the Player's `Transform` every frame in `RunFixedMainLoop` (`LookSystems`), and `move_player` reads `transform.rotation` for its wish direction. Each tick's input must carry the yaw (and pitch, for shooting) the client had at that tick. Otherwise the host and a replay move in different directions.
3. **The predicted state is bigger than the transform.** `Locomotion` (velocity, grounded, stamina, winded, sprinting) and the speed modifiers from `Loadout`/`Modifiers`/`Drinking`/`LastStand` all feed the step. `State` must carry the mutable ones. Points, Perks and Weapons are host-owned and change rarely, so they go reliably and are not predicted.
4. **Zombies in the move filter.** The Player's move-and-slide filter includes `GameLayer::Zombie`. Replayed ticks see Zombies where the client *currently* shows them, not where they were at that tick. Block Racers solves the same problem for cars with `meet_ahead` offsets. Either copy that, or leave Zombies out of the replay's filter and accept small corrections when a Player pushes into a horde. The cheaper choice is probably fine, because Zombies move slowly and corrections are smoothed.
5. **No AI takeover:** a stalled remote Player just stops.
6. **Shooting** isn't predicted on the host. The shooter judges hits (map decision), so the client runs its own `combat` locally and reports hits reliably. There's nothing to replay for it.

## Keeping the snapshot under 1000 bytes

[BR `src/net/state.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/state.rs), `a_full_grid_is_told_in_one_datagram` in `tests.rs`

- `DATAGRAM = 1000` bytes. iroh refuses (`TooLarge`) anything over the path's datagram limit, and the transport then **drops it silently** (logged once), so this is a hard budget.
- Only the receiving player's own car goes in full (`State`, about 30 fields: pos, vel, rot, facing, contacts, rigid-body momentum, surface, slide state, …). Every car, own included, goes as a packed `Sight`:
  - position as exact `Vec3` (3 × f32),
  - rotation as **smallest-three quaternion**: the largest component is dropped (its index goes in 2 bits of a `bits` byte), the other three are scaled by `32767·√2` into `i16`s, 6 bytes,
  - velocity at 1/64 unit/s in 3 × `i16`, 6 bytes,
  - steer as `i8`; booleans packed into `bits`; six effect timers at 1/20 s in `u8`s (max ≈ 12.75 s),
  - postcard varints shrink the small integers (`slot`, `lap`, `place`, `downs`).
- Lap times change rarely and go reliably with the `Scene`, not in the snapshot.
- The test fills a full 6-car grid with worst-case values (`u32::MAX` ticks, `i32::MAX` laps, every `Option` `Some`) and asserts `encode(&snapshot).len() <= DATAGRAM * 3 / 4` (750 bytes), leaving 25 % headroom. `host::send` also `debug_assert!`s `<= DATAGRAM` on every send.

**For zombies:** the worst case is 4 Players plus `ALIVE_CAP = 24` Zombies (`src/tuning.rs`), plus up to 2 Hellhounds. With the same tricks (exact or `i16`-quantised position, yaw only as an `i16`, a `u8` animation/state, a varint id), a Zombie is about 12–20 bytes, so 26 × 20 ≈ 520 bytes plus 4 Player sights and one own `State`. That fits under 750, but only if Zombie health, pathing and anything else not needed for drawing stay out. Copy the worst-case test as is. If it ever doesn't fit, the fallback is to split Zombies across two alternating snapshots. Block Racers never needed that.

## Reconnect tokens

[BR `src/net/mod.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/mod.rs) (`token`, `Member`, `Absent`, `Retry`, the `Hello` handling in `session`)

- `token() -> u64` comes from a randomly-keyed `RandomState` hasher's `finish() | 1`, so it is never 0 (0 means "no token"). The host sends it in `ToPlayer::Welcome { you, token, title }`, and the client stores it in `Session.token`.
- On every `Joined(HOST)` the client sends `Hello { .., back: Some(token).filter(|t| *t != 0) }`.
- The host keeps `Session.absent: Vec<Absent { token, peer, name, points }>`, capped at `ABSENT = 32`. A `Hello` whose `back` matches an absent entry gets the **same token** back, its points, and its grid seat re-pointed from the old `Peer` to the new one. A name proves nothing ("being anyone's to take").
- **Stale member:** if a `Hello` carries the token of someone still listed as a member under a different peer (their end broke before the host noticed), the host closes the old peer and removes its `Remote` first.
- Client side: on `Lost(HOST)` (or `Failed` after it has been connected once, or `SILENT = 5 s` with no snapshot mid-race) it enters `Retry { left: REDIAL_FOR = 30 s, wait: REDIAL_EVERY = 2 s, racing }`, goes back to the room screen, and calls `Link::redial()` every 2 s until `Welcome` arrives or 30 s have passed. Then it leaves with "The host has gone". To a host, `Lost` is treated the same as `Left`.
- Tests: `a_player_who_comes_back_has_what_they_had`, `a_name_is_not_who_someone_is`, `someone_back_before_they_were_missed_has_their_own_place`, `a_player_whose_way_to_the_host_breaks_dials_again`, `a_host_that_is_never_reached_again_is_given_up`, `a_race_gone_silent_is_a_host_lost`.

**For zombies:** the mechanism carries over directly and matches the map's decision. `Absent` grows from `points` to the Player's Points, Weapons (with ammo and Pack-a-Punch state), Perks and seat. The difference is *when* they come back: Block Racers puts them back in the race straight away if it's still on, while zombies brings them back **at the start of the next Round** (map decision). So the host holds a returning Player as "waiting" until the next Round begins. `REDIAL_FOR = 30 s` should be revisited: a zombies Run outlives a race, so the host may want to remember absent Players for the whole Run, which `ABSENT = 32` already allows.

## Bevy 0.19 → 0.20

Checked against the [official 0.19 → 0.20 migration guide](https://bevy.org/learn/migration-guides/0-19-to-0-20/) and the `bevy_*-0.20.0` sources in the local cargo registry.

- The APIs the net code uses still exist in 0.20 and zombies already uses most of them: `Time::<Fixed>::from_hz` (`bevy_time-0.20.0/src/fixed.rs`), `RunFixedMainLoopSystems::{BeforeFixedMainLoop, AfterFixedMainLoop}` (zombies' `player.rs`, `zombie.rs`), `TimeUpdateStrategy::ManualDuration` (zombies' headless tests), `Query::get_many_mut` (`bevy_ecs-0.20.0/src/system/query.rs`), `App::register_required_components`, `MessageReader<AppExit>` (`lobby::farewell`), `Has<T>`, `resource_exists`, `not`, `single()`/`single_mut()`.
- The migration guide's ECS breaks don't touch `src/net/`: `iter_many*` now yields `Result` (unused), `NextState::set_if_neq` → `set_if_different` (unused; deprecated alias anyway), observer `On<Add, A>` → `On<Add<A>>` (no observers in `src/net/`), exclusive-system unification (none), weakly ordered built-in render/UI sets (net systems chain themselves explicitly in `FixedUpdate`).
- `transport.rs` calls `bevy::log::{warn, debug}` directly; these are unchanged.
- So the real cost of the port is coupling to Block Racers' game types (`Kart`, `Track`, `Controls`, `menu::*`, `items`, `audio`), not the Bevy version. `link.rs`, `transport.rs` and the codec need no Bevy changes at all.
- Bevy 0.20's default feature profiles (`2d`, `3d`, `ui`, `audio`) don't include `serialize`. Block Racers enables it for serde on `Vec3`/`Quat` in `State`/`Sight`. Zombies' lockfile already has `glam` with serde, but enable `bevy/serialize` explicitly rather than relying on that.

## Lobby (facts for the lobby ticket)

[BR `crates/lobby-api/src/lib.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/crates/lobby-api/src/lib.rs), [BR `src/net/lobby.rs`](https://github.com/jchevertonwynne/blockracers/blob/4c63ae52c1a7739c234ce7897562ec2a1bbc9421/src/net/lobby.rs)

- The lobby API has **no game key**. Sessions are filtered only by `protocol: u32` (`GET /sessions?protocol=N`, `GET /codes/{code}?protocol=N`), and `Register` has `protocol, name, host, endpoint, locked, max, status{players, circuit, racing}, unlisted`. `MAX_PLAYERS = 6`, `MAX_NAME = 32`, `MAX_ENDPOINT = 512`, 5-letter codes, heartbeat every 10 s (`BEAT_EVERY`), and a session is dropped after 30 s without one (`GONE_AFTER`).
- The game side defaults to `https://racers.jchevertonwynne.uk`, overridable by the `BRICK_LOBBY` env var.
- Sharing it needs either a new `game` field (a lobby change), or zombies taking a `protocol` range Block Racers will never reach (a workaround with no lobby change). `status.circuit`/`racing` would be reused as Map name and "Run in progress".

## Open questions this leaves for later tickets

- 64 Hz (zombies today) vs 60 Hz (Block Racers) for `TICKS`.
- Whether replay includes Zombies in the Player's move filter (the `meet_ahead` equivalent) or ignores them.
- Whether hit reports go reliably (likely) or ride in the `Inputs` datagram with `RESENT` redundancy.
- How long the host remembers an absent Player (Block Racers: 30 s redial window on the client, 32 absent entries on the host).
