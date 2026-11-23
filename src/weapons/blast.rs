//! What a blast looks like: a textured fireball with a second, smaller flame, a smoke puff that
//! lingers, sparks and debris thrown out, and a scorch on the ground under it. All of it grows
//! with the blast's radius and is cosmetic. The Host's `explode` and a Co-op client's `hear` (from
//! the `Burst` message: a position, a radius and a kind) both call `spawn`, so every machine draws
//! the same blast and nothing is added to the snapshot.
//!
//! The sprites are Kenney CC0 (`assets/sprites/<name>/color.ktx2`). Until they have loaded, or
//! with the files missing (and in headless runs, which never load), a blast is the bare fireball
//! sphere it always was. Meshes and materials are shared, the live particles are capped, and
//! scorches are capped and the oldest goes first.
//!
//! A blast has a `BlastKind`, which tints its fire, smoke, sparks and scorch: the same meshes and
//! sprites, but one small set of materials per kind (`Tint`). The kind rides the `Burst` message
//! to a client, so it draws the tint the Host does.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::layers::GameLayer;
use crate::maps::map::textures::on_disk;
use crate::state::InRun;
use crate::tuning::{GAUNTLET_ELITE_TINTS, GRAVITY};
use crate::weapons::weapon::UPGRADED_PURPLE;
use crate::weapons::ammo_mod::AmmoMod;
use crate::weapons::explosive::{BlastKind, ExplosiveAssets, fireball};
use crate::weapons::muzzle::sprite_file;

/// The most blast particles (cards, sparks and debris) alive at once.
pub const BLAST_PARTICLE_CAP: usize = 120;
/// The most scorches alive at once.
pub const SCORCH_CAP: usize = 12;
const SCORCH_SECS: f32 = 20.0;
const FIRE_SECS: f32 = 0.35;
const SMOKE_SECS: f32 = 1.6;
const SPARK_SECS: f32 = 0.5;
const DEBRIS_SECS: f32 = 0.9;

const FIRE_SPRITES: [&str; 2] = ["fire_01", "fire_02"];
const SMOKE_SPRITES: [&str; 2] = ["smoke_02", "smoke_03"];
const SPARK_SPRITE: &str = "spark_01";
const SCORCH_SPRITE: &str = "scorch_01";

/// How a particle's size runs over its life, `t` from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// Swells fast, then shrinks away: a fireball.
    Swell,
    /// Billows out, holds, then thins at the end: smoke.
    Billow,
    /// Shrinks from the start: sparks and debris.
    Shrink,
}

/// The fraction of its full size a particle of `shape` has at `t`.
pub fn envelope(shape: Shape, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match shape {
        Shape::Swell => (t * std::f32::consts::PI).sin().max(0.05),
        Shape::Billow => {
            let grow = 1.0 - (1.0 - (t / 0.4).min(1.0)).powi(2);
            let thin = if t > 0.7 { 1.0 - (t - 0.7) / 0.3 } else { 1.0 };
            (0.3 + 0.7 * grow) * thin.max(0.0)
        }
        Shape::Shrink => (1.0 - t) * (1.0 - t),
    }
}

/// How many of `wanted` new particles fit with `alive` out there.
pub fn allowed(alive: usize, wanted: u32) -> u32 {
    (BLAST_PARTICLE_CAP.saturating_sub(alive) as u32).min(wanted)
}

/// How many sparks and pieces of debris a blast of `radius` throws, as `(sparks, debris)`.
pub fn bits_for(radius: f32) -> (u32, u32) {
    let r = radius.clamp(0.5, 8.0);
    ((4.0 + r * 2.0).round() as u32, (3.0 + r).round() as u32)
}

/// Which of the oldest scorches (given as ages, in any order) to remove so that one more fits.
pub fn scorches_to_clear(ages: &[f32]) -> usize {
    (ages.len() + 1).saturating_sub(SCORCH_CAP)
}

struct Sprite {
    image: Handle<Image>,
}

/// The four colours a blast kind is drawn in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    pub fire: Color,
    pub smoke: Color,
    pub spark: Color,
    pub scorch: Color,
}

