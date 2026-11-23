# Maps are data files, built from Footprints and hand-drawn Walls

Every Map, ours and Players' alike, is a **Map File**: a folder holding `map.ron` and an `assets/` folder, loaded when a Run starts. The Shipped Maps live in `assets/maps/`, a Player's own in `~/.zombies/maps/<map-id>/`, and Maps received in Co-op in a cache at `~/.zombies/cache/<hash>`. A Map File holds everything about its Map: geometry, Zones, windows, Spawn Points, Props, lamps, where each system's machines stand (Perk Machines, Pack-a-Punch, the Power Switch, Traps, Equipment walls, Teleporters, Ziplines, Jump Pads, Generators), its Look, its ambient sound, and which of the built-in mechanics it uses (Darkness, the Tide, Shifting, Sections, Sapper timing, the Quarantine) with their numbers and placed parts. So `MapId` stops being an enum, no system branches on which Map it is, and the Shipped Maps get no special treatment. There is no scripting; a mechanic is code we wrote, turned on and set up by the Map File, and stays decided by the Host.

This supersedes the part of [ADR 0005](0005-each-map-has-its-own-part-of-the-world.md) that made every Map's Zones variants of the one `ZoneId` enum and kept every window in one `WINDOWS` table. A Map File names its Zones, and the game numbers them in file order: Zones, windows and Barriers are a Map's own numbers in memory and on the wire (a `u8` for Zones, so at most 255 per Map). Both machines in Co-op play the same Map File, checked by hash, so the numbers agree. [ADR 0009](0009-every-map-is-built-at-the-world-origin.md) stands.

## Geometry

- A **Footprint** is a mesh of triangles with shared vertices laid out on the ground plane, each vertex with its own height. Zones and their floors, Infills, Raised Floors, Sunken areas, Pools and ramps (which replace `Slope`) are all Footprints, so no room need be a box; a box is two triangles, and porting the Shipped Maps changes nothing. A Zone's volume is its Footprint pushed straight up to its top; `zone_at` is a point-in-triangle test behind a coarse grid. An indoor Zone gets a ceiling from its Footprint at its top. A floor triangle's slope is capped at what the movement step climbs ([ADR 0004](0004-zombies-climb-only-by-steps-and-ramps.md)).
- A **Wall** is drawn by hand: a straight segment with its own base height at each end, a height, a thickness, a material and a Look. Walls are not made from Zones' edges: Zones say where you are, Walls what stops you.
- Windows, Doors and **Openings** (archways) are placed with one click on a Wall. The game cuts the Wall, builds the frame, works out the Zones on each side, and for a window its facing and a Riser outside it. Wall Weapons and Equipment walls go on a Wall the same way.
- The path-node graph is still a graph ([ADR 0001](0001-hand-authored-navigation-graph.md) stands), but it is generated: a grid of nodes in each Zone, edges kept only where the walk clears Walls, Props and Infills, gated by Doors and windows, one-way where the floor drops by more than a step. `Join` is gone: Zones touching with no Wall between them, or through an Opening, are joined. Any node or edge can be pinned, added or removed by hand, and the edit survives regeneration.
- Custom models get a box, a convex hull or no collider, never an arbitrary mesh collider.

## Identity, Forks and Records

A Map is known by its **Map id** (a UUID made when it is created) and its **creator** (the Player's Name at that moment, written into the file and never read from Settings again). Editing a Map that isn't yours makes a **Fork**: the creator becomes your Name, and the parent is added to the Map's **Lineage**, the ordered list of the Maps it came from, so every earlier creator stays credited. Records are kept per (Map id, creator), each Run Record noting the hash it was played on. Only the Shipped Maps' hashes earn Scrip and Commendations; Runs on Custom Maps keep their own Records.

`map.ron` carries a `format` number. The game migrates any older format on load, one step at a time, and refuses a newer one with "update the game". Each bump has a test loading a frozen sample of the format before it.

## Custom assets and Co-op

A Map may carry its own glTF/GLB models, PNG/KTX2 textures and OGG sounds, never code, within 64 MB, a triangle cap per model and 2048² per texture. `map.ron` names each by content hash. The Room and `Hello` carry the Map's hash; the Host sends `map.ron` with `Start`, as Block Racers sends its map, the client asks only for the hashes it lacks, and the Host sends them in chunks over the reliable stream while the Room shows each Player's progress. Everything received is cached by hash. A Map Server may come later.

## The Map Editor

The Map Editor is a mode of the game, opened from the main menu, built with `bevy_egui`. It is used on one machine: the Co-op parity invariant applies to the Maps it makes, which play in Co-op as they do solo, not to the editor itself. Saving is always allowed; validation errors block playing and sharing, warnings (including a performance budget) don't, and the same checks run when any Map loads.

We chose data over Rust because Players make Maps from day one, and a Map they make must load in a game they didn't build. The cost is a large port: about 30 modules branch on `MapId` today and `ZoneId` has about 1,800 uses in 46 files. Decided in [Map editor](https://github.com/jchevertonwynne/zombies/issues/530).
