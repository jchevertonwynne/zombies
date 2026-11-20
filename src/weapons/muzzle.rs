//! What a shot looks like: a short muzzle flash and light on the held Weapon, puffs where hitscan
//! bullets land (grey on the world, red on a Zombie, a bigger burst on a Headshot), and a faint
//! tracer on every few bullets. All of it is cosmetic and reads `ShotFired` and `BulletImpact`.
//! A teammate's gun gets its flash and tracers from `TeammateShot` instead, and the same message
//! throws puffs where their aim lands, found by a ray against the local world, so a Co-op client
//! sees every shot's impacts as the Host does.
//!
//! The flash and puffs are Kenney CC0 sprites (`assets/sprites/<name>/color.ktx2`) once their
//! files have loaded: crossed additive cards for the flash, camera-facing blended quads for the
//! puffs. Until then, or with the files missing (and in headless runs, which never load), they are
//! the boxes they always were.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::enemies::zombie::{HitArea, HitZone};
use crate::layers::GameLayer;
use crate::maps::map::textures::on_disk;
use crate::weapons::weapon::zombie_for;

use crate::weapons::arsenal::WeaponClass;
use crate::weapons::gun_model::{WORLD_SCALE, model_length};
use crate::state::world_runs;
use crate::players::teammate::{TeammateGun, TeammateShot};
use crate::tuning::*;
use crate::weapons::weapon::{ShotFired, ViewModel};

/// What a bullet hit, which decides the colour and size of its puffs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactKind {
    World,
    Zombie,
    Headshot,
}

/// A hitscan bullet landed at `at` on a surface facing `normal`.
#[derive(Message, Debug, Clone, Copy)]
pub struct BulletImpact {
    pub at: Vec3,
    pub normal: Vec3,
    pub kind: ImpactKind,
}

/// How a class's muzzle flash looks: its size and colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashLook {
    pub size: f32,
    pub colour: Color,
}

/// The muzzle flash for a class of Weapon: bigger for Shotguns and LMGs, tinted for the Ray Gun,
/// Thundergun and launchers, and none for the quiet Crossbow and Ballistic Knife.
pub fn flash_look(class: WeaponClass) -> Option<FlashLook> {
    let warm = Color::srgb(1.0, 0.8, 0.4);
    let (size, colour) = match class {
        WeaponClass::Pistol | WeaponClass::DualWield => (MUZZLE_FLASH_SIZE_SMALL, warm),
        WeaponClass::Smg | WeaponClass::Rifle | WeaponClass::Sniper => (MUZZLE_FLASH_SIZE_MEDIUM, warm),
        WeaponClass::Shotgun | WeaponClass::Lmg => (MUZZLE_FLASH_SIZE_LARGE, warm),
        WeaponClass::Launcher => (MUZZLE_FLASH_SIZE_LARGE, Color::srgb(1.0, 0.5, 0.2)),
        WeaponClass::RayGun => (MUZZLE_FLASH_SIZE_MEDIUM, Color::srgb(0.4, 1.0, 0.5)),
        WeaponClass::Thundergun => (MUZZLE_FLASH_SIZE_LARGE, Color::srgb(0.5, 0.7, 1.0)),
        WeaponClass::Crossbow | WeaponClass::BallisticKnife => return None,
    };
    Some(FlashLook { size, colour })
}

/// The sprites a class's muzzle flash draws from, one picked at random per shot: small for
/// pistols, medium for the middle weight, large for the heavy guns. Empty where `flash_look` is
/// `None`.
pub fn flash_frames(class: WeaponClass) -> &'static [&'static str] {
    match class {
        WeaponClass::Pistol | WeaponClass::DualWield => &["muzzle_04", "muzzle_05"],
        WeaponClass::Smg | WeaponClass::Rifle | WeaponClass::Sniper | WeaponClass::RayGun => &["muzzle_02", "muzzle_03"],
        WeaponClass::Shotgun | WeaponClass::Lmg | WeaponClass::Launcher | WeaponClass::Thundergun => {
            &["muzzle_01", "muzzle_02", "muzzle_03"]
        }
        WeaponClass::Crossbow | WeaponClass::BallisticKnife => &[],
    }
}

