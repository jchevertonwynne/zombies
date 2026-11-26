//! The Refinery's sky and what stands past its wall (#565, from #510): the flare stack and its
//! flame, the smoke plume it lights from below, the flare light (the Map's key light), the red
//! aviation lamp on its tip, and the dark silhouettes of the plant beyond the blast wall with a
//! second, distant flare. All of it is built from constants on every machine, so a client sees
//! the same sky; the flames and the plume move on deterministic hashes of their index and the
//! clock, which no gameplay depends on, so nothing is on the wire. None of it has a collider, and
//! with no sprites on disk the cards are left out and the rest still builds.
//!
//! The flame, plume and aviation materials set `fog_enabled: false`, so the brown fog (18 to
//! 130 m) never hides them from any Zone.

use std::f32::consts::TAU;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::maps::darkness::lamp_flicker::position_hash;
use crate::maps::map::textures::on_disk;
use crate::state::InRun;
use crate::weapons::blast::tint;
use crate::weapons::explosive::BlastKind;
use crate::weapons::muzzle::sprite_file;

/// The flare stack's axis in the Flare Yard (base x -43..-37, z 79..85), and how it is built.
pub const STACK_AT: Vec2 = Vec2::new(-40.0, 82.0);
pub const STACK_RADIUS: f32 = 0.8;
pub const STACK_TOP: f32 = 38.0;
/// The dark tip, 2 m on top of the stack.
pub const TIP_TOP: f32 = 40.0;
/// The ring platforms' heights.
pub const RINGS: [f32; 2] = [14.0, 28.0];
const RING_RADIUS: f32 = 1.8;

/// The flare light: a point at y 41 over the stack.
pub const FLARE_LIGHT_AT: Vec3 = Vec3::new(STACK_AT.x, 41.0, STACK_AT.y);
pub const FLARE_COLOUR: Color = Color::srgb(1.0, 0.58, 0.25);
pub const FLARE_LUMENS: f32 = 30_000_000.0;
pub const FLARE_RANGE: f32 = 140.0;

/// How many billboard flames the stack has, and how tall they stand (m).
pub const FLAMES: u32 = 4;
pub const FLAME_HEIGHT: (f32, f32) = (6.0, 9.0);
/// The flame's swell, in Hz.
const SWELL_HZ: f32 = 0.7;

/// The plume's cards, how long each takes to rise and grow, and how big it gets (m).
pub const PLUME_CARDS: u32 = 6;
pub const PLUME_SECS: f32 = 14.0;
pub const PLUME_SIZE: (f32, f32) = (12.0, 30.0);
/// How fast the plume drifts downwind (+x) and rises, in m/s.
const PLUME_DRIFT: f32 = 3.5;
const PLUME_RISE: f32 = 1.4;
/// What a plume card is tinted at birth (lit orange from below) and when it is old and grey.
const PLUME_BIRTH: Color = Color::srgba(0.35, 0.15, 0.05, 0.85);
const PLUME_OLD: Color = Color::srgba(0.06, 0.045, 0.035, 0.7);

/// The aviation lamp on the flare's tip: where it sits, and how it blinks (1 Hz).
pub const AVIATION_AT: Vec3 = Vec3::new(STACK_AT.x, TIP_TOP - 0.6, STACK_AT.y - 0.6);
pub const AVIATION_HZ: f32 = 1.0;

/// The second flare, far off to the north-east, and how tall it is.
pub const DISTANT_FLARE_AT: Vec3 = Vec3::new(110.0, 0.0, 150.0);
pub const DISTANT_FLARE_HEIGHT: f32 = 35.0;
/// Its flames' seeds start here, so they don't swell in step with the near flare's.
const DISTANT_SEED: u32 = 100;

/// What kind of dark shape a silhouette is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// A tank: a cylinder `size.x` across and `size.y` tall.
    Tank,
    /// A column: a tall, narrow cylinder.
    Column,
    /// A pipe-rack bent: two posts, a beam and a pipe run, `size.x` across, `size.y` tall and `size.z` along the run.
    Bent,
    /// The jack-knifed tanker: a cab and a lying cylinder, in a footprint `size.x` by `size.z`.
    Tanker,
}

