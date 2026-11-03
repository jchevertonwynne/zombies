# Co-op runs on a host-authoritative iroh link copied from Block Racers

Co-op is host-authoritative. The host's machine runs the whole simulation: avian3d, Zombies, Rounds, Buyables and every random roll. Each client predicts only its own Player's movement and owns its own Weapon state. Hits are judged by the shooter, with no lag compensation. Everything else is judged by the host.

We chose this over lockstep or a dedicated server because co-op is 2–4 friends with no game server and no port forwarding. Block Racers already proves this shape works over iroh, and trusting the shooter is the honest trade when nobody is competing. Decided in [Grilling: the network layer ADR](https://github.com/jchevertonwynne/zombies/issues/119) on the [Co-op: 2–4 Players online](https://github.com/jchevertonwynne/zombies/issues/112) map.

## Transport

- `link.rs`, `transport.rs`, the postcard codec and the test `Hub` are **copied** from [blockracers](https://github.com/jchevertonwynne/blockracers) `src/net/` into `src/net/`, not shared as a crate. The two repos are on different Bevy versions, and the code is about 800 lines that rarely change.
- iroh, dialled by `EndpointId`, with one reliable stream per Player and QUIC datagrams for the rest. The ALPN is `zombies/1`, raised whenever the wire format breaks. `Hello` carries the protocol version, and the host refuses a mismatch with a reason.
- **Solo has no network path.** Today's game is the host's simulation, and Co-op adds a net plugin to it. Solo adds nothing: no plugin and no loopback.

## Ticks and datagrams

- The simulation stays at **64 Hz** `FixedUpdate`. Inputs go up every tick with the last 4 resent. The host applies them through Block Racers' `Remote` jitter buffer and never rewinds.
- Each input carries yaw and pitch. `move_player` is pulled out into a step function, so a client can replay its unacknowledged inputs through it. The replay keeps Zombies at their shown positions, so body-blocking feels right, without `meet_ahead` offsetting.
- Snapshots go down at **32 Hz**, one per Player, each a **full** state of the fast-changing things: Zombies, Hellhounds, the other Players and each Player's Points. The receiver's own Player goes in full and the rest are packed as in Block Racers. There are no deltas and no interest management. A lost snapshot costs nothing, and the worst case (40 Zombies under Overkill's raised Co-op Alive Cap, 2 Hellhounds, 4 Players, about 900–1000 B) fits under **1000 B**, inside one QUIC datagram. The Co-op Alive Cap is set in [Grilling: Rounds and Hordes by Player count](https://github.com/jchevertonwynne/zombies/issues/121).
- On a client, Zombies and other Players are **puppets**: no AI, no Zombie physics, just interpolation about **2 snapshots** behind.

## The reliable stream

- Everything that changes rarely goes as host events: Doors, Power, Planks, the Mystery Box, Pack-a-Punch, Traps, Links, Power-ups spawning and being picked up, Rounds, Downs, Revives and Respawns. Any change the host makes to a Player's Weapons also goes this way: buys, Max Ammo, Pack-a-Punch, Down, Revive, Respawn.
- Anything that spends Points or touches shared state is a client **request**: Use at this Buyable, Revive this Player, pick up this Power-up. The host checks it, applies it and broadcasts the result. Nothing is predicted, so nothing rolls back.
- **Hit reports** cover hitscan bullets and the Knife only, tagged with the tick. The host trusts them and drops only hits on a Zombie it has already killed. The shooter sees the hit marker and blood at once, but the kill and its Points wait for the host. Lethals, Claymores, launcher rounds and Wonder Weapon projectiles are reported as a **launch**, and the host simulates their flight and splash.
- The host judges all harm to Players, using where each Player is on the host.

## Leaving and coming back

- A dropped Player's seat (Points, Weapons, Perks, Boons) is held for the rest of the Run. They come back at the start of the next Round.
- The client redials on its own for **30 s**, then offers Rejoin until the game is closed. The reconnect token lives only in memory.
- There is no host migration: a host gone for good ends the Run for everyone, and each machine saves its own Run Record.

## Testing

- The copied `Hub` (seeded loss, jitter, stalls, cuts) drives two- to four-App headless lockstep tests on top of the existing headless Player and Zombie harnesses.
- A worst-case snapshot test asserts at most 1000 B.
- The `move_player` step function is tested on its own: replaying the same inputs must reach the same position.

## Considered options

- **A loopback host for solo**: one code path, but solo would pay for netcode it never uses and lose its existing headless tests.
- **A crate shared with Block Racers**: no duplicated code, but every change would have to work on both Bevy versions.
- **Deltas or interest management**: smaller snapshots, but acks and baselines aren't needed when the full state already fits.
- **Victim-judged harm**: mirrors shooter-judged hits, but a lagging client could dodge everything.
