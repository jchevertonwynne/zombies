//! Research bench for issue #273: what each Bevy 0.20 lighting / post feature costs on the Mac.
//!
//! A synthetic stand-in for a late Round: ~1,100 flat-coloured boxes laid out as rooms and
//! streets, 32 moving "zombies" (4 boxes each), 12 warm lamps, one dim moon with cascaded
//! shadows, and the game's linear `DistanceFog`. The camera walks a loop at eye height.
//!
//! Run one config per process so nothing leaks between them:
//!   cargo run --example lighting_bench -- <config>
//! It warms up (pipelines compile), measures, prints one line, and exits.
//! `scripts` in the research note loop every config.

use std::f32::consts::TAU;

use bevy::{
    anti_alias::{fxaa::Fxaa, smaa::Smaa, taa::TemporalAntiAliasing},
    camera::Hdr,
    core_pipeline::tonemapping::Tonemapping,
    light::{
        atmosphere::ScatteringMedium, Atmosphere, AtmosphereEnvironmentMapLight,
        CascadeShadowConfigBuilder, DirectionalLightShadowMap, FogVolume, PointLightShadowMap,
        VolumetricFog, VolumetricLight,
    },
    pbr::{
        AtmosphereSettings, ContactShadows, DefaultOpaqueRendererMethod, DistanceFog, FogFalloff,
        ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel,
        ScreenSpaceReflections,
    },
    post_process::bloom::Bloom,
    prelude::*,
    render::view::ColorGrading,
    window::{PresentMode, WindowResolution},
};

const WARMUP_FRAMES: u32 = 240;
const MEASURE_FRAMES: u32 = 900;
const LAMPS: usize = 12;

#[derive(Resource, Clone)]
struct Config(String);

impl Config {
    fn has(&self, flag: &str) -> bool {
        self.0.split('+').any(|f| f == flag)
    }
}

#[derive(Resource, Default)]
struct Samples {
    frame: u32,
    ms: Vec<f32>,
}

#[derive(Component)]
struct Walker(f32);

#[derive(Component)]
struct Eye;

fn main() {
    let config = Config(std::env::args().nth(1).unwrap_or_else(|| "base".into()));
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "lighting bench".into(),
            resolution: WindowResolution::new(1920, 1080).with_scale_factor_override(1.0),
            present_mode: PresentMode::Immediate,
            ..default()
        }),
        ..default()
    }))
    .insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.6, 0.65, 0.8),
        brightness: 120.0,
        ..default()
    })
    .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.04)))
    .init_resource::<Samples>();
    if config.has("deferred") || config.has("ssr") {
        app.insert_resource(DefaultOpaqueRendererMethod::deferred());
    }
    if config.has("dir4096") {
        app.insert_resource(DirectionalLightShadowMap { size: 4096 });
    }
    if config.has("pt512") {
        app.insert_resource(PointLightShadowMap { size: 512 });
    }
    app.insert_resource(config)
        .add_systems(Startup, setup)
        .add_systems(Update, (walk, measure))
        .add_systems(Startup, report_device)
        .run();
}