/// The tint of a blast of `kind`. A plain blast is the look #497 shipped; every other kind takes
/// one base colour (a glint, a muzzle flash or an Elite tint) and turns it into the same four
/// parts: fire and sparks bright and additive, smoke a dim wash of it over grey, scorch near black.
pub fn tint(kind: BlastKind) -> Tint {
    let base = match kind {
        BlastKind::Plain => {
            return Tint {
                fire: Color::linear_rgb(2.4, 1.0, 0.3),
                smoke: Color::srgba(0.22, 0.21, 0.2, 0.85),
                spark: Color::linear_rgb(3.0, 1.8, 0.5),
                scorch: Color::srgba(0.01, 0.01, 0.01, 0.9),
            };
        }
        BlastKind::Upgraded => UPGRADED_PURPLE,
        // The Ray Gun's muzzle flash.
        BlastKind::RayGun => Color::srgb(0.4, 1.0, 0.5),
        // The Elite's own tint, as on its body and its fuse ring.
        BlastKind::Volatile => {
            let [r, g, b] = GAUNTLET_ELITE_TINTS[2];
            Color::srgb(r, g, b)
        }
        // Redder than its orange glint, which a Volatile's blast already wears.
        BlastKind::BlastFurnace => Color::srgb(1.0, 0.12, 0.03),
        BlastKind::DeadWire => AmmoMod::DeadWire.glint(),
        BlastKind::Fireworks => AmmoMod::Fireworks.glint(),
        BlastKind::ThunderWall => AmmoMod::ThunderWall.glint(),
        BlastKind::Turned => AmmoMod::Turned.glint(),
    };
    let LinearRgba { red, green, blue, .. } = base.to_linear();
    let scaled = |k: f32| Color::linear_rgb(red * k, green * k, blue * k);
    Tint {
        fire: scaled(2.4),
        smoke: Color::linear_rgba(0.04 + red * 0.1, 0.04 + green * 0.1, 0.04 + blue * 0.1, 0.85),
        spark: scaled(3.0),
        scorch: Color::linear_rgba(0.002 + red * 0.01, 0.002 + green * 0.01, 0.002 + blue * 0.01, 0.9),
    }
}

/// One kind's materials; the sprites and meshes under them are shared by every kind.
struct KindLook {
    fire: Vec<Handle<StandardMaterial>>,
    smoke: Vec<Handle<StandardMaterial>>,
    spark: Handle<StandardMaterial>,
    scorch: Handle<StandardMaterial>,
}

/// What a blast is made of, shared by every blast.
pub(crate) struct BlastLook {
    quad: Handle<Mesh>,
    cube: Handle<Mesh>,
    sprites: Vec<Sprite>,
    /// A set of materials per `BlastKind`, in `BlastKind::ALL` order.
    kinds: Vec<KindLook>,
    debris: Handle<StandardMaterial>,
    ready: bool,
}