/// Plant standing past the blast wall, drawn in dark unlit shapes (and in fog): `at` is the middle of its base.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Silhouette {
    pub shape: Shape,
    pub at: Vec3,
    pub size: Vec3,
    pub yaw: f32,
}

const fn sil(shape: Shape, x: f32, z: f32, size: (f32, f32, f32), yaw: f32) -> Silhouette {
    Silhouette { shape, at: Vec3::new(x, 0.0, z), size: Vec3::new(size.0, size.1, size.2), yaw }
}

/// Five tanks, two columns, three rack bents (the rack carrying on north past the wall) and the jack-knifed tanker on the south road.
pub const SILHOUETTES: [Silhouette; 11] = [
    sil(Shape::Tank, -90.0, 20.0, (12.0, 10.0, 12.0), 0.0),
    sil(Shape::Tank, -97.0, 48.0, (12.0, 10.0, 12.0), 0.0),
    sil(Shape::Tank, -88.0, 78.0, (12.0, 10.0, 12.0), 0.0),
    sil(Shape::Tank, 92.0, 30.0, (12.0, 10.0, 12.0), 0.0),
    sil(Shape::Tank, 100.0, 64.0, (12.0, 10.0, 12.0), 0.0),
    sil(Shape::Column, -122.0, 60.0, (6.0, 30.0, 6.0), 0.0),
    sil(Shape::Column, 126.0, 14.0, (6.0, 30.0, 6.0), 0.0),
    sil(Shape::Bent, 0.0, 100.0, (10.0, 6.0, 6.0), 0.0),
    sil(Shape::Bent, 0.0, 106.0, (10.0, 6.0, 6.0), 0.0),
    sil(Shape::Bent, 0.0, 112.0, (10.0, 6.0, 6.0), 0.0),
    sil(Shape::Tanker, 4.0, -22.0, (14.0, 3.2, 12.0), 0.9),
];

impl Silhouette {
    /// How far its footprint reaches from `at` on the ground, at most.
    pub fn reach(&self) -> f32 {
        Vec2::new(self.size.x, self.size.z).length() / 2.0
    }

    /// Whether its whole footprint lies outside the rectangle `plan`.
    pub fn outside(&self, plan: (Vec2, Vec2)) -> bool {
        let r = self.reach();
        self.at.x + r <= plan.0.x || self.at.x - r >= plan.1.x || self.at.z + r <= plan.0.y || self.at.z - r >= plan.1.y
    }
}

/// The dark an unlit silhouette is drawn in.
const SILHOUETTE: Color = Color::srgb(0.014, 0.011, 0.01);

/// A number in [0, 1) from `seed` and `k`, the same on every machine.
pub fn unit(seed: u32, k: u32) -> f32 {
    (position_hash(Vec3::new(seed as f32, k as f32, 3.5)) >> 8) as f32 / (1u32 << 24) as f32
}

/// How far the flame `seed` has swelled at `now` seconds, 0 to 1: a 0.7 Hz swell with gusts on top. A pure function of its two arguments.
pub fn flame_swell(seed: u32, now: f32) -> f32 {
    let phase = unit(seed, 0) * TAU;
    let gust_phase = unit(seed, 1) * TAU;
    let swell = (now * SWELL_HZ * TAU + phase).sin();
    let gust = (now * 0.31 * TAU + gust_phase).sin() * (now * 1.9 * TAU + phase).sin();
    (0.5 + 0.32 * swell + 0.2 * gust).clamp(0.0, 1.0)
}

/// A flame card's height (6 to 9 m) at `now`: a base of its own plus the swell.
pub fn flame_height(seed: u32, now: f32) -> f32 {
    let s = 0.3 * unit(seed, 2) + 0.7 * flame_swell(seed, now);
    FLAME_HEIGHT.0 + (FLAME_HEIGHT.1 - FLAME_HEIGHT.0) * s
}

