# Texturing the code-built, scaled-cube geometry

Research for [#275](https://github.com/jchevertonwynne/zombies/issues/275), part of map [#272](https://github.com/jchevertonwynne/zombies/issues/272).

Question: in Bevy 0.20, how should tiling PBR textures go on Map geometry that is one unit cube (and one unit cylinder) scaled per piece? Where do trim, skirting, cornices and decals fit? And what is the format pipeline (KTX2 / Basis Universal, Cargo features, mips, conversion tools), and what does a material set cost against the ~300 MB `assets/` budget?

Bevy facts come from the crate sources in `~/.cargo/registry` (`bevy_*-0.20.0`, `wgpu-hal-30.0.1`). They're cited as `crate/path:line`.

## Answer in brief

1. **Bake world-space box UVs into the mesh on the CPU.** Give each textured piece its own `Cuboid` mesh, with UVs rewritten from the piece's world position and size (metres divided by tile size), then call `with_generated_tangents()`. Materials stay plain `StandardMaterial`, one per surface (plaster, brick, oak floor, and so on), so prepass, shadows, SSAO, normal maps and forward decals all work unchanged. Because UVs are anchored to world position, a wall split into pieces and skins tiles across the joins without seams.
2. **Keep a vertex-shader version as the fallback.** It's an `ExtendedMaterial<StandardMaterial, BoxUv>` that keeps the shared unit cube and computes UVs and tangents from world position in a custom `vertex_shader()`. Only build it if per-piece meshes ever cost too much. Full fragment triplanar isn't worth it: every piece is axis-aligned, so one projection per face is exact, and triplanar would mean re-implementing StandardMaterial's sampling of every map.
3. **Trim (skirting, cornice, sills, door frames) uses one trim-sheet material per Zone.** Each trim box's front-face V range is mapped onto its strip of the sheet, and U runs along the length in world metres. These boxes are already separate pieces in `skin()`, so this is a UV choice, not new geometry.
4. **Decals are thin alpha-masked quads or `ForwardDecal`s.** `ClusteredDecal` is turned off on macOS in 0.20.
5. **Formats: offline-compressed KTX2, BC7 for colour and ORM, BC5 for normals, with zstd supercompression and baked mips.** Bevy's default features already load this (`ktx2` + `zstd_rust`), so no Cargo change and no C++ dependency is needed at runtime. Avoid Basis ETC1S (it won't load on Mac or Windows) and UASTC (it needs the C++ `basis-universal` feature).
6. **Budget: about 4.2 MB per full 1K set and about 16.8 MB per 2K set** before zstd. Default to 1K and keep 2K for a few hero floors.
7. **Fallback:** a material whose image handle never loads is never drawn (the piece goes invisible, it doesn't fall back to flat colour). So check the files exist before swapping a palette material for a textured one.

## The geometry today

`src/maps/map/build.rs`:

- `Builder::new` makes one `Cuboid::from_length(1.0)` and one `Cylinder::new(0.5, 1.0)` (lines ~88-89). Every piece is `Mesh3d(cube)` with `Transform { scale: size }` (`shape()`, ~111-121).
- `paint()` caches one `StandardMaterial` per sRGB colour (`perceptual_roughness: 0.9`).
- `piece()` adds an invisible collider, then calls `skin()` for each `Face`. `skin()` draws half the wall's thickness as a slab in the face's Zone colour, plus a **skirting** slab (`SKIRTING` 0.16 m tall, 0.03 m proud) and a **cornice** slab (`CORNICE` 0.18 m by 0.04 m) as separate boxes in `colours.trim`.
- Zone looks come from `palette(zone) -> Palette { floor, wall, trim, light }` (`src/maps/map/mod.rs:217-240`).

So the visual mesh and the collider are already separate. Colliders are `Collider::cuboid(size)` on a bare entity and never depend on the render mesh, so changing the render mesh doesn't touch physics.

## Why default UVs stretch

`CuboidMeshBuilder::build` gives each of the 6 faces UVs from 0 to 1, with flips that vary by face (`bevy_mesh-0.20.0/src/primitives/dim3/cuboid.rs:29-58`). Scaling the entity stretches one texture repeat over the whole face, whether that's a 12 m wall or a 3 cm skirting lip.

`StandardMaterial::uv_transform: Affine2` ("applied to the UVs corresponding to `ATTRIBUTE_UV_0` on the mesh before sampling", `bevy_pbr-0.20.0/src/pbr_material.rs:789-790`) is **per material**. It can't fix this on its own:

- Each box size would need its own material, which breaks the one-material-per-surface batching.
- It applies one scale to all six faces, but a box's faces have three different aspect ratios.

## Options for tiling

### A. CPU-baked world-space box UVs (recommended)

For each textured piece, build the mesh from `Cuboid::from_size(size)` and overwrite `ATTRIBUTE_UV_0`. For each face, project the vertex's **world** position onto the face's two in-plane axes and divide by the material's tile size in metres:

| Face | u | v |
|---|---|---|
| ±X | z | y |
| ±Z | x | y |
| ±Y | x | z |

Flip u on the negative faces so the texture isn't mirrored. Then call `Mesh::with_generated_tangents()`. That's mikktspace, which is on through `3d_api` → `bevy_mikktspace` (`bevy/Cargo.toml` `3d_api`; `bevy_mesh-0.20.0/src/mesh.rs:2171-2188`). Tangents are required for `normal_map_texture` (`pbr_material.rs:372-396`).

- **Pros:**
  - No shaders.
  - `StandardMaterial` is unchanged, so everything downstream works: prepass, normal prepass (SSAO), shadows, `ForwardDecal`, deferred.
  - World-anchored UVs make brick courses run continuously across `piece()` splits at Door and window gaps, and across the two `skin()` halves.
  - The flat-palette path is untouched: just don't swap the mesh or material.
- **Cons:**
  - One mesh asset per piece, where today there's one shared mesh. A Map has hundreds to low thousands of boxes at 24 vertices each, which is trivial memory.
  - Different meshes still batch under one material through Bevy's mesh allocator and GPU preprocessing, but this is worth confirming in the vertical-slice prototype with the frame-time bar from #272.
  - If world-anchoring isn't wanted, cache meshes by size quantised to centimetres (local-anchored UVs, so they're reusable). That gives up the seamless joins.