impl BlastLook {
    /// The look, if its sprites are on disk and an asset server exists to load them.
    pub(crate) fn new(
        asset_server: Option<&AssetServer>,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Option<Self> {
        let asset_server = asset_server?;
        let names: Vec<&str> = FIRE_SPRITES.iter().chain(&SMOKE_SPRITES).chain(&[SPARK_SPRITE, SCORCH_SPRITE]).copied().collect();
        if !names.iter().all(|name| on_disk(&sprite_file(name))) {
            debug!("blast sprites missing, a blast is a bare sphere");
            return None;
        }
        let mut sprites = Vec::new();
        let mut load = |name: &str| {
            let image: Handle<Image> = asset_server.load(sprite_file(name));
            sprites.push(Sprite { image: image.clone() });
            image
        };
        let mut sprite_material = |image: Handle<Image>, colour: Color, alpha_mode: AlphaMode| {
            materials.add(StandardMaterial {
                base_color: colour,
                base_color_texture: Some(image),
                unlit: true,
                alpha_mode,
                double_sided: true,
                cull_mode: None,
                ..default()
            })
        };
        // Each sprite loads once; every kind's materials share its image.
        let fire_images: Vec<_> = FIRE_SPRITES.iter().map(|name| load(name)).collect();
        let smoke_images: Vec<_> = SMOKE_SPRITES.iter().map(|name| load(name)).collect();
        let spark_image = load(SPARK_SPRITE);
        let scorch_image = load(SCORCH_SPRITE);
        let kinds = BlastKind::ALL
            .iter()
            .map(|&kind| {
                let tint = tint(kind);
                KindLook {
                    fire: fire_images.iter().map(|image| sprite_material(image.clone(), tint.fire, AlphaMode::Add)).collect(),
                    smoke: smoke_images.iter().map(|image| sprite_material(image.clone(), tint.smoke, AlphaMode::Blend)).collect(),
                    spark: sprite_material(spark_image.clone(), tint.spark, AlphaMode::Add),
                    scorch: sprite_material(scorch_image.clone(), tint.scorch, AlphaMode::Blend),
                }
            })
            .collect();
        let debris = materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.15, 0.1),
            unlit: true,
            ..default()
        });
        Some(Self {
            quad: meshes.add(Rectangle::new(1.0, 1.0)),
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            sprites,
            kinds,
            debris,
            ready: false,
        })
    }
}

/// One cosmetic particle of a blast.
#[derive(Component)]
pub(crate) struct BlastBit {
    age: f32,
    life: f32,
    size: f32,
    shape: Shape,
    velocity: Vec3,
    gravity: f32,
    /// Turned to face the camera with this roll, or `None` to keep its own rotation (debris).
    roll: Option<f32>,
}

/// A scorch on the ground, which stays a while.
#[derive(Component)]
pub(crate) struct Scorch {
    age: f32,
}

/// Marks each sprite drawable once its image has loaded; until all are, blasts are the sphere.
pub(crate) fn settle_blast(mut assets: ResMut<ExplosiveAssets>, asset_server: Option<Res<AssetServer>>) {
    let Some(look) = assets.blast.as_mut() else { return };
    if look.ready {
        return;
    }
    let Some(asset_server) = asset_server else { return };
    look.ready = look.sprites.iter().all(|sprite| asset_server.is_loaded_with_dependencies(&sprite.image));
}

/// Where the ground is under a blast: its point and normal.
pub(crate) fn ground_under(spatial: &SpatialQuery, at: Vec3, radius: f32) -> Option<(Vec3, Vec3)> {
    let reach = radius * 0.8 + 1.0;
    let filter = SpatialQueryFilter::from_mask(GameLayer::World);
    spatial
        .cast_ray(at + Vec3::Y * 0.3, Dir3::NEG_Y, reach, true, &filter)
        .map(|hit| (at + Vec3::Y * 0.3 + Vec3::NEG_Y * hit.distance, hit.normal))
}

fn jitter() -> f32 {
    rand::random::<f32>() - 0.5
}