/// Where a flame card's middle is over its `base`, leaning downwind (+x) as it grows.
pub fn flame_centre(base: Vec3, seed: u32, now: f32) -> Vec3 {
    let height = flame_height(seed, now);
    let side = (unit(seed, 3) - 0.5) * 0.8;
    base + Vec3::new(0.3 * height, 0.5 * height, side)
}

/// The mean swell of the near flare's cards, 0 to 1: what the flare light follows.
pub fn flare_swell(now: f32) -> f32 {
    (0..FLAMES).map(|seed| flame_swell(seed, now)).sum::<f32>() / FLAMES as f32
}

/// The flare light's strength at `now`: full lumens at the flame's brightest, 15% down at its lowest.
pub fn flare_intensity(now: f32) -> f32 {
    FLARE_LUMENS * (0.85 + 0.15 * flare_swell(now))
}

/// Where plume card `index` is at `now`: its offset from the flare tip, its size and its colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlumePose {
    pub offset: Vec3,
    pub size: f32,
    pub colour: Color,
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Plume card `index`: the cards are spread evenly through one 14 s life, so the plume is always a column of six, the oldest the biggest,
/// greyest and farthest downwind.
pub fn plume_pose(index: u32, now: f32) -> PlumePose {
    let age = (now + index as f32 * PLUME_SECS / PLUME_CARDS as f32).rem_euclid(PLUME_SECS);
    let u = age / PLUME_SECS;
    let size = PLUME_SIZE.0 + (PLUME_SIZE.1 - PLUME_SIZE.0) * u;
    let side = (unit(index, 4) - 0.5) * 6.0 * u;
    let offset = Vec3::new(PLUME_DRIFT * age, size * 0.4 + PLUME_RISE * age, side);
    let (from, to) = (LinearRgba::from(PLUME_BIRTH), LinearRgba::from(PLUME_OLD));
    let mut colour = from.mix(&to, u);
    colour.alpha *= smoothstep(0.0, 0.1, u) * (1.0 - smoothstep(0.85, 1.0, u));
    PlumePose { offset, size, colour: colour.into() }
}

/// Whether an aviation lamp at `at` is lit at `now`: on for half of each second, its phase from a hash of where it stands.
pub fn aviation_lit(at: Vec3, now: f32) -> bool {
    let phase = (position_hash(at) >> 8) as f32 / (1u32 << 24) as f32;
    (now * AVIATION_HZ + phase).rem_euclid(1.0) < 0.5
}

/// A flame card: `seed` picks its swell, `base` is the foot of the flame.
#[derive(Component, Debug, Clone, Copy)]
pub struct FlameCard {
    pub seed: u32,
    pub base: Vec3,
}

/// A card of the plume.
#[derive(Component, Debug, Clone, Copy)]
pub struct PlumeCard {
    pub index: u32,
    pub base: Vec3,
}

/// The flare light, the Map's key light: never shadowed.
#[derive(Component)]
pub struct FlareLight;

/// A red aviation lamp, emissive only.
#[derive(Component)]
pub struct Aviation;

/// The Kenney sprites the flame and the plume are drawn from, as the blast uses them.
#[derive(Clone, Default)]
pub struct Sprites {
    pub fire: [Handle<Image>; 2],
    pub smoke: [Handle<Image>; 2],
}

const FIRE_SPRITES: [&str; 2] = ["fire_01", "fire_02"];
const SMOKE_SPRITES: [&str; 2] = ["smoke_03", "smoke_04"];

/// The sprites, if an asset server exists and all four are on disk; otherwise the cards are left out.
pub fn load_sprites(asset_server: Option<&AssetServer>) -> Option<Sprites> {
    let asset_server = asset_server?;
    if !FIRE_SPRITES.iter().chain(&SMOKE_SPRITES).all(|name| on_disk(&sprite_file(name))) {
        return None;
    }
    let load = |name: &str| asset_server.load(sprite_file(name));
    Some(Sprites { fire: FIRE_SPRITES.map(load), smoke: SMOKE_SPRITES.map(load) })
}