/// The sprites an impact's puffs draw from: dust on the world, blood on a Zombie, and a bloody
/// mist as well on a Headshot.
pub fn impact_frames(kind: ImpactKind) -> &'static [&'static str] {
    match kind {
        ImpactKind::World => &["smoke_02", "smoke_03"],
        ImpactKind::Zombie => &["dirt_01", "dirt_02"],
        ImpactKind::Headshot => &["dirt_03", "smoke_04"],
    }
}

/// How an impact's sprites are tinted (they are greyscale).
fn impact_tint(kind: ImpactKind) -> Color {
    match kind {
        ImpactKind::World => Color::srgb(0.6, 0.56, 0.5),
        ImpactKind::Zombie => Color::srgb(0.6, 0.03, 0.03),
        ImpactKind::Headshot => Color::srgb(0.85, 0.05, 0.04),
    }
}

const CLASSES: [WeaponClass; 12] = [
    WeaponClass::Pistol,
    WeaponClass::DualWield,
    WeaponClass::Smg,
    WeaponClass::Rifle,
    WeaponClass::Shotgun,
    WeaponClass::Sniper,
    WeaponClass::Lmg,
    WeaponClass::Launcher,
    WeaponClass::RayGun,
    WeaponClass::Thundergun,
    WeaponClass::Crossbow,
    WeaponClass::BallisticKnife,
];

/// Every sprite name any flash or impact uses.
fn all_frames() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for class in CLASSES {
        names.extend(flash_frames(class));
    }
    for kind in [ImpactKind::World, ImpactKind::Zombie, ImpactKind::Headshot] {
        names.extend(impact_frames(kind));
    }
    names.sort_unstable();
    names.dedup();
    names
}

/// Where a sprite's file is, under `assets/`.
pub fn sprite_file(name: &str) -> String {
    format!("sprites/{name}/color.ktx2")
}

/// Where the muzzle sits on the view model for a class, as an offset from the receiver.
pub fn muzzle_position(class: WeaponClass) -> Vec3 {
    Vec3::new(0.0, 0.01, -0.15 * model_length(class))
}

/// How much of its size an effect still has `age` seconds into its `life`: 1 at the start,
/// shrinking quickly to 0 at the end.
pub fn fade(age: f32, life: f32) -> f32 {
    if life <= 0.0 {
        return 0.0;
    }
    let left = (1.0 - age / life).clamp(0.0, 1.0);
    left * left
}

/// How many puffs an impact throws.
pub fn puff_count(kind: ImpactKind) -> u32 {
    match kind {
        ImpactKind::World => IMPACT_PUFFS_WORLD,
        ImpactKind::Zombie => IMPACT_PUFFS_ZOMBIE,
        ImpactKind::Headshot => IMPACT_PUFFS_HEADSHOT,
    }
}

/// How many more puffs may spawn with `alive` already out there, wanting `wanted`.
pub fn puffs_allowed(alive: usize, wanted: u32) -> u32 {
    (IMPACT_PUFF_CAP.saturating_sub(alive) as u32).min(wanted)
}

/// The way a Player looks, as a unit vector: turned `yaw` about +Y (0 faces -Z), then raised `pitch`.
pub fn aim_direction(yaw: f32, pitch: f32) -> Vec3 {
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Vec3::NEG_Z
}

/// A transform for a unit cube stretched into a line from `from` to `to`, `width` across.
pub fn tracer_transform(from: Vec3, to: Vec3, width: f32) -> Transform {
    let line = to - from;
    let length = line.length().max(0.001);
    let direction = line / length;
    let up = if direction.y.abs() > 0.99 { Vec3::X } else { Vec3::Y };
    Transform::from_translation(from.midpoint(to))
        .looking_to(direction, up)
        .with_scale(Vec3::new(width, width, length))
}

/// The flash at the muzzle of the view model, with its light. Hidden between shots.
#[derive(Component)]
struct MuzzleFlash {
    material: Handle<StandardMaterial>,
    size: f32,
    age: f32,
    /// Whether this shot's flash is drawn as sprite cards rather than a box.
    card: bool,
}