/// Puts a blast of `kind` and `radius` at `at`, with a scorch where `ground` (point and normal) says. Returns
/// false when the sprites aren't ready and the caller should draw the bare fireball instead.
pub(crate) fn spawn(
    commands: &mut Commands,
    look: &BlastLook,
    alive: usize,
    scorches: &[(Entity, f32)],
    at: Vec3,
    radius: f32,
    ground: Option<(Vec3, Vec3)>,
    kind: BlastKind,
) -> bool {
    if !look.ready {
        return false;
    }
    let radius = radius.max(0.5);
    let tinted = &look.kinds[kind.index()];
    let pick = |list: &[Handle<StandardMaterial>]| list[rand::random_range(0..list.len())].clone();
    let card = |material: Handle<StandardMaterial>, offset: Vec3, size: f32, life: f32, shape: Shape, rise: f32| {
        (
            InRun,
            BlastBit {
                age: 0.0,
                life,
                size,
                shape,
                velocity: Vec3::Y * rise,
                gravity: 0.0,
                roll: Some(rand::random::<f32>() * std::f32::consts::TAU),
            },
            Mesh3d(look.quad.clone()),
            MeshMaterial3d(material),
            Transform::from_translation(at + offset).with_scale(Vec3::splat(0.01)),
        )
    };
    let mut made = 0usize;
    // The fireball and a smaller, offset flame; then the smoke, which rises and outlasts them.
    let wanted_cards = [
        (pick(&tinted.fire), Vec3::ZERO, radius * 1.7, FIRE_SECS, Shape::Swell, 0.0),
        (pick(&tinted.fire), Vec3::new(jitter(), 0.3, jitter()) * radius * 0.4, radius * 1.1, FIRE_SECS * 1.2, Shape::Swell, 0.5),
        (pick(&tinted.smoke), Vec3::Y * radius * 0.2, radius * 2.0, SMOKE_SECS, Shape::Billow, 0.8),
        (pick(&tinted.smoke), Vec3::new(jitter(), 0.5, jitter()) * radius * 0.5, radius * 1.4, SMOKE_SECS * 0.8, Shape::Billow, 1.1),
    ];
    for (material, offset, size, life, shape, rise) in wanted_cards.into_iter().take(allowed(alive, 4) as usize) {
        commands.spawn(card(material, offset, size, life, shape, rise));
        made += 1;
    }
    let (sparks, debris) = bits_for(radius);
    for _ in 0..allowed(alive + made, sparks) {
        let direction = Vec3::new(jitter(), rand::random::<f32>() * 0.8 + 0.1, jitter()).normalize_or(Vec3::Y);
        commands.spawn((
            InRun,
            BlastBit {
                age: 0.0,
                life: SPARK_SECS * (0.6 + rand::random::<f32>() * 0.6),
                size: 0.06 + radius * 0.015,
                shape: Shape::Shrink,
                velocity: direction * (3.0 + radius * (1.0 + rand::random::<f32>())),
                gravity: GRAVITY * 0.6,
                roll: Some(rand::random::<f32>() * std::f32::consts::TAU),
            },
            Mesh3d(look.quad.clone()),
            MeshMaterial3d(tinted.spark.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(0.01)),
        ));
        made += 1;
    }
    for _ in 0..allowed(alive + made, debris) {
        let direction = Vec3::new(jitter(), rand::random::<f32>() * 0.9 + 0.3, jitter()).normalize_or(Vec3::Y);
        commands.spawn((
            InRun,
            BlastBit {
                age: 0.0,
                life: DEBRIS_SECS * (0.7 + rand::random::<f32>() * 0.5),
                size: 0.05 + rand::random::<f32>() * 0.06,
                shape: Shape::Shrink,
                velocity: direction * (2.5 + radius * rand::random::<f32>() * 1.2),
                gravity: GRAVITY,
                roll: None,
            },
            Mesh3d(look.cube.clone()),
            MeshMaterial3d(look.debris.clone()),
            Transform::from_translation(at)
                .with_rotation(Quat::from_euler(EulerRot::XYZ, jitter() * 6.0, jitter() * 6.0, jitter() * 6.0))
                .with_scale(Vec3::splat(0.01)),
        ));
    }
    if let Some((point, normal)) = ground {
        // Make room: the oldest scorches go first.
        let mut oldest = scorches.to_vec();
        oldest.sort_by(|a, b| b.1.total_cmp(&a.1));
        let ages: Vec<f32> = oldest.iter().map(|&(_, age)| age).collect();
        for &(entity, _) in oldest.iter().take(scorches_to_clear(&ages)) {
            commands.entity(entity).despawn();
        }
        let up = Dir3::new(normal).unwrap_or(Dir3::Y);
        // A quad faces +Z; turn that to the surface normal, with a spin about it of its own.
        let rotation = Quat::from_rotation_arc(Vec3::Z, *up) * Quat::from_rotation_z(rand::random::<f32>() * std::f32::consts::TAU);
        commands.spawn((
            InRun,
            Scorch { age: 0.0 },
            Mesh3d(look.quad.clone()),
            MeshMaterial3d(tinted.scorch.clone()),
            Transform::from_translation(point + *up * (0.02 + rand::random::<f32>() * 0.01))
                .with_rotation(rotation)
                .with_scale(Vec3::splat(radius * 1.6)),
        ));
    }
    true
}