/// A flame's material: the blast's fire tint, additive, unlit and unfogged.
pub fn flame_material(image: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color: tint(BlastKind::Plain).fire,
        base_color_texture: Some(image),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        double_sided: true,
        cull_mode: None,
        fog_enabled: false,
        ..default()
    }
}

/// A plume card's material: smoke, blended, unlit and unfogged. Its colour is set every frame.
pub fn plume_material(image: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color: PLUME_BIRTH,
        base_color_texture: Some(image),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        fog_enabled: false,
        ..default()
    }
}

/// The red aviation lamp's material: emissive, unfogged.
pub fn aviation_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(1.0, 0.05, 0.03),
        emissive: LinearRgba::rgb(10.0, 0.25, 0.12),
        fog_enabled: false,
        ..default()
    }
}

/// The flare light: a point, unshadowed (a point light's shadow would be a cube map over 140 m, and
/// the flare stands in a corner, so its shadows would be right nowhere but one line).
pub fn flare_light() -> PointLight {
    PointLight { color: FLARE_COLOUR, intensity: FLARE_LUMENS, range: FLARE_RANGE, shadow_maps_enabled: false, ..default() }
}

/// Builds the sky: the flare stack, its flame, plume, light and aviation lamp, the silhouettes and the second flare.
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, sprites: Option<Sprites>) {
    let cylinder = meshes.add(Cylinder::new(0.5, 1.0));
    let cube = meshes.add(Cuboid::from_length(1.0));
    let quad = meshes.add(Rectangle::new(1.0, 1.0));

    // The stack: a rusty cylinder to y 38, a ring platform at y 14 and y 28, and a dark tip to y 40.
    let rust = materials.add(StandardMaterial { base_color: Color::srgb(0.24, 0.15, 0.11), perceptual_roughness: 0.9, ..default() });
    let trim = materials.add(StandardMaterial { base_color: Color::srgb(0.2, 0.21, 0.23), perceptual_roughness: 0.8, ..default() });
    let dark = materials.add(StandardMaterial { base_color: Color::srgb(0.03, 0.028, 0.03), perceptual_roughness: 0.9, ..default() });
    let part = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, size: Vec3, at: Vec3| {
        (InRun, Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform { translation: at, scale: size, ..default() })
    };
    commands.spawn(part(&cylinder, &rust, Vec3::new(STACK_RADIUS * 2.0, STACK_TOP, STACK_RADIUS * 2.0), Vec3::new(STACK_AT.x, STACK_TOP / 2.0, STACK_AT.y)));
    for height in RINGS {
        commands.spawn(part(&cylinder, &trim, Vec3::new(RING_RADIUS * 2.0, 0.15, RING_RADIUS * 2.0), Vec3::new(STACK_AT.x, height, STACK_AT.y)));
    }
    commands.spawn(part(&cylinder, &dark, Vec3::new(STACK_RADIUS * 1.4, TIP_TOP - STACK_TOP, STACK_RADIUS * 1.4), Vec3::new(STACK_AT.x, (STACK_TOP + TIP_TOP) / 2.0, STACK_AT.y)));

    // The flare light: never shadowed, flickering with the flame.
    commands.spawn((InRun, FlareLight, flare_light(), Transform::from_translation(FLARE_LIGHT_AT)));

    // The aviation lamp: emissive only.
    let aviation = materials.add(aviation_material());
    commands.spawn((
        InRun,
        Aviation,
        Mesh3d(meshes.add(Sphere::new(0.25).mesh().ico(1).expect("a small sphere"))),
        MeshMaterial3d(aviation),
        Transform::from_translation(AVIATION_AT),
        NotShadowCaster,
    ));

    // The silhouettes, and a dark ground for them to stand on.
    let unlit = materials.add(StandardMaterial { base_color: SILHOUETTE, unlit: true, ..default() });
    commands.spawn((
        InRun,
        Mesh3d(quad.clone()),
        MeshMaterial3d(unlit.clone()),
        Transform::from_xyz(0.0, -0.05, 45.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(700.0, 700.0, 1.0)),
        NotShadowCaster,
    ));
    let dark_part = |mesh: &Handle<Mesh>, size: Vec3, at: Vec3, rotation: Quat| {
        (InRun, Mesh3d(mesh.clone()), MeshMaterial3d(unlit.clone()), Transform { translation: at, rotation, scale: size }, NotShadowCaster)
    };
    for s in SILHOUETTES {
        let yaw = Quat::from_rotation_y(s.yaw);
        match s.shape {
            Shape::Tank | Shape::Column => {
                commands.spawn(dark_part(&cylinder, s.size, s.at + Vec3::Y * s.size.y / 2.0, Quat::IDENTITY));
            }
            Shape::Bent => {
                let post = |dx: f32| dark_part(&cube, Vec3::new(0.5, s.size.y, 0.5), s.at + Vec3::new(dx, s.size.y / 2.0, 0.0), Quat::IDENTITY);
                commands.spawn(post(-s.size.x / 2.0 + 0.5));
                commands.spawn(post(s.size.x / 2.0 - 0.5));
                commands.spawn(dark_part(&cube, Vec3::new(s.size.x, 0.5, 0.5), s.at + Vec3::Y * (s.size.y - 0.25), Quat::IDENTITY));
                commands.spawn(dark_part(&cube, Vec3::new(0.6, 0.6, s.size.z), s.at + Vec3::Y * (s.size.y - 1.2), Quat::IDENTITY));
            }
            Shape::Tanker => {
                // The trailer lies along the yaw; the cab has swung round to one side of it.
                let along = yaw * Vec3::Z;
                commands.spawn(dark_part(&cylinder, Vec3::new(2.6, 9.0, 2.6), s.at + Vec3::Y * 1.9 + along * 0.5, yaw * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)));
                let cab_yaw = Quat::from_rotation_y(s.yaw + 1.1);
                commands.spawn(dark_part(&cube, Vec3::new(2.4, 3.0, 2.8), s.at + Vec3::Y * 1.5 + along * 5.6 + cab_yaw * Vec3::X * 2.0, cab_yaw));
            }
        }
    }
    // The second flare: a thin dark stack, 2 unfogged flames and no light.
    commands.spawn(dark_part(&cylinder, Vec3::new(1.2, DISTANT_FLARE_HEIGHT, 1.2), DISTANT_FLARE_AT + Vec3::Y * DISTANT_FLARE_HEIGHT / 2.0, Quat::IDENTITY));

    // The cards, if the sprites are there.
    let Some(sprites) = sprites else { return };
    let fire: Vec<_> = sprites.fire.iter().map(|image| materials.add(flame_material(image.clone()))).collect();
    let flame = |seed: u32, base: Vec3| {
        let height = flame_height(seed, 0.0);
        (
            InRun,
            FlameCard { seed, base },
            Mesh3d(quad.clone()),
            MeshMaterial3d(fire[seed as usize % fire.len()].clone()),
            Transform::from_translation(flame_centre(base, seed, 0.0)).with_scale(Vec3::new(height * 0.55, height, 1.0)),
            NotShadowCaster,
        )
    };
    let foot = Vec3::new(STACK_AT.x, TIP_TOP + 0.2, STACK_AT.y);
    for seed in 0..FLAMES {
        commands.spawn(flame(seed, foot));
    }
    let far_foot = DISTANT_FLARE_AT + Vec3::Y * (DISTANT_FLARE_HEIGHT + 0.2);
    for seed in DISTANT_SEED..DISTANT_SEED + 2 {
        commands.spawn(flame(seed, far_foot));
    }
    let tip = Vec3::new(STACK_AT.x, TIP_TOP + 1.0, STACK_AT.y);
    for index in 0..PLUME_CARDS {
        let pose = plume_pose(index, 0.0);
        commands.spawn((
            InRun,
            PlumeCard { index, base: tip },
            Mesh3d(quad.clone()),
            MeshMaterial3d(materials.add(plume_material(sprites.smoke[index as usize % 2].clone()))),
            Transform::from_translation(tip + pose.offset).with_scale(Vec3::new(pose.size, pose.size, 1.0)),
            NotShadowCaster,
        ));
    }
}