/// How a flash is scaled at `size`: a box is squat, a card pair is longer than it is wide.
fn flash_scale(size: f32, card: bool) -> Vec3 {
    if card {
        Vec3::new(size / MUZZLE_FLASH_CARD_ASPECT, size / MUZZLE_FLASH_CARD_ASPECT, size)
    } else {
        Vec3::new(size, size, size * 0.6)
    }
}

/// Marks the flash on a teammate's gun, which only their shots light.
#[derive(Component)]
struct TeammateFlash;

/// A flying puff of dust or blood.
#[derive(Component)]
struct Puff {
    age: f32,
    size: f32,
    velocity: Vec3,
    /// Turned to face the camera, as a sprite is.
    sprite: Option<f32>,
}

/// A faint line from the muzzle to where a bullet landed.
#[derive(Component)]
struct Tracer {
    age: f32,
}

/// One sprite image, drawn once it has loaded.
struct Sprite {
    image: Handle<Image>,
    ready: bool,
}

/// An impact's blended material for one of its sprites.
struct ImpactSprite {
    image: &'static str,
    material: Handle<StandardMaterial>,
}

/// The meshes and materials shared by every puff and tracer.
#[derive(Resource)]
struct EffectAssets {
    cube: Handle<Mesh>,
    /// Two crossed quads along -Z, the sprite's up pointing down the barrel.
    cards: Handle<Mesh>,
    /// One quad facing +Z.
    quad: Handle<Mesh>,
    /// Each sprite whose file is on disk.
    sprites: HashMap<&'static str, Sprite>,
    impact_sprites: Vec<(ImpactKind, ImpactSprite)>,
    dust: Handle<StandardMaterial>,
    blood: Handle<StandardMaterial>,
    headshot: Handle<StandardMaterial>,
    tracer: Handle<StandardMaterial>,
}

pub struct MuzzlePlugin;

impl Plugin for MuzzlePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BulletImpact>()
            .add_systems(Startup, setup_assets)
            .add_systems(PreUpdate, settle_sprites.run_if(world_runs))
            .add_systems(
                Update,
                (
                    attach_flash,
                    attach_teammate_flash,
                    start_flash,
                    start_teammate_flash,
                    teammate_impacts,
                    update_flash,
                    spawn_impacts,
                    update_puffs,
                    update_tracers,
                )
                    .chain()
                    .run_if(world_runs),
            );
    }
}

fn unlit(colour: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: colour,
        emissive: LinearRgba::from(colour),
        unlit: true,
        ..default()
    }
}