- **Pieces that move** (Door slabs that slide away, `door_slab()`): bake their UVs at spawn. The texture moves with the slab, which is what you want.

### B. Vertex-shader box projection via `ExtendedMaterial` (fallback)

`MaterialExtension` (`bevy_pbr-0.20.0/src/extended_material.rs:31-80`) can replace `vertex_shader()` while leaving the base StandardMaterial fragment shader in place (`ShaderRef::Default`). The custom vertex shader would:

- choose the dominant axis of the world normal;
- write `uv` from world position divided by tile size, and write a matching `world_tangent`.

That keeps the single shared unit cube (instancing as today), but the unit cube needs tangents generated once so that `VERTEX_TANGENTS` is defined.

Catches:

- `prepass_vertex_shader()` must also be overridden, or the normal prepass (used by SSAO) and alpha-masked prepass will sample with the wrong UVs.
- It's WGSL to maintain across Bevy upgrades.

Only worth building if A's per-piece meshes measurably cost frame time.

### C. Fragment triplanar (not recommended for boxes)

Bevy 0.20 has no built-in triplanar: a grep for "triplanar" across `bevy_pbr`/`bevy_mesh` finds nothing. An extension fragment shader would have to sample base colour, normal and ORM three times each (9 or more fetches instead of 3), blend them, and rebuild the `PbrInput` itself. For axis-aligned boxes the blend weights are always one-hot, so this is paying for B's result at a higher price. Keep it in mind only for non-box geometry (rocks, terrain) if that arrives.

### D. Generated meshes with authored UVs

This is A, generalised. It's the route for anything that isn't a box. The **cylinder** (barrels and drums) also stretches: Bevy's `Cylinder` wraps U from 0 to 1 around the circumference. A per-piece rewrite fixes it: u = angle × r / tile, v = y / tile, and the caps get planar x/z.