fn rng(seed: &mut u32) -> f32 {
    // xorshift, deterministic so every config sees the same scene.
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32) / (u32::MAX as f32)
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut media: ResMut<Assets<ScatteringMedium>>,
    config: Res<Config>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let palette = [
        Color::srgb(0.55, 0.5, 0.42),
        Color::srgb(0.45, 0.3, 0.25),
        Color::srgb(0.35, 0.37, 0.4),
        Color::srgb(0.3, 0.33, 0.25),
        Color::srgb(0.6, 0.58, 0.5),
        Color::srgb(0.25, 0.22, 0.2),
        Color::srgb(0.5, 0.45, 0.3),
        Color::srgb(0.4, 0.4, 0.42),
    ];
    let mats: Vec<_> = palette
        .iter()
        .map(|&c| {
            materials.add(StandardMaterial {
                base_color: c,
                perceptual_roughness: if config.has("ssr") { 0.2 } else { 0.9 },
                ..default()
            })
        })
        .collect();

    // Ground.
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(mats[2].clone()),
        Transform::from_xyz(0.0, -0.5, 0.0).with_scale(Vec3::new(140.0, 1.0, 140.0)),
    ));

    // Rooms: an 8x8 grid of 12 m cells, each walled with 4 walls broken into segments, plus props.
    let mut seed = 0x1234_5678u32;
    let mut count = 0;
    for gx in -4i32..4 {
        for gz in -4..4 {
            let cx = gx as f32 * 16.0 + 8.0;
            let cz = gz as f32 * 16.0 + 8.0;
            let m = mats[((gx + gz).rem_euclid(8)) as usize].clone();
            for side in 0..4 {
                for seg in 0..4 {
                    if seg == 1 && side % 2 == 0 {
                        continue; // doorway
                    }
                    let along = -4.5 + seg as f32 * 3.0;
                    let (x, z, sx, sz) = match side {
                        0 => (cx + along, cz - 6.0, 3.0, 0.3),
                        1 => (cx + along, cz + 6.0, 3.0, 0.3),
                        2 => (cx - 6.0, cz + along, 0.3, 3.0),
                        _ => (cx + 6.0, cz + along, 0.3, 3.0),
                    };
                    commands.spawn((
                        Mesh3d(cube.clone()),
                        MeshMaterial3d(m.clone()),
                        Transform::from_xyz(x, 1.5, z).with_scale(Vec3::new(sx, 3.0, sz)),
                    ));
                    count += 1;
                }
            }
            for _ in 0..4 {
                let s = Vec3::new(0.4 + rng(&mut seed) * 1.6, 0.3 + rng(&mut seed) * 1.5, 0.4 + rng(&mut seed) * 1.6);
                let p = Vec3::new(cx - 4.0 + rng(&mut seed) * 8.0, s.y / 2.0, cz - 4.0 + rng(&mut seed) * 8.0);
                commands.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(mats[(rng(&mut seed) * 8.0) as usize % 8].clone()),
                    Transform::from_translation(p).with_scale(s),
                ));
                count += 1;
            }
        }
    }
    info!("static boxes: {count}");

    // Zombies: 32 walkers, each 4 boxes.
    let flesh = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.4, 0.3),
        ..default()
    });
    for i in 0..32 {
        commands
            .spawn((
                Walker(i as f32 / 32.0 * TAU),
                Transform::default(),
                Visibility::default(),
            ))
            .with_children(|z| {
                for (y, s) in [
                    (0.45, Vec3::new(0.35, 0.9, 0.3)),
                    (1.25, Vec3::new(0.5, 0.7, 0.3)),
                    (1.75, Vec3::new(0.3, 0.3, 0.3)),
                    (1.3, Vec3::new(0.15, 0.15, 0.7)),
                ] {
                    z.spawn((
                        Mesh3d(cube.clone()),
                        MeshMaterial3d(flesh.clone()),
                        Transform::from_xyz(0.0, y, 0.0).with_scale(s),
                    ));
                }
            });
    }

    // Moon.
    let mut moon = commands.spawn((
        DirectionalLight {
            illuminance: 800.0,
            shadow_maps_enabled: !config.has("noshadow"),
            contact_shadows_enabled: config.has("contact"),
            color: Color::srgb(0.7, 0.75, 1.0),
            ..default()
        },
        Transform::default().looking_at(Vec3::new(-0.4, -1.0, -0.3), Vec3::Y),
    ));
    if config.has("cascade2") {
        moon.insert(
            CascadeShadowConfigBuilder {
                num_cascades: 2,
                maximum_distance: 45.0,
                first_cascade_far_bound: 12.0,
                ..default()
            }
            .build(),
        );
    }
    if config.has("vol") {
        moon.insert(VolumetricLight);
    }

    // Lamps: one in each of 12 rooms near the walk loop.
    let pt_shadows: usize = if config.has("pt12") {
        12
    } else if config.has("pt4") {
        4
    } else {
        0
    };
    for i in 0..LAMPS {
        let a = i as f32 / LAMPS as f32 * TAU;
        if config.has("spot4") && i < 4 {
            // A downward cone instead of an omni lamp: one shadow map, not six.
            let mut lamp = commands.spawn((
                SpotLight {
                    color: Color::srgb(1.0, 0.75, 0.45),
                    intensity: 400_000.0,
                    range: 25.0,
                    outer_angle: 1.2,
                    inner_angle: 0.6,
                    shadow_maps_enabled: true,
                    ..default()
                },
                Transform::from_xyz(a.cos() * 30.0, 2.7, a.sin() * 30.0)
                    .looking_at(Vec3::new(a.cos() * 30.0, 0.0, a.sin() * 30.0 + 0.01), Vec3::Y),
            ));
            if config.has("volpt") {
                lamp.insert(VolumetricLight);
            }
            continue;
        }
        let mut lamp = commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.75, 0.45),
                intensity: 400_000.0,
                range: 25.0,
                shadow_maps_enabled: i < pt_shadows,
                contact_shadows_enabled: config.has("contact"),
                ..default()
            },
            Transform::from_xyz(a.cos() * 30.0, 2.7, a.sin() * 30.0),
        ));
        if config.has("volpt") && i < pt_shadows {
            lamp.insert(VolumetricLight);
        }
    }

    if config.has("vol") {
        commands.spawn((
            FogVolume {
                density_factor: 0.05,
                ..default()
            },
            Transform::from_xyz(0.0, 4.0, 0.0).with_scale(Vec3::new(140.0, 8.0, 140.0)),
        ));
    }

    // Camera.
    let mut cam = commands.spawn((
        Eye,
        Camera3d::default(),
        ColorGrading::default(),
        Transform::from_xyz(0.0, 1.6, 0.0),
    ));
    if !config.has("nofog") {
        cam.insert(DistanceFog {
            color: Color::srgb(0.02, 0.02, 0.04),
            falloff: FogFalloff::Linear {
                start: 10.0,
                end: 45.0,
            },
            ..default()
        });
    }
    let msaa_off = ["msaaoff", "smaa", "fxaa", "taa", "ssao", "ssaolow", "ssr", "deferred"]
        .iter()
        .any(|f| config.has(f));
    if msaa_off {
        cam.insert(Msaa::Off);
    }
    if config.has("smaa") {
        cam.insert(Smaa::default());
    }
    if config.has("fxaa") {
        cam.insert(Fxaa::default());
    }
    if config.has("taa") {
        cam.insert(TemporalAntiAliasing::default());
    }
    if config.has("ssao") {
        cam.insert(ScreenSpaceAmbientOcclusion::default());
    }
    if config.has("ssaolow") {
        cam.insert(ScreenSpaceAmbientOcclusion {
            quality_level: ScreenSpaceAmbientOcclusionQualityLevel::Low,
            ..default()
        });
    }
    if config.has("ssr") {
        cam.insert(ScreenSpaceReflections::default());
    }
    if config.has("contact") {
        cam.insert(ContactShadows::default());
    }
    if config.has("bloom") {
        cam.insert((Hdr, Bloom::NATURAL));
    }
    if config.has("hdr") {
        cam.insert(Hdr);
    }
    if config.has("agx") {
        cam.insert(Tonemapping::AgX);
    }
    if config.has("vol") {
        cam.insert(VolumetricFog {
            ambient_intensity: 0.0,
            step_count: if config.has("vol16") { 16 } else { 64 },
            ..default()
        });
    }
    if config.has("atmo") {
        cam.insert((Hdr, AtmosphereSettings::default(), AtmosphereEnvironmentMapLight::default()));
        let medium = media.add(ScatteringMedium::earth(256, 256));
        commands.spawn(Atmosphere::earth(medium));
    }
}