/// A material that draws a sprite added to the scene, for a flash.
fn flash_sprite_material(image: Handle<Image>, colour: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: colour,
        base_color_texture: Some(image),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

/// Two quads crossed along -Z: each is `1` wide and `1` long, centred, its image's up at -Z.
fn cards_mesh() -> Mesh {
    let lying = Mesh::from(Rectangle::new(1.0, 1.0)).rotated_by(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
    let mut cards = lying.clone();
    let standing = lying.rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
    cards.merge(&standing).expect("two quads have the same layout");
    cards
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Option<Res<AssetServer>>,
) {
    // Only the sprites whose files are there: headless runs and a missing file draw boxes.
    let mut sprites = HashMap::new();
    let mut impact_sprites = Vec::new();
    if let Some(asset_server) = asset_server {
        for name in all_frames() {
            let file = sprite_file(name);
            if on_disk(&file) {
                sprites.insert(name, Sprite { image: asset_server.load(file), ready: false });
            } else {
                debug!("{name}: sprite missing, drawn as a box");
            }
        }
        for kind in [ImpactKind::World, ImpactKind::Zombie, ImpactKind::Headshot] {
            for &image in impact_frames(kind) {
                let Some(sprite) = sprites.get(image) else { continue };
                let material = materials.add(StandardMaterial {
                    base_color: impact_tint(kind),
                    base_color_texture: Some(sprite.image.clone()),
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                });
                impact_sprites.push((kind, ImpactSprite { image, material }));
            }
        }
    }
    commands.insert_resource(EffectAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        cards: meshes.add(cards_mesh()),
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        sprites,
        impact_sprites,
        dust: materials.add(unlit(Color::srgb(0.55, 0.53, 0.5))),
        blood: materials.add(unlit(Color::srgb(0.65, 0.04, 0.04))),
        headshot: materials.add(unlit(Color::srgb(0.9, 0.08, 0.06))),
        tracer: materials.add(unlit(Color::srgb(1.0, 0.9, 0.6))),
    });
}

/// Marks each sprite drawable once its image has loaded.
fn settle_sprites(mut assets: ResMut<EffectAssets>, asset_server: Option<Res<AssetServer>>) {
    let Some(asset_server) = asset_server else { return };
    if assets.sprites.values().all(|sprite| sprite.ready) {
        return;
    }
    for sprite in assets.sprites.values_mut() {
        if !sprite.ready && asset_server.is_loaded_with_dependencies(&sprite.image) {
            sprite.ready = true;
        }
    }
}

impl EffectAssets {
    /// A loaded sprite by name.
    fn ready(&self, name: &str) -> Option<&Sprite> {
        self.sprites.get(name).filter(|sprite| sprite.ready)
    }

    /// A random loaded sprite of `names`.
    fn pick(&self, names: &[&'static str]) -> Option<&Sprite> {
        let loaded: Vec<&Sprite> = names.iter().filter_map(|name| self.ready(name)).collect();
        (!loaded.is_empty()).then(|| loaded[rand::random_range(0..loaded.len())])
    }
}

/// A flash with its light, dark until a shot lights it.
fn new_flash(assets: &EffectAssets, materials: &mut Assets<StandardMaterial>) -> impl Bundle {
    let material = materials.add(unlit(Color::WHITE));
    (
        MuzzleFlash { material: material.clone(), size: 0.0, age: MUZZLE_FLASH_SECS, card: false },
        Mesh3d(assets.cube.clone()),
        MeshMaterial3d(material),
        PointLight {
            intensity: 0.0,
            range: MUZZLE_LIGHT_RANGE,
            ..default()
        },
        Transform::default(),
        Visibility::Hidden,
    )
}

/// Gives each new view model its flash, as a child.
fn attach_flash(
    mut commands: Commands,
    view_models: Query<Entity, Added<ViewModel>>,
    assets: Res<EffectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for view_model in &view_models {
        commands.entity(view_model).with_child(new_flash(&assets, &mut materials));
    }
}

/// Gives each teammate's gun its flash, as a child.
fn attach_teammate_flash(
    mut commands: Commands,
    guns: Query<Entity, Added<TeammateGun>>,
    assets: Res<EffectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for gun in &guns {
        commands.entity(gun).with_child((TeammateFlash, new_flash(&assets, &mut materials)));
    }
}

/// Lights `flash` for a shot of `class` from a gun whose muzzle sits `muzzle_scale` times as far
/// out as on the view model, turned a random way so no two look alike.
fn light_flash(
    look: FlashLook,
    class: WeaponClass,
    muzzle_scale: f32,
    (mut flash, mut transform, mut visibility, mut light, mut mesh): (
        Mut<MuzzleFlash>,
        Mut<Transform>,
        Mut<Visibility>,
        Mut<PointLight>,
        Mut<Mesh3d>,
    ),
    materials: &mut Assets<StandardMaterial>,
    assets: &EffectAssets,
) {
    flash.age = 0.0;
    flash.size = look.size;
    *visibility = Visibility::Inherited;
    // A loaded sprite of the class draws crossed cards; otherwise the box.
    let sprite = assets.pick(flash_frames(class));
    flash.card = sprite.is_some();
    mesh.0 = if flash.card { assets.cards.clone() } else { assets.cube.clone() };
    // Centred a little past the muzzle so the flash sits in front of the barrel.
    let at = muzzle_position(class) * muzzle_scale - Vec3::Z * look.size * 0.5;
    *transform = Transform::from_translation(at)
        .with_rotation(Quat::from_rotation_z(rand::random::<f32>() * std::f32::consts::TAU))
        .with_scale(flash_scale(look.size, flash.card));
    light.color = look.colour;
    light.intensity = MUZZLE_LIGHT_INTENSITY;
    if let Some(mut material) = materials.get_mut(&flash.material) {
        match sprite {
            Some(sprite) => {
                *material = flash_sprite_material(
                    sprite.image.clone(),
                    Color::from(LinearRgba::from(look.colour) * MUZZLE_FLASH_SPRITE_GAIN),
                );
            }
            None => {
                *material = unlit(look.colour);
                material.emissive = LinearRgba::from(look.colour) * 4.0;
            }
        }
    }
}

/// A shot lights the flash at the muzzle.
fn start_flash(
    mut shots: MessageReader<ShotFired>,
    flashes: Query<
        (&mut MuzzleFlash, &mut Transform, &mut Visibility, &mut PointLight, &mut Mesh3d),
        Without<TeammateFlash>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<EffectAssets>,
) {
    let Some(shot) = shots.read().last() else { return };
    // An Underbarrel fires as its own class of gun.
    let class = match shot.underbarrel {
        Some(underbarrel) => underbarrel.stats().class,
        None => shot.weapon.stats().class,
    };
    let Some(look) = flash_look(class) else { return };
    for flash in flashes {
        light_flash(look, class, 1.0, flash, &mut materials, &assets);
    }
}

/// A teammate's shot lights the flash on their gun, and every few draw a tracer along their aim.
fn start_teammate_flash(
    mut commands: Commands,
    mut shots: MessageReader<TeammateShot>,
    children: Query<&Children>,
    mut flashes: Query<
        (&mut MuzzleFlash, &mut Transform, &mut Visibility, &mut PointLight, &mut Mesh3d),
        With<TeammateFlash>,
    >,
    assets: Res<EffectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for shot in shots.read() {
        if let Some(look) = flash_look(shot.class)
            && let Ok(kids) = children.get(shot.gun)
        {
            for &kid in kids {
                if let Ok((flash, transform, visibility, light, mesh)) = flashes.get_mut(kid) {
                    light_flash(look, shot.class, WORLD_SCALE, (flash, transform, visibility, light, mesh), &mut materials, &assets);
                }
            }
        }
        if shot.tracer {
            commands.spawn((
                Tracer { age: 0.0 },
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.tracer.clone()),
                tracer_transform(shot.muzzle, shot.muzzle + shot.aim * TEAMMATE_TRACER_LENGTH, TRACER_WIDTH),
            ));
        }
    }
}

/// The flash and its light shrink away, then hide.
fn update_flash(
    mut flashes: Query<(&mut MuzzleFlash, &mut Transform, &mut Visibility, &mut PointLight)>,
    time: Res<Time>,
) {
    for (mut flash, mut transform, mut visibility, mut light) in &mut flashes {
        if flash.age >= MUZZLE_FLASH_SECS {
            continue;
        }
        flash.age += time.delta_secs();
        let left = fade(flash.age, MUZZLE_FLASH_SECS);
        if left <= 0.0 {
            *visibility = Visibility::Hidden;
            light.intensity = 0.0;
            continue;
        }
        let size = flash.size * left;
        transform.scale = flash_scale(size, flash.card);
        light.intensity = MUZZLE_LIGHT_INTENSITY * left;
    }
}

/// A teammate's shot lands where their aim meets the world or a Zombie, which is found here with
/// a ray (without the spread the shooter's own machine rolled), so every machine in a Co-op Run
/// draws the same kinds of impact. Pellets share the one ray.
fn teammate_impacts(
    mut shots: MessageReader<TeammateShot>,
    spatial: SpatialQuery,
    hit_areas: Query<(&HitArea, &ChildOf)>,
    mut impacts: MessageWriter<BulletImpact>,
) {
    let filter = SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::ZombieHitArea]);
    for shot in shots.read() {
        let Ok(direction) = Dir3::new(shot.aim) else { continue };
        if let Some(first) = spatial.cast_ray(shot.muzzle, direction, HITSCAN_RANGE, true, &filter) {
            impacts.write(BulletImpact {
                at: shot.muzzle + *direction * first.distance,
                normal: first.normal,
                kind: match zombie_for(first.entity, &hit_areas) {
                    Some((_, HitZone::Head)) => ImpactKind::Headshot,
                    Some(_) => ImpactKind::Zombie,
                    None => ImpactKind::World,
                },
            });
        }
    }
}