/// Particles drift, fall, change size and vanish; the flat ones face the camera.
pub(crate) fn update_blast(
    mut commands: Commands,
    mut bits: Query<(Entity, &mut BlastBit, &mut Transform)>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let eye = cameras.iter().next().map(GlobalTransform::translation);
    for (entity, mut bit, mut transform) in &mut bits {
        bit.age += dt;
        if bit.age >= bit.life {
            commands.entity(entity).despawn();
            continue;
        }
        let gravity = bit.gravity;
        bit.velocity.y -= gravity * dt;
        transform.translation += bit.velocity * dt;
        transform.scale = Vec3::splat(bit.size * envelope(bit.shape, bit.age / bit.life));
        if let (Some(roll), Some(eye)) = (bit.roll, eye) {
            let toward = eye - transform.translation;
            if toward.length_squared() > 1e-6 {
                transform.rotation = Transform::IDENTITY.looking_to(-toward, Vec3::Y).rotation * Quat::from_rotation_z(roll);
            }
        }
    }
}

/// Scorches age, then go.
pub(crate) fn update_scorches(mut commands: Commands, mut scorches: Query<(Entity, &mut Scorch)>, time: Res<Time>) {
    for (entity, mut scorch) in &mut scorches {
        scorch.age += time.delta_secs();
        if scorch.age >= SCORCH_SECS {
            commands.entity(entity).despawn();
        }
    }
}

/// What a system needs to show a blast: the live particles and scorches, and the ground.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Blasts<'w, 's> {
    bits: Query<'w, 's, (), With<BlastBit>>,
    scorches: Query<'w, 's, (Entity, &'static Scorch)>,
    spatial: Option<SpatialQuery<'w, 's>>,
}