## Trim, skirting, cornices

- **Trim sheet:** one texture set holding horizontal strips (skirting profile, cornice moulding, a plain band, a metal strip). Each Zone's palette gains a trim material rather than a `trim: Color`.
- In `skin()` the skirting and cornice are already separate boxes, with their height and depth known (`SKIRTING`, `CORNICE`). Their **front face** gets v mapped to the strip's V range, `[v0, v1]`, and u along the length in world metres. Their top, bottom and end faces map to a plain strip.
- The same applies to `edging()` (sunken-floor lips), window sills, Door frames and steps' nosings.
- One trim material per Zone covers all of these, so the material count stays at about 3-4 per Zone (floor, wall, trim, plus maybe a ceiling).
- The trim material's sampler needs `Repeat` on U only, so strips don't bleed into each other. A sampler with `address_mode_u = Repeat` and `address_mode_v = ClampToEdge` does it. Also leave a few texels of padding between strips, because of mips.

## Decals (grime, blood, posters, scorch)

- **`ClusteredDecal`** is out on the Mac. In `clustered_decals_are_usable` the comment says "Clustered decals are currently disabled on macOS and iOS due to insufficient texture bindings" (`bevy_pbr-0.20.0/src/decal/clustered.rs:570-583`). It also needs the `pbr_clustered_decals` feature.
- **`ForwardDecal`** works anywhere. It's a 1x1 quad that blends into the surface beneath, and "StandardMaterial comes with out of the box support". The catch is that "any camera rendering a forward decal must have the `DepthPrepass` component" (`bevy_pbr-0.20.0/src/decal/forward.rs:50-80`). The game's camera has no prepass today (no `Prepass`/`Msaa` matches in `src/`), so this adds a depth prepass. That's worth having anyway if SSAO comes in with the lighting work.
- **Cheapest:** a thin box or quad with `AlphaMode::Mask` or `Blend`, standing 2-5 mm off the wall, built through the same `Builder`. There's no projection, but on flat axis-aligned walls nothing is lost. Start here, and move to `ForwardDecal` for decals that wrap corners or lie on floors near slopes.

## Format pipeline

### What Bevy 0.20 loads with the project's current features

`Cargo.toml` uses `bevy = { version = "0.20", features = ["serialize"] }` with default features. The default set is `3d` → `3d_bevy_render` → `3d_api`, which includes **`ktx2`** and **`zstd_rust`** (`bevy-0.20.0/Cargo.toml:2725-2745`). So KTX2 files holding **BCn or ASTC blocks, zstd- or zlib-supercompressed**, load today (`bevy_image-0.20.0/src/ktx2.rs:51-100`).

| KTX2 payload | Loads in 0.20 with default features? | Source |
|---|---|---|
| BC1-7 / ASTC, zstd supercompression | Yes | `ktx2.rs:69-82` |
| UASTC (Basis) | Only with the `basis-universal` feature, which pulls in the C++ basis transcoder. UASTC is transcoded to ASTC 4x4 or BC7. | `ktx2.rs:171-240`, `310-370` |
| ETC1S / BasisLZ (Basis) | Effectively **no**. BasisLZ supercompression hits the `_ =>` "Unsupported supercompression scheme" arm. Plain ETC1S is passed through as ETC2 only if the GPU supports ETC2 (`// TODO: Implement transcoding`), and wgpu reports no ETC2 on macOS (`format_eac_etc: os_type != Macos ...`) or on desktop D3D12 and Vulkan. | `ktx2.rs:94-99`, `226-238`; `wgpu-hal-30.0.1/src/metal/adapter.rs:775-779` |

BC support on the dev's Mac: wgpu's Metal backend sets `format_bc` true whenever the OS is macOS (`metal/adapter.rs:775`). Bevy requests every feature the adapter supports (`bevy_render-0.20.0/src/renderer/mod.rs:319-323`). Desktop Windows GPUs all support BC. **So a single BC7/BC5 KTX2 file covers both target platforms.**

`StandardMaterial` recognises two-channel normal maps. A `Bc5RgUnorm` normal texture sets `TWO_COMPONENT_NORMAL_MAP`, and the shader reconstructs Z (`pbr_material.rs:992`, `1171-1180`).