/// Each bullet that lands throws puffs, within the cap, and every few draw a tracer.
fn spawn_impacts(
    mut commands: Commands,
    mut impacts: MessageReader<BulletImpact>,
    assets: Res<EffectAssets>,
    alive: Query<(), With<Puff>>,
    flashes: Query<&GlobalTransform, (With<MuzzleFlash>, Without<TeammateFlash>)>,
    mut landed: Local<u32>,
) {
    let mut count = alive.iter().count();
    for impact in impacts.read() {
        let (material, size) = match impact.kind {
            ImpactKind::World => (&assets.dust, IMPACT_PUFF_SIZE),
            ImpactKind::Zombie => (&assets.blood, IMPACT_PUFF_SIZE),
            ImpactKind::Headshot => (&assets.headshot, IMPACT_PUFF_SIZE_HEADSHOT),
        };
        let puffs = puffs_allowed(count, puff_count(impact.kind));
        count += puffs as usize;
        for _ in 0..puffs {
            let scatter = Vec3::new(
                rand::random::<f32>() - 0.5,
                rand::random::<f32>() - 0.3,
                rand::random::<f32>() - 0.5,
            );
            // A loaded sprite of the kind makes a camera-facing quad, with a roll of its own;
            // otherwise the coloured cube.
            let sprite = sprites_of(&assets, impact.kind);
            let sprite = (!sprite.is_empty()).then(|| sprite[rand::random_range(0..sprite.len())]);
            let (mesh, material, size, roll) = match sprite {
                Some(sprite) => (
                    assets.quad.clone(),
                    sprite.material.clone(),
                    size * IMPACT_SPRITE_SCALE,
                    Some(rand::random::<f32>() * std::f32::consts::TAU),
                ),
                None => (assets.cube.clone(), material.clone(), size, None),
            };
            commands.spawn((
                Puff {
                    age: 0.0,
                    size,
                    velocity: (impact.normal + scatter) * IMPACT_PUFF_SPEED * (0.4 + rand::random::<f32>() * 0.6),
                    sprite: roll,
                },
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_translation(impact.at + impact.normal * 0.02).with_scale(Vec3::splat(size)),
            ));
        }
        *landed += 1;
        if landed.is_multiple_of(TRACER_EVERY)
            && let Ok(muzzle) = flashes.single()
        {
            commands.spawn((
                Tracer { age: 0.0 },
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.tracer.clone()),
                tracer_transform(muzzle.translation(), impact.at, TRACER_WIDTH),
            ));
        }
    }
}

