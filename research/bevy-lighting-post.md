# Bevy 0.20 lighting and post: what it can do, and what it costs on the Mac

Research for [#273](https://github.com/jchevertonwynne/zombies/issues/273), under map [#272](https://github.com/jchevertonwynne/zombies/issues/272). Which of Bevy 0.20's rendering features could give the Maps a moody WaW/BO1 look, what does each cost on Metal at 1080p, and does any of them clash with what we have?

## Sources

Primary, in trust order:

1. **Bevy 0.20.0 source** at tag [`v0.20.0`](https://github.com/bevyengine/bevy/tree/v0.20.0) (the exact crates in our `Cargo.lock`, read from `~/.cargo/registry`). Doc comments and constants are quoted from these files. Links below go to the tag on GitHub (`crates/<crate>/src/...`).
2. **Bevy 0.20 examples** shipped in the same tag ([`examples/3d`](https://github.com/bevyengine/bevy/tree/v0.20.0/examples/3d)), for how each feature is meant to be set up.
3. **[Bevy 0.20 release notes](https://bevy.org/news/bevy-0-20/)**. The rendering news in 0.20 is Solari (ray tracing), mesh shaders, the WESL shader move and `ctt` texture compression. Nothing changed in shadows, SSAO, SSR, bloom, fog, probes or lightmaps. For those, 0.20 behaves as the source below says.
4. **A measurement on the dev's Mac** (Apple M1 Max, 32-core GPU, Metal), using a synthetic bench committed with this note: [`examples/lighting_bench.rs`](../examples/lighting_bench.rs). Bevy's GPU timestamps don't work on Metal ("Timestamp queries and pipeline statistics are currently supported only on Vulkan and DX12. On other platforms (Metal, WebGPU, WebGL2) only CPU time will be recorded", [`bevy_render/src/diagnostic/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_render/src/diagnostic/mod.rs)). So the costs below are whole-frame times, not per-pass GPU times.

## What we have today

- One `Camera3d` ([`src/players/player/mod.rs`](../src/players/player/mod.rs)) with the defaults: **MSAA 4×** (`Msaa::Sample4` is the default, [`bevy_render/src/view/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_render/src/view/mod.rs)), **TonyMcMapface** tonemapping (the default, same file), no `Hdr`, a neutral `ColorGrading` (the Teleporter grades it to black and white), and a **linear `DistanceFog`** from 10 m to 45 m (`FOG_START`/`FOG_END` in `src/tuning.rs`). A Hellhound Round pulls that fog in by lerping `FogFalloff::Linear` ([`src/enemies/hellhound.rs`](../src/enemies/hellhound.rs) `pull_in_fog`).
- One `DirectionalLight` "moon" at 800 lux with shadows and the **default cascades** (4 cascades, 150 m maximum distance, first bound at 10 m, from `CascadeShadowConfigBuilder::default`, [`bevy_light/src/cascade.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_light/src/cascade.rs)). The shadow map is 2048² ([`directional_light.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_light/src/directional_light.rs)).
- One warm `PointLight` per lamp (400,000 lm, range 25 m), **without shadows**. Muzzle flashes, Power-ups, the Box, Hellhound bolts and napalm add short-lived unshadowed point lights.
- `GlobalAmbientLight` at 120, and `ClearColor` as the sky.
- Every surface is a **scaled unit `Cuboid`** with a flat `StandardMaterial`. Bevy's `Cuboid` mesh has **only `UV_0`, and each face spans the whole 0..1 square** ([`bevy_mesh/src/primitives/dim3/cuboid.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_mesh/src/primitives/dim3/cuboid.rs)). That matters for lightmaps below.

## Feature by feature

Costs are from the bench (see [Measurement](#measurement)): the mean frame time with the feature on, minus the 4.8 ms baseline that mirrors today's setup. The budget is 16.7 ms.

| Feature | How to turn it on (0.20) | Requirements and limits (source) | Measured cost | Mood value |
|---|---|---|---|---|
| **Directional shadows, cascades** | already on | ≤10 directional lights, ≤4 cascades each ([`bevy_pbr/src/render/light.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/render/light.rs) `MAX_DIRECTIONAL_LIGHTS = 10`, `MAX_CASCADES_PER_LIGHT = 4`). | All shadows off saves 0.9 ms. **2 cascades to 45 m saves 0.85 ms with no loss**, because the fog hides everything past 45 m anyway. A 4096² map costs nothing measurable. | Moonlit hard shadows are the backbone of the night look. |
| **Point-light shadows** | `PointLight { shadow_maps_enabled: true }` | Cube map, 6 faces at 1024² by default (`PointLightShadowMap`, [`point_light.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_light/src/point_light.rs)). The cap is `max_texture_array_layers / 6`. The Mac reports 2048 layers, so **341 shadowed point lights**, far more than we could afford. | **The expensive one: about +1.5 ms per shadowed lamp** (4 lamps +5.9 ms, 12 lamps +15.7 ms). Halving the map to 512² changes **nothing**, so the cost is the 6 extra depth passes over every caster in range, not fill rate. | Lamps that cast shadows are WaW's interiors. Use them sparingly. |
| **Spot-light shadows** | `SpotLight { shadow_maps_enabled: true }` | One 2D map each, sharing the array with directional cascades. The cap is `max_texture_array_layers − 4 × shadowed directional lights` ([`light.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/render/light.rs)). | **4 shadowed spots cost +1.0 ms**, against +5.9 ms for 4 shadowed points. | A downward cone under a hanging lamp gives most of the look of a shadowed point light at about a sixth of the cost. |
| **Soft shadows (PCSS)** | `soft_shadows_enabled` / `soft_shadow_size` | Behind the **`experimental_pbr_pcss`** cargo feature, which isn't in our build. The doc warns they are "significantly more expensive" and "rather noisy if not using the temporal mode" ([`point_light.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_light/src/point_light.rs)). | not measured | Low. Gritty and hard suits the style. |
| **Shadow filtering** | `ShadowFilteringMethod` on the camera | `Gaussian` by default. `Temporal` is the CoD: Advanced Warfare method and wants TAA ([`bevy_light/src/lib.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_light/src/lib.rs)). | ~0 | Keep the default. |
| **Contact shadows** | `ContactShadows` on the camera, plus `contact_shadows_enabled` per light | Screen-space raymarch. Needs the depth prepass, which it requires automatically ([`bevy_pbr/src/contact_shadows.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/contact_shadows.rs)). Works forward or deferred. | +1.1 ms with all 13 lights opted in | Grounds props and Zombies' feet. Cheap enough. |
| **SSAO** | `ScreenSpaceAmbientOcclusion` | Needs **`Msaa::Off`** (it errors otherwise), plus depth and normal prepasses (added automatically). Darkens only *indirect* light, not direct. TAA is "strongly recommended" against noise. Quality runs Low 4 spp, Medium 8, High 18, Ultra 54 ([`bevy_pbr/src/ssao/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/ssao/mod.rs)). | **High +0.3 ms, Low +0.25 ms** | High. Crevices and corners in a box-built world read much better. Since it only darkens ambient, keep ambient a meaningful share of the light. |
| **SSR** | `ScreenSpaceReflections` | **Deferred only**: `DefaultOpaqueRendererMethod::deferred()` and `Msaa::Off`. Reflects only what's on screen ([`bevy_pbr/src/ssr/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/ssr/mod.rs)). | Deferred itself ~0 (measured −1.1 to 0 ms in this scene). SSR adds about +0.3 ms. | Low. WaW/BO1 surfaces are rough. Wet floors or puddles at most. Switching the whole renderer to deferred for this isn't worth it now. |
| **MSAA / AA choices** | `Msaa`, `Fxaa`, `Smaa`, `TemporalAntiAliasing` | SSAO and TAA both need `Msaa::Off`. TAA needs motion vectors and has a `reset` flag for camera cuts ([`bevy_anti_alias/src/taa/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_anti_alias/src/taa/mod.rs)). `smaa_luts` is in Bevy's default `3d` features. | MSAA 4× vs off ~0. FXAA ~0. SMAA +0.15 ms. TAA +0.2 ms. | Moving to SSAO means dropping MSAA for SMAA (no ghosting) or TAA (cleaner SSAO and volumetrics). TAA needs `reset = true` on Teleports. |
| **HDR + Bloom** | `Hdr` + `Bloom` (Bloom requires `Hdr`, [`bevy_post_process/src/bloom/settings.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_post_process/src/bloom/settings.rs)) | Glow comes from emissive materials, not from cranking intensity. | HDR alone +0.1 ms. Bloom +1.0 ms. | High for readability: emissive accents on Perks, the Box, Pack-a-Punch and Power-ups get a halo. This is how "readability beats realism" survives darker lighting. |
| **Tonemapping** | `Tonemapping::*` | TonyMcMapface (the default, neutral), AgX (neutral, slightly desaturated), AcesFitted ("intentional and dramatic hue shifting… significantly increased contrast"), BlenderFilmic, KhronosPbrNeutral, Reinhard and others. AgX, TonyMcMapface and BlenderFilmic need `tonemapping_luts`, which is a default feature. `Tonemapping::None` disables `ColorGrading` ([`bevy_render/src/view/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_render/src/view/mod.rs), [`bevy_core_pipeline/src/tonemapping/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_core_pipeline/src/tonemapping/mod.rs)). | AgX vs TonyMcMapface ~0. They're all one LUT or curve. | A free style lever. AcesFitted plus `ColorGrading` (lift the shadows towards green or teal, desaturate) is the cheapest route to WaW's graded look. Pick it by eye in the lighting prototype. |
| **Volumetric fog / god rays** | `VolumetricFog` on the camera, `FogVolume` boxes in the world, `VolumetricLight` on lights | The light needs a shadow map. Point and spot lights also scatter (the shader loops over clustered point and spot lights, [`volumetric_fog.wesl`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/volumetric_fog/volumetric_fog.wesl), and the [`volumetric_fog` example](https://github.com/bevyengine/bevy/blob/v0.20.0/examples/3d/volumetric_fog.rs) toggles `VolumetricLight` on both). `step_count` defaults to 64 ([`bevy_light/src/volumetric.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_light/src/volumetric.rs)). Density textures can scroll ([`scrolling_fog`](https://github.com/bevyengine/bevy/blob/v0.20.0/examples/3d/scrolling_fog.rs)). | **64 steps over the whole map: +9 to +11 ms. 16 steps: +0.9 ms.** Adding 4 volumetric shadowed point lights: about +14 ms in total. 4 volumetric spots: about +10 ms. | Very high for mood: moonbeams through windows, lamp haze. Use **16–24 steps** and `FogVolume`s only where the shafts are wanted, and add `jitter` with TAA. Keep volumetric point lights out. |
| **Atmosphere** | `Atmosphere` entity + `AtmosphereSettings` on the camera (requires `Hdr`), optionally `AtmosphereEnvironmentMapLight` | Hillaire 2020 procedural sky. "Using both [with volumetric fog] at once is untested" ([`bevy_pbr/src/atmosphere/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/atmosphere/mod.rs)). | +0.8 to +1.6 ms with the environment-map light | Low. It's built for daylight and time of day. A night sky is better as a `Skybox` cubemap (a CC0 night HDRI). |
| **Skybox** | `Skybox { image, brightness, rotation }` | A cubemap. It "does not (currently) affect the scene's lighting. To do so, use `EnvironmentMapLight` alongside this component" ([`bevy_light/src/probe.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_light/src/probe.rs)). | not measured (one full-screen pass behind the geometry, trivial) | Medium. It replaces the flat `ClearColor`, but the 45 m fog mostly hides it. Fog colour and sky must agree. |
| **Environment map / IBL** | `EnvironmentMapLight` (prefiltered diffuse and specular KTX2 pair) or `GeneratedEnvironmentMapLight` (filtered on the GPU at runtime from one cubemap) on the camera, or in a `LightProbe` | Light probes are 1×1×1 cubes scaled in the world, with up to **8 per view** (`MAX_VIEW_LIGHT_PROBES = 8`, [`bevy_pbr/src/light_probe/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/light_probe/mod.rs)). Several probes need binding arrays. **The M1 Max has them** (bench `DEVICE` line: `texture_binding_array=true`, `non_uniform_indexing=true`). Without them only the nearest probe counts. 0.20 has probe falloff blending ([`light_probe_blending`](https://github.com/bevyengine/bevy/blob/v0.20.0/examples/3d/light_probe_blending.rs)) and parallax-corrected cubemaps ([`pccm`](https://github.com/bevyengine/bevy/blob/v0.20.0/examples/3d/pccm.rs)). | Probe sampling is a few texture reads per fragment. Not measured separately. The atmosphere run includes one generated environment map. | High for textured PBR. Without IBL, rough PBR textures look flat in the ambient. One probe per Zone with a dim, Zone-tinted cubemap is the cheapest win once textures land. |
| **Irradiance volumes** | `IrradianceVolume` in a `LightProbe` | Baked diffuse GI as an ambient-cube voxel grid in a 3D texture. "Bevy doesn't currently come with a way to [bake]". Blender's Eevee bake plus `export-blender-gi` from [`bevy-baked-gi`](https://github.com/pcwalton/bevy-baked-gi) produce them ([`irradiance_volume.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/light_probe/irradiance_volume.rs)). | Not measured. It's a texture lookup. | Medium. This is the realistic baked route for us (below). |
| **Lightmaps** | `Lightmap { image, uv_rect, bicubic_sampling }` per mesh entity | Needs **`ATTRIBUTE_UV_1`** on the mesh. "Bevy doesn't currently have any way to actually bake lightmaps", so bake in Blender ([The Lightmapper](https://github.com/Naxela/The_Lightmapper)) ([`bevy_pbr/src/lightmap/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/lightmap/mod.rs)). Lights can skip lightmapped diffuse with `affects_lightmapped_mesh_diffuse` to avoid counting it twice ([`point_light.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_light/src/point_light.rs)). | n/a | **Not usable as the Maps are built.** Our boxes are one shared `Cuboid` mesh, scaled per entity, with no `UV_1` and overlapping per-face UVs. A lightmap route means exporting each Map's boxes to Blender, unwrapping, baking and re-importing per-entity lightmaps, all of which goes stale whenever the Map code changes. |
| **Clustered decals** | `ClusteredDecal` | Up to 4 textures each (base, normal, metallic-roughness, emissive). Needs bindless, which the M1 Max has ([`bevy_pbr/src/decal/clustered.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/decal/clustered.rs)). There are also forward decals ([`decal/forward.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/decal/forward.rs)). | not measured | High for grit (blood, grime, posters), but that belongs to the texturing ticket. Noted here only because it shares the clustered light budget. |
| **Auto exposure, DoF, motion blur** | `AutoExposure` (requires `Hdr`), `DepthOfField`, `MotionBlur` (needs motion vectors) | [`bevy_post_process`](https://github.com/bevyengine/bevy/tree/v0.20.0/crates/bevy_post_process/src) | not measured | Low. WaW/BO1 used little of these in play. Skip. |
| **Solari (ray-traced GI)** | `SolariLighting` | New on macOS in 0.20 but "has no built-in denoiser there", and point and spot lights aren't supported ([release notes](https://bevy.org/news/bevy-0-20/)). | not measured | Out: no denoiser on Mac, no point lights, and Windows parity is unclear. |

### Light-count caps, in one place

- Directional lights: 10. Cascades per directional light: 4.
- Point and spot lights: clustered, with no fixed cap on Metal, which uses storage buffers. The 204-light cap (`MAX_UNIFORM_BUFFER_CLUSTERABLE_OBJECTS`) only applies when storage buffers aren't available ([`bevy_pbr/src/cluster/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/cluster/mod.rs)).
- **Shadowed point lights:** `max_texture_array_layers / 6` = **341** on this Mac. **Shadowed spot lights:** `2048 − 4 × (shadowed directional lights)`. Past the cap, lights still shine but drop their shadows. Shadowed lights sort first, ties break by entity ([`light.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/render/light.rs)).
- Light probes: 8 per view. Rect area lights: 8, but only without storage buffers.
- **The real cap is cost, not the hardware.** At about 1.5 ms per shadowed point lamp, and with the frame budget already shared with gameplay, **budget about 4 shadowed lights per view**, and prefer spot lights.

## Fit with the box-built world, avian3d and our fog

- **Box geometry and screen-space effects:** SSAO, contact shadows, SSR and volumetrics all work from the depth (and normal) buffer, so they don't care that everything is a scaled cube. Flat-shaded boxes with hard 90° corners are a good case for SSAO.
- **No UV2:** this rules out lightmaps until the Maps are exported to glTF (see above). Irradiance volumes and reflection probes don't need UVs, only a bounding box per Zone, so they fit procedural Maps. Baking them still means getting the Map into Blender. Hand-authored cubemaps per Zone, or `GeneratedEnvironmentMapLight` from a CC0 HDRI, avoid baking entirely.
- **`DistanceFog`:** this is a per-fragment term in the PBR shader that `StandardMaterial` can opt out of (`fog_enabled`, [`bevy_pbr/src/fog.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/fog.rs)). It is independent of the volumetric post pass, so the two stack. The bench ran them together without warnings. It costs ~0.05 ms. `DistanceFog` also has `directional_light_color` and `directional_light_exponent` for a cheap glow towards the moon.
- **Hellhound fog effect:** it only lerps `DistanceFog.falloff`, so it is unaffected by every feature above. If volumetric fog comes in, the Hellhound Round could also raise `FogVolume.density_factor` for a thicker look. That's a design choice, not a conflict.
- **Teleporter grade:** it uses `ColorGrading` on the camera, which keeps working under every tonemapper except `Tonemapping::None`. A permanent Map-wide grade and the Teleporter's black and white will both write `ColorGrading`, so they need to compose (save and restore, or layer).
- **TAA and Teleports:** a Teleport is a camera cut. Set `TemporalAntiAliasing.reset = true` on that frame or the image smears. Unlit, alpha-blended effects (muzzle flash quads) "don't work well" with TAA because they write no motion vectors ([`taa/mod.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_anti_alias/src/taa/mod.rs)). That's a reason to prefer **SMAA**.
- **The many short-lived point lights** (muzzle flashes ×4 in Co-op, Power-ups, Hellhound bolts, napalm) must stay **unshadowed**. Each shadowed one is about 1.5 ms.
- **avian3d:** no rendering overlap. It moves `Transform`s, and `bevy_transform_interpolation` smooths them, so motion vectors (for TAA or motion blur) come out right. Nothing in this list touches physics.
- **Headless and test paths:** all of these are camera or light components, so a headless app without a camera never builds them. The fallback rule in #272 holds if the camera setup lives behind one "lighting profile" function.

## Measurement

**Bench:** [`examples/lighting_bench.rs`](../examples/lighting_bench.rs). A 140 m square with an 8×8 grid of walled rooms (1,152 static boxes in 8 flat materials), 32 walking "Zombies" of 4 boxes each (Co-op's base Alive Cap of 32), 12 warm lamps (400 klm, 25 m range, as the game uses), the 800-lux moon with shadows, the game's linear `DistanceFog` and ambient, and the camera walking a loop at eye height. The window is 1920×1080 physical (scale factor 1), `PresentMode::Immediate`, in the dev profile (dependencies at opt-level 3, as the game builds). Each config runs in a fresh process: 240 warm-up frames, then the mean of 900. Each config ran twice.

Run it with: `CARGO_BUILD_TARGET=aarch64-apple-darwin cargo run --example lighting_bench -- <config>`, where config flags are joined with `+`, for example `msaaoff+smaa+ssao+bloom`.

**Results:** mean frame ms over two runs. A few runs were pinned at exactly 16.67 ms by macOS display sync (the window wasn't frontmost). Those are dropped and the clean run is kept.

| Config | ms | Δ vs base |
|---|---|---|
| base (today: MSAA4, fog, moon shadows, 12 unshadowed lamps) | 4.8 | — |
| no fog | 4.7 | −0.05 |
| no shadows | 3.7–4.0 | −0.9 |
| moon: 2 cascades to 45 m | 3.9–4.0 | −0.85 |
| moon: 4096² map | 4.7 | 0 |
| MSAA off | 4.7 | 0 |
| FXAA | 4.8 | 0 |
| SMAA | 4.9 | +0.15 |
| TAA | 5.0 | +0.2 |
| 4 shadowed point lamps | 10.7 | **+5.9** |
| 4 shadowed point lamps, 512² | 10.7 | +5.9 |
| 12 shadowed point lamps | 20.4 | **+15.7** |
| 4 shadowed spot lamps | 5.8 | **+1.0** |
| contact shadows (all lights) | 5.8–5.9 | +1.1 |
| SSAO Low | 5.0 | +0.25 |
| SSAO High | 5.1 | +0.3 |
| HDR | 4.9 | +0.1 |
| HDR + Bloom | 5.8 | +1.0 |
| AgX tonemapping | 4.6–4.8 | 0 |
| volumetric fog, 64 steps, whole map | 14.0–16.5 | **+9 to +11** |
| volumetric fog, 16 steps | 5.7 | +0.9 |
| vol 64 + 4 volumetric shadowed points | 18.6–18.8 | +14 |
| vol 64 + 4 volumetric shadowed spots | 14.3–15.0 | +10 |
| deferred renderer | 3.7–4.8 | ~0 |
| deferred + SSR | 4.7–5.1 | +0.3 |
| atmosphere + atmosphere environment light | 5.6–6.4 | +0.8 to +1.6 |
| **"Moody A"**: SMAA, SSAO High, Bloom, vol 16 steps, 4 shadowed spots, 2 cascades | **7.4–8.1** | **+2.6 to +3.3** |
| **"Moody B"**: TAA, SSAO, Bloom, vol 64, 4 volumetric shadowed points, contact | 20.3–22.9 | +15.5 to +18 (misses 60 fps) |

**Caveats:**

- This is a synthetic scene on one machine. The real game also runs AI, physics, the HUD and audio. Bevy pipelines rendering against the main world, so the frame is roughly the longer of simulation and render, not their sum. Treat the deltas as **render costs to budget**, not as fps forecasts.
- The base number may be partly CPU-bound (render-world extraction and draw encoding for ~1,300 meshes). In that case cheap GPU passes hide under it, and the "~0" rows are "under 0.2 ms", not free.
- The point-shadow result (no change from 1024² to 512²) suggests that cost is **per-pass draw submission** over every caster in a lamp's 25 m range, multiplied by 6 faces. Two cheaper fixes follow: shorter `range` on shadowed lamps, and `NotShadowCaster` on small props. Neither was measured.
- Windows wasn't measured. The #272 bar says Windows must be no worse. A Vulkan or DX12 run would also give per-pass GPU timings through `RenderDiagnosticsPlugin`.

## Answer

- **Everything on the ticket's list exists in 0.20 and runs on Metal.** That includes the binding-array features (multiple probes, clustered decals), which the M1 Max supports. Solari is the exception: no denoiser on Mac.
- **Cheap and high-value (about 3 ms in total):** SSAO (with MSAA swapped for SMAA), HDR + Bloom on emissive accents, a tonemapper and `ColorGrading` grade (AcesFitted or AgX, chosen by eye), contact shadows, 2 cascades capped at the fog distance, and a few **shadowed spot lights** in place of shadowed point lights.
- **Expensive and needs a budget:** shadowed *point* lights at about 1.5 ms each. Volumetric fog at the default 64 steps costs about 10 ms. At **16 steps** in targeted `FogVolume`s it costs about 1 ms. Volumetric point lights are out.
- **Baked lighting:** lightmaps don't work with today's UV-less shared cube mesh. Light probes do: per-Zone `EnvironmentMapLight` or `GeneratedEnvironmentMapLight`, and later Blender-baked `IrradianceVolume`s once a Map exists as glTF. They give per-Zone ambient identity without UV2.
- **No conflicts:** `DistanceFog` (including the Hellhound pull-in) stacks with volumetric fog. avian3d is unaffected. Watch two things: TAA needs `reset` on Teleports and dislikes the unlit muzzle quads (another reason to prefer SMAA), and a permanent grade must compose with the Teleporter's `ColorGrading`.
- A concrete profile to try in the lighting prototype is **"Moody A"**, which measured 7.4–8.1 ms against today's 4.8 ms.