/// Turns `at` to face the eye, upright, or fully when `upright` is false.
fn facing(at: Vec3, eye: Vec3, upright: bool) -> Option<Quat> {
    let mut toward = eye - at;
    if upright {
        toward.y = 0.0;
    }
    (toward.length_squared() > 1e-6).then(|| Transform::IDENTITY.looking_to(-toward, Vec3::Y).rotation)
}

/// The flames swell and lean, upright to the eye.
pub fn sway_flames(time: Res<Time>, cameras: Query<&GlobalTransform, With<Camera3d>>, mut cards: Query<(&FlameCard, &mut Transform)>) {
    let now = time.elapsed_secs();
    let eye = cameras.iter().next().map(GlobalTransform::translation);
    for (card, mut at) in &mut cards {
        let height = flame_height(card.seed, now);
        at.translation = flame_centre(card.base, card.seed, now);
        at.scale = Vec3::new(height * 0.55, height, 1.0);
        if let Some(rotation) = eye.and_then(|eye| facing(at.translation, eye, true)) {
            at.rotation = rotation;
        }
    }
}

/// The plume rises, spreads, darkens and fades, facing the eye.
pub fn drift_plume(
    time: Res<Time>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cards: Query<(&PlumeCard, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    let now = time.elapsed_secs();
    let eye = cameras.iter().next().map(GlobalTransform::translation);
    for (card, mut at, material) in &mut cards {
        let pose = plume_pose(card.index, now);
        at.translation = card.base + pose.offset;
        at.scale = Vec3::new(pose.size, pose.size, 1.0);
        if let Some(rotation) = eye.and_then(|eye| facing(at.translation, eye, false)) {
            at.rotation = rotation;
        }
        if let Some(mut paint) = materials.get_mut(&material.0) {
            paint.base_color = pose.colour;
        }
    }
}

/// The flare light flickers with the flame, and the aviation lamp blinks.
pub fn flicker_flare(time: Res<Time>, mut lights: Query<&mut PointLight, With<FlareLight>>, mut lamps: Query<(&mut Visibility, &Transform), With<Aviation>>) {
    let now = time.elapsed_secs();
    for mut light in &mut lights {
        light.intensity = flare_intensity(now);
    }
    for (mut visibility, at) in &mut lamps {
        let want = if aviation_lit(at.translation, now) { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != want {
            *visibility = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::MapId;
    use bevy::ecs::system::RunSystemOnce;

    fn world_with_sky(sprites: Option<Sprites>) -> World {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world
            .run_system_once(move |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>| {
                spawn(&mut commands, &mut meshes, &mut materials, sprites.clone());
            })
            .unwrap();
        world
    }

    #[test]
    fn the_flare_light_is_never_shadowed() {
        assert!(!flare_light().shadow_maps_enabled);
        let mut world = world_with_sky(None);
        let mut lights = world.query_filtered::<(&PointLight, Has<crate::maps::darkness::Lamp>), With<FlareLight>>();
        let all: Vec<_> = lights.iter(&world).collect();
        assert_eq!(all.len(), 1);
        for (light, is_lamp) in all {
            assert!(!light.shadow_maps_enabled);
            // Not a `Lamp`, so `shadow_nearest_lamps` never turns shadows on for it.
            assert!(!is_lamp);
            assert_eq!((light.intensity, light.range, light.color), (FLARE_LUMENS, FLARE_RANGE, FLARE_COLOUR));
        }
        // It flickers between 85% and full, and never casts shadows.
        for step in 0..200 {
            let k = flare_intensity(step as f32 * 0.37);
            assert!(k <= FLARE_LUMENS && k >= FLARE_LUMENS * 0.85);
        }
        // And it's the only light the sky adds.
        assert_eq!(world.query::<&PointLight>().iter(&world).count(), 1);
        assert_eq!(world.query::<&SpotLight>().iter(&world).count(), 0);
        assert_eq!(FLARE_LIGHT_AT.y, 41.0);
    }

    #[test]
    fn flare_and_aviation_materials_are_unfogged() {
        let image = Handle::<Image>::default();
        assert!(!flame_material(image.clone()).fog_enabled);
        assert!(!plume_material(image.clone()).fog_enabled);
        assert!(!aviation_material().fog_enabled);
        // Every material the sky puts on a flame, plume card or aviation lamp, near flare and distant one alike.
        let mut world = world_with_sky(Some(Sprites { fire: [image.clone(), image.clone()], smoke: [image.clone(), image] }));
        let mut cards = world.query_filtered::<&MeshMaterial3d<StandardMaterial>, Or<(With<FlameCard>, With<PlumeCard>, With<Aviation>)>>();
        let handles: Vec<_> = cards.iter(&world).map(|material| material.0.clone()).collect();
        assert_eq!(handles.len(), (FLAMES + 2 + PLUME_CARDS + 1) as usize);
        let materials = world.resource::<Assets<StandardMaterial>>();
        for handle in handles {
            assert!(!materials.get(&handle).unwrap().fog_enabled);
        }
    }

    #[test]
    fn silhouettes_have_no_colliders_and_stand_outside_the_plan() {
        let plan = MapId::Refinery.layout().plan;
        assert_eq!(SILHOUETTES.iter().filter(|s| s.shape == Shape::Tank).count(), 5);
        assert_eq!(SILHOUETTES.iter().filter(|s| s.shape == Shape::Column).count(), 2);
        assert_eq!(SILHOUETTES.iter().filter(|s| s.shape == Shape::Bent).count(), 3);
        assert_eq!(SILHOUETTES.iter().filter(|s| s.shape == Shape::Tanker).count(), 1);
        for s in SILHOUETTES {
            assert!(s.outside(plan), "{s:?} stands over the plan {plan:?}");
        }
        // The second flare stands beyond it too.
        assert!(DISTANT_FLARE_AT.x > plan.1.x && DISTANT_FLARE_AT.z > plan.1.y);
        assert_eq!(DISTANT_FLARE_AT, Vec3::new(110.0, 0.0, 150.0));
        assert_eq!(DISTANT_FLARE_HEIGHT, 35.0);
        // Nothing the sky spawns is solid.
        let mut world = world_with_sky(Some(Sprites::default()));
        assert_eq!(world.query::<&avian3d::prelude::Collider>().iter(&world).count(), 0);
        assert_eq!(world.query::<&avian3d::prelude::RigidBody>().iter(&world).count(), 0);
    }

    #[test]
    fn the_flare_stack_stands_in_the_flare_yard_and_in_bounds() {
        let layout = MapId::Refinery.layout();
        let (lo, hi) = crate::maps::zone::ZoneId::FlareYard.volume();
        assert!(STACK_AT.x - RING_RADIUS >= lo.x && STACK_AT.x + RING_RADIUS <= hi.x);
        assert!(STACK_AT.y - RING_RADIUS >= lo.z && STACK_AT.y + RING_RADIUS <= hi.z);
        assert_eq!((STACK_AT, STACK_TOP, TIP_TOP, RINGS), (Vec2::new(-40.0, 82.0), 38.0, 40.0, [14.0, 28.0]));
        // Its base, its light and its lamp are inside the bounds the Map plays in, so nothing of the stack's own is Out of Bounds.
        for at in [Vec3::new(STACK_AT.x, 0.0, STACK_AT.y), FLARE_LIGHT_AT, AVIATION_AT] {
            assert!(at.x > layout.plan.0.x && at.x < layout.plan.1.x && at.z > layout.plan.0.y && at.z < layout.plan.1.y);
            assert!(!layout.out_of_bounds(crate::maps::water::TideLevel::Low, Vec3::new(at.x, at.y.min(1.0), at.z)));
        }
    }

    #[test]
    fn the_flare_swell_is_the_same_on_every_machine() {
        // A pure function of the card's seed and the clock: no random number, no per-machine state.
        for seed in 0..FLAMES {
            for step in 0..60 {
                let now = step as f32 * 0.173;
                assert_eq!(flame_swell(seed, now), flame_swell(seed, now));
                assert!((0.0..=1.0).contains(&flame_swell(seed, now)));
                let height = flame_height(seed, now);
                assert!((FLAME_HEIGHT.0..=FLAME_HEIGHT.1).contains(&height), "{height}");
                // It leans downwind.
                let base = Vec3::new(STACK_AT.x, TIP_TOP, STACK_AT.y);
                assert!(flame_centre(base, seed, now).x > base.x);
            }
        }
        // Pinned, so a change to the hash or the swell (which would put two machines' skies out of step) fails here.
        let hashes: Vec<u32> = (0..4).map(|k| position_hash(Vec3::new(0.0, k as f32, 3.5))).collect();
        assert_eq!(hashes, PINNED_HASHES);
        let swells: Vec<f32> = (0..4).map(|seed| (flame_swell(seed, 1.0) * 1000.0).round() / 1000.0).collect();
        assert_eq!(swells, PINNED_SWELLS);
        assert!(flame_swell(0, 0.0) != flame_swell(0, 0.5));
        // The plume is the same too: a 14 s loop, six cards in step.
        for index in 0..PLUME_CARDS {
            assert_eq!(plume_pose(index, 3.0), plume_pose(index, 3.0));
            let (a, b) = (plume_pose(index, 2.0), plume_pose(index, 2.0 + PLUME_SECS));
            assert!((a.offset - b.offset).length() < 1e-2 && (a.size - b.size).abs() < 1e-3);
        }
        // The lamp blinks at 1 Hz: lit for half of each second.
        let lit = (0..100).filter(|&step| aviation_lit(AVIATION_AT, step as f32 * 0.01)).count();
        assert_eq!(lit, 50);
    }

    const PINNED_HASHES: [u32; 4] = [449192359, 1642823180, 2519169580, 4235487039];
    const PINNED_SWELLS: [f32; 4] = [0.193, 0.102, 0.296, 0.219];

    #[test]
    fn the_plume_grows_from_12_to_30_metres_and_goes_from_orange_to_smoke() {
        let (young, old) = (plume_pose(0, 1.5), plume_pose(0, PLUME_SECS - 1.5));
        assert!(young.size < 15.0 && old.size > 27.0);
        assert!(old.offset.x > young.offset.x, "it drifts +x");
        let (young, old) = (young.colour.to_srgba(), old.colour.to_srgba());
        assert!(young.red > young.blue * 3.0, "lit orange at birth");
        assert!(old.red < young.red, "grey smoke when old");
        // A card is invisible as it is reborn, so it doesn't pop.
        assert!(plume_pose(0, 0.0).colour.to_srgba().alpha < 0.01);
        assert!(plume_pose(0, PLUME_SECS - 0.001).colour.to_srgba().alpha < 0.01);
    }

    #[test]
    fn the_sky_builds_with_no_asset_files() {
        // No sprites: the stack, the light, the lamp and the silhouettes still build; no card does.
        let mut world = world_with_sky(None);
        assert_eq!(world.query::<&FlameCard>().iter(&world).count(), 0);
        assert_eq!(world.query::<&PlumeCard>().iter(&world).count(), 0);
        assert_eq!(world.query::<&FlareLight>().iter(&world).count(), 1);
        assert_eq!(world.query::<&Aviation>().iter(&world).count(), 1);
        assert!(load_sprites(None).is_none());
        // With them: 4 flame cards on the stack, 2 on the far flare and 6 of plume.
        let mut world = world_with_sky(Some(Sprites::default()));
        assert_eq!(world.query::<&FlameCard>().iter(&world).count(), 6);
        assert_eq!(world.query::<&PlumeCard>().iter(&world).count(), 6);
    }
}