/// The impact sprites of `kind` whose images have loaded.
fn sprites_of(assets: &EffectAssets, kind: ImpactKind) -> Vec<&ImpactSprite> {
    assets
        .impact_sprites
        .iter()
        .filter(|(of, sprite)| *of == kind && assets.ready(sprite.image).is_some())
        .map(|(_, sprite)| sprite)
        .collect()
}

/// Puffs drift out, fall a little, shrink and vanish; sprite puffs face the camera.
fn update_puffs(
    mut commands: Commands,
    mut puffs: Query<(Entity, &mut Puff, &mut Transform)>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let eye = cameras.iter().next().map(GlobalTransform::translation);
    for (entity, mut puff, mut transform) in &mut puffs {
        puff.age += dt;
        if puff.age >= IMPACT_PUFF_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        puff.velocity.y -= GRAVITY * 0.3 * dt;
        transform.translation += puff.velocity * dt;
        transform.scale = Vec3::splat(puff.size * fade(puff.age, IMPACT_PUFF_SECS));
        if let (Some(roll), Some(eye)) = (puff.sprite, eye) {
            let toward = eye - transform.translation;
            if toward.length_squared() > 1e-6 {
                transform.rotation = Transform::IDENTITY.looking_to(-toward, Vec3::Y).rotation * Quat::from_rotation_z(roll);
            }
        }
    }
}