### Mips

Compressed textures must carry their mips in the file: Bevy's loaders don't build mips for KTX2. Bevy 0.20 does have a GPU downsampler (`bevy_core_pipeline-0.20.0/src/mip_generation/mod.rs`, `MipGenerationJobs`), but it's for uncompressed formats it can render to. Every tool below generates mips offline.

### Sampler: tiling and anisotropy

`ImageSamplerDescriptor` defaults to `ClampToEdge` with `anisotropy_clamp: 1` (`bevy_image-0.20.0/src/image.rs:858-893`). Tiling textures need `ImageAddressMode::Repeat`. Floors seen at a glancing angle want `anisotropy_clamp` of 8 or 16. There are two ways to set this:

- per texture, through `asset_server.load_builder().with_settings(|s: &mut ImageLoaderSettings| { s.sampler = ...; })` (the 0.20 load API, `pbr_material.rs:405-411`);
- globally, through `ImagePlugin { default_sampler }` (`image.rs:163-211`).

Normal and ORM maps must be linear. In KTX2 the vkFormat decides that (`*_UNORM` vs `*_SRGB`). `is_srgb` in `ImageLoaderSettings` only applies where the format doesn't say (`image_loader.rs:145-148`). Most CC0 sources ship normals in both GL and DX conventions. Take the GL one, or set `flip_normal_map_y` (`pbr_material.rs:419-421`).

### Conversion tools (offline, committed output)

