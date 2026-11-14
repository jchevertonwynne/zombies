# PROTOTYPE: ceilings over the Greybox at dusk (#302)

Throwaway. Never merged. Built on `prototype/greybox-zone-materials` (the approved Zone sets and the dusk look).
Code: `src/ceiling_prototype.rs`, plus small hooks in `src/look_prototype.rs` (the lamps and ambient follow the roof).

## Variants

| roof | what it is |
|---|---|
| `open` | today: open to the sky at dusk |
| `dark` | a ceiling slab on every Zone but the Yard and the Balcony (at the top of the walls, textured as the Zone's wall set); lamps off |
| `lamps` | the same ceilings; every lamp a downward spot (1.5 Mlm, cone 1.45 rad) with a glowing bulb, the 4 nearest the eye shadowed |

Under a ceiling, the roofed variants turn the global ambient down (450 → 120) and the distance fog dark,
while the eye is in a roofed Zone. That stands in for per-Zone light probes (#273): right for the room you
stand in, wrong for a sunlit Yard seen through a Door.

## Sheets (left to right: open, dark, lamps)

`start-room-row.jpg`, `west-hall-row.jpg`, `dining-room-row.jpg`, `passage-row.jpg`, `workshop-row.jpg`, `yard-row.jpg`.

## Bench (`ZOMBIES_BENCH=greybox`, slice textured, dusk). NOT TRUSTED

Another session's `cargo test` was using ~7 cores during these runs, so they break the idle-GPU rule.

| run | mean ms | p99 ms |
|---|---|---|
| open #1 | 7.73 | 22.03 |
| lamps #1 | 8.36 | 19.70 |
| dark | 6.05 | 8.70 |
| lamps #2 | 7.40 | 8.95 |
| open #2 | 16.77 | 30.37 |

## Run it

Fetch the textures as in `prototype/zone-sets/README.md`, then:

```
ZOMBIES_LOOK=dusk ZOMBIES_ZONESETS=west_hall=1,dining_room=1,passage=1,workshop=1,yard=3 ZOMBIES_ROOF=lamps cargo run --release
```

`=` / `-` cycle the roof in game. `ZOMBIES_ROOF_TOUR=<dir>` shoots every roof at six viewpoints and quits.
`ZOMBIES_BENCH=greybox` benches whichever `ZOMBIES_ROOF` names. Use a scratch `HOME`.
