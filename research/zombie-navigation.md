# Zombie navigation options for Bevy 0.20 + avian3d 0.8

Research for ticket `03-zombie-navigation-options` (Zombies v2 map). This file states facts only. The choice is made in "Zombie navigation and Barrier entry".

Checked on 2026-10-25. Version data comes from the crates.io API (`/api/v1/crates/<name>` and `/<version>/dependencies`) and from each repo's default-branch `Cargo.toml` via the GitHub API.

## Baseline

- **Bevy 0.20.0** was released 2026-10-08 (rc.1 on 09-15, rc.2 on 09-28). **avian3d 0.8.0** was released 2026-10-20. Source: crates.io version lists.
- **Current code:** `src/zombie.rs` `chase` runs in `FixedUpdate`. It steers each Zombie straight at the Player's horizontal position and moves it with avian's `MoveAndSlide`, colliding against `World`, `Zombie` and `Player` layers. There is no pathfinding, so a wall between a Zombie and the Player stops it.
- **Cargo semver:** a crate that requires `bevy = "^0.19"` does not accept 0.20. Mixing it into this project pulls in a second Bevy, whose types do not interoperate. Using such a crate today means a git dependency or `[patch]` on an unreleased port, or a fork.

## 1. Navmesh and pathfinding crates

| Crate (latest) | Released | Bevy req | avian req | Bevy 0.20 status |
|---|---|---|---|---|
| `vleue_navigator` 0.16.0 | 2026-08-25 | ^0.19.1 | avian3d ^0.7 (optional) | Dependabot PR [#149](https://github.com/vleue/vleue_navigator/pull/149), opened 2026-10-13, is open with no review. avian 0.8 is not addressed. |
| `bevy_landmass` 0.12.1 (+ `landmass` 0.9.3) | 2026-09-30 / 2026-10-11 | bevy_* ^0.19.0 | none | Draft PR [#195](https://github.com/andriyDev/landmass/pull/195) targets 0.20.0-rc.1. The author says it stays "Draft until rerecast is updated". |
| `bevy_rerecast` 0.5.0 (+ `avian_rerecast` 0.6.0) | 2026-08-05 | ^0.19 | avian3d 0.7 (workspace) | PR [#63](https://github.com/janhohenheim/rerecast/pull/63) targets 0.20.0-rc.2. The maintainer said "I'm not gonna merge this until Bevy 0.20 is released". On 10-24 a commenter asked why it was still unmerged and got no reply. |
| `oxidized_navigation` 0.12.0 | 2024-12-25 | ^0.15 | avian3d ^0.2 | **Archived.** The last commit, on 2025-11-14, is "Deprecation notice". |
| `bevy_pathmesh` 0.6.0 | 2024-03-09 | old | n/a | Superseded by `vleue_navigator` (same author). |
| `bevy_northstar` 0.7.0 | 2026-07-08 | ^0.19 | none | No 0.20 release. It does grid-based hierarchical pathfinding. |
| `bevy_flowfield_tiles_plugin` 0.16.0 | 2026-10-22 | **^0.20** | avian3d ^0.8 (dev/examples only) | **The only Bevy-integrated navigation crate that supports Bevy 0.20 on crates.io.** |
| `pathfinding` 4.16.0 | 2026-09-07 | none (engine-agnostic) | none | Works with any Bevy version. |
| `dodgy_2d` 0.5.5 / `dodgy_3d` 0.5.2 | 2025-04 / 2024-12 | none | none | Engine-agnostic ORCA local avoidance. `landmass` uses `dodgy_2d` internally. |

### vleue_navigator (polyanya-based navmesh)

Sources: [README](https://github.com/vleue/vleue_navigator), [docs.rs prelude](https://docs.rs/vleue_navigator/0.16.0/vleue_navigator/prelude/index.html), `src/updater.rs`, `src/obstacles/avian3d.rs`.

- **Algorithm:** it uses Polyanya for any-angle pathfinding over a navmesh. Meshes can be loaded with `NavMesh::from_bevy_mesh`, or built live from obstacle entities with `NavmeshUpdaterPlugin<Obstacle: ObstacleSource, Marker = Obstacle>`.
- **avian3d integration:** `impl ObstacleSource for Collider` slices each collider's trimesh with the navmesh plane, using `upward_shift`, to produce 2D obstacle outlines. Supported shapes: Cuboid, Ball, Capsule, TriMesh, HeightField, ConvexPolyhedron, Cylinder, Cone, their Round variants, and Compound. Segment, Triangle, Polyline, HalfSpace and Voxels log a warning and are skipped. Sleeping bodies get `CachableObstacle` automatically.
- **Dynamic obstacles (Doors):** `NavMeshUpdateMode` has three modes:
  - `Direct`: rebuild on every change.
  - `Debounced(f32)`: rebuild at most every N seconds.
  - `OnDemand(bool)`: rebuild when set to true.

  A rebuild triggers when an obstacle's transform or component changes, when a `Marker` component is removed, or when settings change. Builds run asynchronously on `AsyncComputeTaskPool` unless `NavMeshUpdateModeBlocking` is set. So despawning or removing a Door's obstacle triggers a rebuild. This is a whole-mesh rebuild; the docs describe no partial or tiled update.
- **3D model:** each navmesh is a 2D mesh positioned by a transform. `NavMeshSettings` has `layer` and `stitches` for multi-layer meshes. Other fields include `agent_radius` (inflates obstacles), `agent_radius_on_outer_edge`, `simplify`, `merge_steps`, `build_timeout` and `ignore_obstacles`.
- **Agents:** none. It returns `Path` / `TransformedPath` only. Steering appears under "To Implement" in the README, and there is no crowd avoidance.
- **Maintenance:** active. Releases have tracked every Bevy version from 0.13 to 0.19, usually within about 2 months of the Bevy release (0.19 came out 06-19 and nav 0.16 on 08-25).

### landmass / bevy_landmass (navigation system: paths + steering + avoidance)

Sources: [repo](https://github.com/andriyDev/landmass), [docs.rs landmass 0.9.3](https://docs.rs/landmass/0.9.3/landmass/), [docs.rs bevy_landmass 0.12.1](https://docs.rs/bevy_landmass/0.12.1/bevy_landmass/).

- **Model:** an `Archipelago` holds `Island`s (one navmesh each), `Agent`s (AI) and `Character`s (non-AI obstacles that agents avoid, such as the Player). `AgentSettings` has `radius`, `desired_speed` and `max_speed`.
- **Moving agents:** landmass does not move agents itself. The game sets `AgentTarget` and keeps `Velocity` in sync, then reads `AgentDesiredVelocity` after `LandmassSystemSet::Output`. That desired velocity fits directly into the existing `MoveAndSlide` call.
- **Local avoidance:** included. The `dodgy_2d` (ORCA) dependency is the only avoidance crate in its dependency list.
- **Navmesh source:** landmass does not generate meshes. You supply vertices and polygons (`NavigationMesh` → `validate()` → `ValidNavigationMesh`). `landmass_rerecast` 0.3.0 (2026-08-09) bridges to `bevy_rerecast` for generation.
- **Other features:**
  - `polygon_type_indices` with per-agent `AgentTypeIndexCostOverrides`, for area costs.
  - `AnimationLink` (off-mesh links that need an action, e.g. a window climb-through), with `UsingAnimationLink`, `PauseAgent` and `ReachedAnimationLink`.
  - Islands can be added at runtime (`add_island`, `IslandMut`).

  The docs.rs pages do not say how to model a Door opening, such as swapping or adding an island or changing a type cost.
- **Maintenance:** active. landmass 0.9.3 came out 2026-10-11. The Bevy 0.20 port waits on rerecast.

### bevy_rerecast (Recast port: navmesh generation)

Source: [readme](https://github.com/janhohenheim/rerecast).

- **What it is:** a Rust port of Recast, the generator used by Unreal, Unity and Godot. It generates navmeshes and has an editor for extracting meshes from a running game. `AvianBackendPlugin` (crate `avian_rerecast`) treats colliders on static rigid bodies as walkable or obstacle geometry.
- **Runtime changes:** `NavmeshGenerator::regenerate` performs a full regeneration and emits `NavmeshReady`. "Partial regeneration" and tile generation are unchecked roadmap items.
- **Scope:** generation only. Pathfinding comes from `vleue_navigator` or `landmass`. `polyanya` 0.17 also depends on `rerecast` ^0.4.

### bevy_flowfield_tiles_plugin (flow fields)

Source: [README](https://github.com/BlondeBurrito/bevy_flowfield_tiles_plugin).

- **Design:** a grid of 10×10 sectors connected by a portal graph. Flow fields are aimed at "crowd and flocking behaviour", with many actors sharing one field. The `3d` feature uses the x-z plane. It is a single grid; the README does not describe multiple floors.
- **Dynamic costs:** cell costs run from 1 to 255, where 255 is impassable. `add_costfield_update_3d()` regenerates the affected sector's and its neighbours' portals and invalidates routes. A Door would be a cost change; the README does not mention doors.
- **Steering:** no actor-to-actor avoidance. Steering is "out of scope", though an example is provided. Each actor size needs its own `FlowFieldTiles`. Builds are async. The README benchmarks 3 ms (open field) to 776 ms (full init) on an i7-12700H.

### Engine-agnostic building blocks

- `pathfinding` 4.16: generic `astar`, `dijkstra`, `bfs` and similar over user-defined successor functions. It has no Bevy dependency.
- `dodgy_2d` / `dodgy_3d`: ORCA avoidance with no Bevy dependency.
- avian3d 0.8 has no navigation. It provides `MoveAndSlide` and spatial queries (ray, shape cast), which the current code already uses.

## 2. Hand-authored waypoint / zone graph

This describes how the approach works, based on the original games' design below and the crates above.

- **Data:**
  - Nodes are placed by hand (in code or a level file) at Barrier exterior goals, Door thresholds, room centres and corridor bends.
  - Edges are tagged with the Door or Barrier that gates them. Opening a Door enables its edges, and a Barrier's edge is usable only once its boards are gone.
  - Zones are volumes, each owning its Spawners and its interior nodes.
- **Runtime:**
  1. Find the node nearest the Zombie and the node nearest the Player, with a visibility check via avian ray or shape cast.
  2. Run A* (for example `pathfinding::astar`) over the enabled edges.
  3. Steer at the next node with the existing `MoveAndSlide`.
  4. Once the Zombie has line of sight to the Player and is in the same Zone, drop to direct chase, as the prototype does now.
- **Costs:**
  - Node placement is manual work for each map, and every layout change needs nodes moved.
  - There is no automatic agent-radius clearance, so nodes must be kept away from walls by hand.
  - Path quality is only as good as node density. Zombies walk node-to-node polylines unless smoothed, for example by skipping to the furthest visible node.
- **Known pitfalls:**
  - **Crowding:** all Zombies converge on the same nodes and doorways. `MoveAndSlide` with Zombie-vs-Zombie collision makes them slide and queue but does not route around one another. ORCA (`dodgy_2d`) or simple separation steering is the usual addition.
  - **Single-file chokepoints:** the original games limit this at windows with three attack spots per Barrier (see below).
  - **Unreachable targets:** the originals fall back to the Player's breadcrumb trail when the engine reports `bad_path`.
- **What it buys:** Door and Barrier gating becomes a boolean on an edge, so there is no mesh rebuild. It has no dependency on a crate's Bevy 0.20 port.

## 3. How WaW/BO1 route Zombies

Primary source: the BO1 (T5) zombie game scripts, raw-file dump at [plutoniummod/t5-scripts](https://github.com/plutoniummod/t5-scripts/tree/70689b8288e60079d48926657e670c41aab4abdf/ZM/Common/maps) (`_zombiemode_spawner.gsc`, `_zombiemode_blockers.gsc`, `_zombiemode_zone_manager.gsc`, `_zombiemode.gsc`). A WaW copy of `_zombiemode_zone_manager.gsc` exists in [SyndiShanX/COD-GSC-Source](https://github.com/SyndiShanX/COD-GSC-Source/blob/4a7b41d2a1e2236fd58219565c68ad24e84326b4/WAW-GSC/maps/_zombiemode_zone_manager.gsc), and BO1 comments cite changes "from DLC3", which is WaW's Der Riese. I did not compare it line by line with the BO1 copy.

**Engine layer.** Actual path search is done by the engine, not the scripts. The scripts call `SetGoalPos(origin)` or `setgoalnode(node)`, wait for the `"goal"` notify, and handle a `"bad_path"` notify when no route exists. The graph is made of map-placed path nodes (`GetNode`), and script brushes link or unlink them with `ConnectPaths()` / `DisconnectPaths()`.

**Two-phase Zombie brain (`zombie_think` → `zombie_goto_entrance` → `find_flesh`):**

1. **Entrance phase.**
   - On spawn the Zombie picks a Barrier. It takes the 3 nearest `exterior_goal` structs (`get_array_of_closest(origin, level.exterior_goals, undefined, 3)`). It drops any whose distance is more than `max_dist` = 500 units beyond the previous one, then picks one at random.
   - It walks there with `goalradius = 128`. `zombie_assure_node` re-targets the next-closest goal if the Zombie has not moved for 1 second.
   - It then runs `tear_into_building`. It claims one of the goal's `attack_spots` with `goalradius = 2` and pulls `barrier_chunks` until all are destroyed.
   - The window's clip then runs `connectpaths()` and the Zombie traverses inside.
   - Exception: spawn structs with `script_noteworthy "find_flesh"` (risers, outdoor spawns) skip teardown and chase at once.
2. **Chase phase (`find_flesh`).**
   - `goalradius = 32`. In a loop, the Zombie picks `get_closest_valid_player` (or a point of interest such as a Monkey Bomb) and runs `zombie_pathing`, which calls `SetGoalPos(favoriteenemy.origin)` every 0.1 s.
   - Distance throttling adds an extra wait between goal updates:

     | Distance from Player | Extra wait |
     |---|---|
     | over 3200 units | 2–3 s |
     | over 2200 units | 1–1.5 s |
     | over 1200 units | 0.5–1 s |

   - The enemy is re-acquired every 1–3 s (`zombie_path_timer`).
   - **Split-up rule:** if more than 12 other Zombies chasing the same player are within 225 units of that player while this Zombie is more than 525 units away, it ignores that player. Single-player clears `ignore_player`, so this rule never applies solo.

**Attack spots.** `blocker_attack_spots()` creates exactly 3 ground positions per Barrier. One is in front of the closest chunk, set back 36 units from it, and the other two sit ±28 units to its right and left. Zombies claim a free spot at random. A full window makes extra Zombies wait or pick another spot.

**Breadcrumb fallback.** Each player drops `zombie_breadcrumbs` when they move 24 units (squared distance 24²), with 3 rings of area crumbs 16 units apart. On `bad_path` the Zombie walks to crumbs in order, with a 20% chance of a random crumb, marking failures as bad. If every crumb fails it goes to the player's `spectator_respawn` origin.

**Doors.**
- A Door's `script_brushmodel` calls `ConnectPaths()` when opened and `DisconnectPaths()` at init.
- Debris uses `junk connectpaths()`.
- Barrier boards `DisconnectPaths()` on repair and `ConnectPaths()` when fully destroyed.
- `flag_blocker` entities connect or disconnect paths when a `script_flag_wait` flag is set.

**Zones (`_zombiemode_zone_manager.gsc`).**
- **Volumes and spawners:** a Zone is one or more `info_volume` sharing a `targetname`. The first volume's `target` names its spawners, `<target>_rise` its riser structs and `<target>_dog` its dog structs.
- **Adjacency:** `add_adjacent_zone(a, b, flag, one_way)` links Zones and is gated by a flag. Door purchases set `script_flag`. Several flags on one link are OR'd, and an array of flags is AND'd.
- **Enabling:** `enable_zone` activates the Zone's `exterior_goal`s that have `script_noteworthy == "<zone>_barriers"`. `deactivate_initial_barrier_goals` turns those off at start so Zombies do not path to windows that lead nowhere reachable.
- **Spawning:** `manage_zones` loops over the Zones. A Zone is active if the player is touching its volume, and its enabled adjacent Zones become active with it. Only active Zones' spawners feed spawning.
- **Effect:** Zones decide where Zombies spawn and which windows they target. Movement itself uses the engine node graph.

## Could not verify

- Whether `vleue_navigator`, `landmass` or `rerecast` will port to **avian3d 0.8** as well as Bevy 0.20, and when. No open PR mentions avian 0.8. Dependabot PR #149 bumps only bevy.
- Whether the rerecast 0.20 PR (#63) is now unblocked: Bevy 0.20 final has shipped, but as of 10-24 the PR had not merged.
- The CI state of the open port PRs. Their GitHub pages did not render check results.
- How `landmass` expects a Door opening to be modelled (island swap vs type cost). The docs.rs overview does not say.
- The name of the engine's path search algorithm in the IW/Treyarch engine. The scripts only show `SetGoalPos` / `bad_path` / `ConnectPaths`, and I found no first-party engine documentation.
- Whether WaW's Nacht der Untoten and Verrückt used the zone manager. The evidence is that it originates with DLC3 (Der Riese) and is used by all BO1 maps.