| Tool | BC7/BC5 KTX2? | Notes |
|---|---|---|
| **`ctt` / `ctt-cli`** (`cargo install ctt-cli`) | **Yes.** BC1-7 and ASTC to KTX2, with `--mipmap` and `--zstd` (levels up to 22). MIT, Apache-2.0 or Zlib. | Needs a C++ compiler to build, because of its ISPC encoders. Suggests BC5 with `--input-color-space linear` for normals. [github.com/cwfitzgerald/ctt](https://github.com/cwfitzgerald/ctt) |
| **Bevy `CompressedImageSaver`** (feature `compressed_image_saver`) | Yes. It uses `ctt` internally: BC7 for RGBA, BC4/BC5 for one or two channels, mips through `generate_mipmaps`, and a normal-map mip filter. | Runs inside Bevy's asset processor. It would mean a processed-assets step or a tiny `tools/` bin. Docs at `bevy_image-0.20.0/src/compressed_image_saver/mod.rs:1-110`, `ctt.rs:93-95`. |
| **KTX-Software `ktx create`** | **No BCn.** It encodes Basis (`--encode basis-lz`, `uastc-ldr-4x4`), ASTC (via `--format ASTC_*`) or uncompressed. Has `--generate-mipmap`, `--zstd 1..22`, `--normal-mode`. Reads PNG and EXR. | [ktx create docs](https://github.khronos.org/KTX-Software/ktxtools/ktx_create.html) |
| **`basisu`** (v2.5, Apache-2.0) | Basis formats only: ETC1S, UASTC, and the new XUASTC/XUBC7, which Bevy 0.20 can't load. ETC1S is about 0.3-3 bpp, UASTC 4x4 is 8 bpp. | [github.com/BinomialLLC/basis_universal](https://github.com/BinomialLLC/basis_universal) |

**Recommendation:** a scripted `ctt-cli` pass (or a `tools/` bin using `CompressedImageSaver`) that turns CC0 source maps into KTX2:

- `*_color.ktx2` as BC7 sRGB;
- `*_normal.ktx2` as BC5 linear;
- `*_orm.ktx2` as BC7 linear, packing occlusion in R, roughness in G and metallic in B, which is the layout StandardMaterial expects (`pbr_material.rs:436-446`);
- all with mips and `--zstd 19` or so.

Poly Haven already ships packed `arm` maps (AO, roughness, metal). ambientCG ships them as separate maps, so they'd be packed in the script, or in Blender, which #272 allows offline. Source PNG and EXR files are **not** committed, only the KTX2 output.

## Size against the 300 MB budget

GPU block formats have fixed rates: BC7 and BC5 are 1 byte per texel, BC4 is 0.5. A full mip chain adds a third. So, before zstd:

| Map | 1K (1024²) | 2K (2048²) |
|---|---|---|
| Colour, BC7 + mips | 1.40 MB | 5.59 MB |
| Normal, BC5 + mips | 1.40 MB | 5.59 MB |
| ORM, BC7 + mips | 1.40 MB | 5.59 MB |
| **Set of 3** | **4.2 MB** | **16.8 MB** |
| Set with roughness as BC4 instead of ORM (no AO or metal) | 3.5 MB | 14.0 MB |

zstd on BC data is lossless and usually trims a modest amount. I couldn't measure it here: installing `ctt-cli` was blocked in this session. **Treat these as upper bounds**, and measure on the first converted set.

For comparison, the source downloads are much larger:

- ambientCG zips (all maps, including displacement and both normal conventions), from `ambientcg.com/api/v2/full_json`:

  | Material | 1K-JPG | 2K-JPG | 2K-PNG |
  |---|---|---|---|
  | Bricks090 | 7.5 MB | 25.3 MB | 65.3 MB |
  | Plaster001 | 7.4 MB | 26.1 MB | 62.0 MB |
  | CorrugatedSteel007A | 5.1 MB | 17.1 MB | 53.5 MB |
  | PavingStones138 | 4.9 MB | 15.3 MB | 33.4 MB |
  | MetalPlates006 | 5.2 MB | 13.1 MB | 41.9 MB |

- Poly Haven `red_brick_03` at 2K (`api.polyhaven.com/files/red_brick_03`): Diffuse PNG 9.1 MB, nor_gl PNG 8.1 MB, arm PNG 5.9 MB, so about 23 MB of PNG for the three maps the game would use.

So converting to BCn KTX2 roughly halves even the JPG zips and gives GPU-ready data. Committing source PNGs would blow the budget within about five materials.

**Budget arithmetic:**

- If textures get about 150 MB of the 300 MB (models, audio and the sky take the rest), that's about **35 sets at 1K**, or about **9 at 2K**.
- At a 2 m tile, 1K gives 512 texels per metre, which is plenty for walls at 1080p. 2K is for the hero floors the Player stares at.
- Three Maps with about 6-10 Zones each, at 3-4 materials per Zone plus shared trim and decals, fits at 1K with sharing (one brick, one plaster, and so on, reused across Maps).

Licences: ambientCG ([docs.ambientcg.com/license](https://docs.ambientcg.com/license/)) and Poly Haven ([polyhaven.com/license](https://polyhaven.com/license)) both publish their materials as CC0, which meets #272's CC0-only rule.

## Fallback with no asset files

The `AsBindGroup` derive returns `AsBindGroupError::RetryNextUpdate` when a texture handle has no loaded `GpuImage` (`bevy_render_macros-0.20.0/src/as_bind_group.rs:587, 634, 742`). A StandardMaterial pointing at a missing or failed file is therefore **never prepared, and the mesh is never drawn**. It doesn't fall back to `base_color`.

So `Builder` should decide per surface material before spawning:

- at startup (or in the Map loading state), check that `assets/materials/<name>/*.ktx2` exist, or wait for `LoadState::Loaded` and treat `Failed` as "use the palette";
- if the files are there, use the textured material plus the UV-baked mesh; otherwise use today's `paint(colour)` and the shared unit cube.

Headless and test apps use `MinimalPlugins + AssetPlugin` with no renderer (for example `src/players/player/headless.rs:64`), so they never touch image files. Keep the textured path behind the same check so tests stay asset-free.

## Open items for the prototype (#278 and the vertical slice)

- Measure the frame-time effect of per-piece meshes (option A) against the shared cube on the worst-case scene.
- Measure real zstd savings on BC7/BC5 output from `ctt`.
- Decide tile sizes and texel density per material family. A good starting point is 2 m for walls and 1-2 m for floors.
- Decide whether a depth prepass (for `ForwardDecal`, and SSAO) fits the 60 fps bar.