fn walk(
    time: Res<Time>,
    mut walkers: Query<(&Walker, &mut Transform), Without<Eye>>,
    mut eye: Query<&mut Transform, With<Eye>>,
) {
    let t = time.elapsed_secs();
    for (w, mut tf) in &mut walkers {
        let a = w.0 + t * 0.1;
        let r = 6.0 + (w.0 * 3.0).sin() * 3.0;
        tf.translation = Vec3::new(a.cos() * r, 0.0, a.sin() * r);
        tf.rotation = Quat::from_rotation_y(-a);
    }
    // Eye walks a 24 m circle through the middle of the zombies, looking along its path.
    let a = t * 0.15;
    let pos = Vec3::new(a.cos() * 24.0, 1.6, a.sin() * 24.0);
    let ahead = Vec3::new((a + 0.3).cos() * 24.0, 1.4, (a + 0.3).sin() * 24.0);
    for mut tf in &mut eye {
        *tf = Transform::from_translation(pos).looking_at(ahead, Vec3::Y);
    }
}

fn measure(
    time: Res<Time<Real>>,
    mut samples: ResMut<Samples>,
    config: Res<Config>,
    mut exit: MessageWriter<AppExit>,
) {
    samples.frame += 1;
    if samples.frame <= WARMUP_FRAMES {
        return;
    }
    let ms = time.delta_secs() * 1000.0;
    samples.ms.push(ms);
    if samples.ms.len() as u32 >= MEASURE_FRAMES {
        let mut v = samples.ms.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        let p50 = v[v.len() / 2];
        let p95 = v[v.len() * 95 / 100];
        let p99 = v[v.len() * 99 / 100];
        println!(
            "RESULT {:<28} mean {:6.2} ms  p50 {:6.2}  p95 {:6.2}  p99 {:6.2}  ({:.0} fps)",
            config.0,
            mean,
            p50,
            p95,
            p99,
            1000.0 / mean
        );
        exit.write(AppExit::Success);
    }
}

fn report_device(device: Res<bevy::render::renderer::RenderDevice>) {
    let f = device.features();
    let l = device.limits();
    use bevy::render::settings::WgpuFeatures as F;
    println!(
        "DEVICE texture_binding_array={} non_uniform_indexing={} timestamp_query={} max_texture_array_layers={} max_binding_array_elements={} max_binding_array_samplers={} max_storage_textures_per_stage={}",
        f.contains(F::TEXTURE_BINDING_ARRAY),
        f.contains(F::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING),
        f.contains(F::TIMESTAMP_QUERY),
        l.max_texture_array_layers,
        l.max_binding_array_elements_per_shader_stage,
        l.max_binding_array_sampler_elements_per_shader_stage,
        l.max_storage_textures_per_shader_stage,
    );
}
