# PROTOTYPE: material sets for the slice's Zones (#288)

Throwaway. Never merged. Code: `src/tex_prototype.rs` (a rewrite of #278's).

## Approved sets

| Zone | pick | floor | walls (and exterior faces, for the Yard) |
|---|---|---|---|
| Start Room | ruined (#278) | `dark_wooden_planks` 2.0 m | `damaged_plaster` 1.85 m |
| West Hall | 1 tiled | `floor_tiles_06` 3.0 m | `worn_mossy_plasterwall` 1.8 m |
| Dining Room | 1 peeling | `diagonal_parquet` 2.36 m | `peeling_painted_wall` 1.8 m |
| Passage | 1 brick | `worn_brick_floor` 2.0 m | `brick_wall_001` 3.0 m |
| Workshop | 1 rusty-tin | `concrete_floor_02` 2.0 m | `rusty_corrugated_iron` 2.0 m |
| Yard | 3 cobbled | `mossy_cobblestone` 2.0 m | `plastered_stone_wall` 2.5 m |

All Poly Haven, CC0, at 1K, tiled at each texture's real-world size (from the Poly Haven API's
`dimensions`). Trim stays flat paint in each Zone's current palette trim colour.

## Sheets

- One per view, flat plus three candidates: `west-hall-room.jpg`, `west-hall-wall.jpg`,
  `dining-room-*.jpg`, `passage-*.jpg`, `workshop-*.jpg`, `yard-room.jpg`, `yard-house.jpg`.
- `route.jpg`: the approved picks through each Door (Doors hidden for the shot).

## Run it

Textures aren't committed (215 MB of 8-bit PNG). Fetch each id's `Diffuse`, `nor_gl` and `arm` at
1K PNG from `https://api.polyhaven.com/files/<id>` into `assets/prototype_textures/<id>_{diff,nor,arm}.png`,
as 8-bit RGBA (Poly Haven ships some as 16-bit, which the on-load mip maker skips).

```
ZOMBIES_LOOK=dusk ZOMBIES_ZONESETS=west_hall=1,dining_room=1,passage=1,workshop=1,yard=3 cargo run --release
```

`.` / `,` cycle the candidate for the Zone you stand in. `ZOMBIES_TEX_TOUR=<dir>` shoots every
candidate; add `ZOMBIES_TEX_ROUTE=1` for the route shots. Use a scratch `HOME` (with `RUSTUP_HOME`
and `CARGO_HOME` pointed back at the real ones).

## Rejected candidates

- West Hall: panelled (`herringbone_parquet` + `dark_paneled_wood`), bare (`floor_tiles_08` + `worn_plaster_wall`)
- Dining Room: terracotta (`terracotta_floor_tiles` + `rough_plaster_broken`), panelled (`rectangular_parquet` + `wooden_panels`)
- Passage: whitewash (`concrete_floor_worn_001` + `whitewashed_brick`), service (`dirty_tiles` + `painted_worn_brick`)
- Workshop: brick-works (`dirty_concrete` + `factory_brick`), grey-tin (`hangar_concrete_floor` + `corrugated_iron_02`)
- Yard: garden (`forrest_ground_01` + `red_brick_03`), muddy (`brown_mud_leaves_01` + `brick_wall_02`)