impl Blasts<'_, '_> {
    /// Shows a blast of `kind` and `radius` at `at`: the textured one, in the kind's tint, once its
    /// sprites have loaded, else the bare fireball.
    pub(crate) fn show(&self, commands: &mut Commands, assets: &ExplosiveAssets, at: Vec3, radius: f32, kind: BlastKind) {
        if let Some(look) = &assets.blast {
            let ground = self.spatial.as_ref().and_then(|spatial| ground_under(spatial, at, radius));
            let scorches: Vec<(Entity, f32)> = self.scorches.iter().map(|(entity, scorch)| (entity, scorch.age)).collect();
            if spawn(commands, look, self.bits.iter().count(), &scorches, at, radius, ground, kind) {
                return;
            }
        }
        commands.spawn(fireball(assets, at, radius, kind));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fireball_swells_then_shrinks() {
        assert!(envelope(Shape::Swell, 0.5) > envelope(Shape::Swell, 0.1));
        assert!(envelope(Shape::Swell, 0.5) > envelope(Shape::Swell, 0.95));
        assert!((envelope(Shape::Swell, 0.5) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn smoke_billows_holds_and_thins() {
        assert!(envelope(Shape::Billow, 0.4) > envelope(Shape::Billow, 0.0));
        assert!((envelope(Shape::Billow, 0.6) - 1.0).abs() < 1e-3);
        assert!(envelope(Shape::Billow, 0.99) < 0.1);
    }

    #[test]
    fn sparks_shrink_to_nothing() {
        assert_eq!(envelope(Shape::Shrink, 0.0), 1.0);
        assert_eq!(envelope(Shape::Shrink, 1.0), 0.0);
    }

    #[test]
    fn bigger_blasts_throw_more_and_the_cap_holds() {
        let (small_sparks, small_debris) = bits_for(1.0);
        let (big_sparks, big_debris) = bits_for(5.7);
        assert!(big_sparks > small_sparks && big_debris > small_debris);
        assert_eq!(bits_for(100.0), bits_for(8.0));
        assert_eq!(allowed(BLAST_PARTICLE_CAP - 3, 10), 3);
        assert_eq!(allowed(BLAST_PARTICLE_CAP + 5, 10), 0);
        assert_eq!(allowed(0, 4), 4);
    }

    #[test]
    fn the_oldest_scorches_make_room() {
        assert_eq!(scorches_to_clear(&[1.0; SCORCH_CAP - 1]), 0);
        assert_eq!(scorches_to_clear(&[1.0; SCORCH_CAP]), 1);
        assert_eq!(scorches_to_clear(&[1.0; SCORCH_CAP + 2]), 3);
    }

    fn ready_look(_world: &mut World) -> BlastLook {
        let handle = || Handle::<StandardMaterial>::default();
        BlastLook {
            quad: Handle::default(),
            cube: Handle::default(),
            sprites: Vec::new(),
            kinds: BlastKind::ALL
                .iter()
                .map(|_| KindLook { fire: vec![handle()], smoke: vec![handle()], spark: handle(), scorch: handle() })
                .collect(),
            debris: handle(),
            ready: true,
        }
    }

    #[test]
    fn a_blast_throws_cards_sparks_debris_and_a_scorch_within_the_caps() {
        let mut world = World::new();
        let look = ready_look(&mut world);
        let ground = Some((Vec3::ZERO, Vec3::Y));
        let mut commands = world.commands();
        assert!(spawn(&mut commands, &look, 0, &[], Vec3::Y, 5.0, ground, BlastKind::Plain));
        world.flush();
        let (sparks, debris) = bits_for(5.0);
        assert_eq!(world.query::<&BlastBit>().iter(&world).count() as u32, 4 + sparks + debris);
        assert_eq!(world.query::<&Scorch>().iter(&world).count(), 1);
        // With the cap already reached, only the scorch is made.
        let mut commands = world.commands();
        spawn(&mut commands, &look, BLAST_PARTICLE_CAP, &[], Vec3::Y, 5.0, None, BlastKind::Plain);
        world.flush();
        assert_eq!(world.query::<&BlastBit>().iter(&world).count() as u32, 4 + sparks + debris);
    }

    #[test]
    fn a_blast_whose_sprites_have_not_loaded_is_left_to_the_bare_fireball() {
        let mut world = World::new();
        let mut look = ready_look(&mut world);
        look.ready = false;
        let mut commands = world.commands();
        assert!(!spawn(&mut commands, &look, 0, &[], Vec3::ZERO, 3.0, None, BlastKind::Plain));
        world.flush();
        assert_eq!(world.query::<&BlastBit>().iter(&world).count(), 0);
    }

    #[test]
    fn a_plain_blast_keeps_the_look_it_had_and_every_other_kind_has_its_own() {
        let plain = tint(BlastKind::Plain);
        assert_eq!(plain.fire, Color::linear_rgb(2.4, 1.0, 0.3));
        assert_eq!(plain.smoke, Color::srgba(0.22, 0.21, 0.2, 0.85));
        assert_eq!(plain.spark, Color::linear_rgb(3.0, 1.8, 0.5));
        assert_eq!(plain.scorch, Color::srgba(0.01, 0.01, 0.01, 0.9));
        for (i, a) in BlastKind::ALL.iter().enumerate() {
            assert_eq!(a.index(), i, "ALL is in index order");
            for b in &BlastKind::ALL[i + 1..] {
                assert_ne!(tint(*a).fire, tint(*b).fire, "{a:?} and {b:?} would burn the same");
            }
        }
    }

    #[test]
    fn every_kind_spawns_from_its_own_materials() {
        let mut world = World::new();
        let look = ready_look(&mut world);
        for kind in BlastKind::ALL {
            let mut commands = world.commands();
            assert!(spawn(&mut commands, &look, 0, &[], Vec3::Y, 2.0, None, kind));
        }
        world.flush();
    }

    #[test]
    fn every_sprite_has_a_file_name() {
        assert_eq!(sprite_file(SCORCH_SPRITE), "sprites/scorch_01/color.ktx2");
    }
}