/// Tracers thin out over their short life.
fn update_tracers(mut commands: Commands, mut tracers: Query<(Entity, &mut Tracer, &mut Transform)>, time: Res<Time>) {
    for (entity, mut tracer, mut transform) in &mut tracers {
        tracer.age += time.delta_secs();
        if tracer.age >= TRACER_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        let thin = fade(tracer.age, TRACER_SECS);
        transform.scale.x = TRACER_WIDTH * thin;
        transform.scale.y = TRACER_WIDTH * thin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shotguns_and_lmgs_flash_bigger_than_pistols() {
        let size = |class| flash_look(class).unwrap().size;
        assert!(size(WeaponClass::Shotgun) > size(WeaponClass::Rifle));
        assert!(size(WeaponClass::Lmg) > size(WeaponClass::Pistol));
        assert!(size(WeaponClass::Rifle) > size(WeaponClass::Pistol));
    }

    #[test]
    fn quiet_weapons_have_no_flash() {
        assert!(flash_look(WeaponClass::Crossbow).is_none());
        assert!(flash_look(WeaponClass::BallisticKnife).is_none());
    }

    #[test]
    fn every_class_that_flashes_has_frames_and_the_quiet_ones_none() {
        for class in CLASSES {
            assert_eq!(flash_look(class).is_some(), !flash_frames(class).is_empty(), "{class:?}");
        }
        for kind in [ImpactKind::World, ImpactKind::Zombie, ImpactKind::Headshot] {
            assert!(!impact_frames(kind).is_empty(), "{kind:?}");
        }
        // Sprites are shared, so the list of files has no repeats.
        let names = all_frames();
        assert!(names.windows(2).all(|pair| pair[0] != pair[1]));
        assert_eq!(sprite_file("muzzle_01"), "sprites/muzzle_01/color.ktx2");
    }

    #[test]
    fn a_flash_card_is_longer_than_a_box_is_squat() {
        assert!((flash_scale(0.1, false) - Vec3::new(0.1, 0.1, 0.06)).length() < 1e-6);
        let card = flash_scale(0.1, true);
        assert!((card.z - 0.1).abs() < 1e-6 && card.x < card.z);
    }

    #[test]
    fn muzzle_sits_further_out_on_longer_models() {
        assert!(muzzle_position(WeaponClass::Lmg).z < muzzle_position(WeaponClass::Pistol).z);
        assert!((muzzle_position(WeaponClass::Pistol).z + 0.15).abs() < 1e-6);
    }

    #[test]
    fn fade_runs_from_one_to_zero() {
        assert_eq!(fade(0.0, 0.2), 1.0);
        assert!(fade(0.1, 0.2) < 0.5);
        assert_eq!(fade(0.2, 0.2), 0.0);
        assert_eq!(fade(5.0, 0.2), 0.0);
        assert_eq!(fade(0.0, 0.0), 0.0);
    }

    #[test]
    fn headshots_burst_bigger_and_the_cap_holds() {
        assert!(puff_count(ImpactKind::Headshot) > puff_count(ImpactKind::Zombie));
        assert!(puff_count(ImpactKind::Zombie) >= puff_count(ImpactKind::World));
        assert_eq!(puffs_allowed(0, 5), 5);
        assert_eq!(puffs_allowed(IMPACT_PUFF_CAP - 2, 5), 2);
        assert_eq!(puffs_allowed(IMPACT_PUFF_CAP + 10, 5), 0);
    }

    #[test]
    fn aim_follows_yaw_and_pitch() {
        assert!((aim_direction(0.0, 0.0) - Vec3::NEG_Z).length() < 1e-5);
        assert!(aim_direction(0.0, 0.5).y > 0.4, "looking up aims up");
        assert!((aim_direction(std::f32::consts::FRAC_PI_2, 0.0) - Vec3::NEG_X).length() < 1e-5);
    }

    #[test]
    fn tracer_spans_the_line() {
        let transform = tracer_transform(Vec3::ZERO, Vec3::new(0.0, 0.0, -10.0), 0.01);
        assert!((transform.scale.z - 10.0).abs() < 1e-5);
        assert!((transform.translation.z + 5.0).abs() < 1e-5);
        // Straight up doesn't degenerate.
        let up = tracer_transform(Vec3::ZERO, Vec3::Y * 4.0, 0.01);
        assert!(up.rotation.is_finite());
    }
}
