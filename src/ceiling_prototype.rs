//! PROTOTYPE, throwaway (wayfinder ticket #302 "Do the Greybox's Zones get ceilings?"). Not for
//! main.
//!
//! Three roofs over the Greybox at dusk (run with `ZOMBIES_LOOK=dusk`), switchable in game:
//!
//! - `open`:  the Greybox as it is, open to the sky.
//! - `dark`:  a ceiling over every Zone but the Yard and the Balcony, lamps off. Only the windows
//!   and open Doors let the sun in.
//! - `lamps`: the same ceilings, every lamp a downward spot with a glowing bulb, the 4 nearest the
//!   eye shadowed.
//!
//! Under a ceiling the sky's ambient shouldn't reach, so while the eye stands in a roofed Zone the
//! roofed variants turn the global ambient down. That stands in for per-Zone light probes: right
//! for the room you're in, wrong for a sunlit Yard seen through a Door.
//!
//! `ZOMBIES_ROOF=<name>` picks at start (default `open`); `=` and `-` cycle it in game.
//! `ZOMBIES_ROOF_TOUR=<dir>` skips the menu, visits fixed viewpoints, saves a screenshot of every
//! roof at each into `<dir>`, and quits. `ZOMBIES_BENCH=greybox` measures whichever roof
//! `ZOMBIES_ROOF` names.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::look_prototype::{Look, Lamp};
use crate::maps::map::{MapId, STOREY, palette};
use crate::maps::zone::ZoneId;
use crate::players::player::{LocalPlayer, PlayerCamera, Teleport};
use crate::state::{GameState, InRun};
use crate::tex_prototype::{Surface, SurfaceKind};

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Roof {
    Open,
    Dark,
    Lamps,
}

const ROOFS: [Roof; 3] = [Roof::Open, Roof::Dark, Roof::Lamps];

/// The dusk ambient's brightness under a ceiling (the open sky's is 450).
pub const INDOOR_AMBIENT: f32 = 120.0;
/// A roofed lamp's spot, in lumens.
pub const LAMP_INTENSITY: f32 = 1_500_000.0;
/// A roofed lamp's spot cone, in radians from straight down.
pub const LAMP_CONE: f32 = 1.45;
/// The distance fog's colour under a ceiling, and dusk's out under the sky.
const INDOOR_FOG: Color = Color::srgb(0.035, 0.03, 0.03);
const DUSK_FOG: Color = Color::srgb(0.28, 0.25, 0.28);
const DUSK_AMBIENT: f32 = 450.0;

impl Roof {
    pub fn name(self) -> &'static str {
        match self {
            Roof::Open => "open",
            Roof::Dark => "dark",
            Roof::Lamps => "lamps",
        }
    }

    fn from_env() -> Self {
        let Ok(name) = std::env::var("ZOMBIES_ROOF") else { return Roof::Open };
        ROOFS
            .into_iter()
            .find(|roof| roof.name() == name.to_ascii_lowercase())
            .unwrap_or_else(|| panic!("ZOMBIES_ROOF={name}: expected open, dark or lamps"))
    }
}

/// A ceiling slab over one Zone.
#[derive(Component)]
struct Ceiling;

/// The Greybox's Zones that get a ceiling, and how high it sits: everything but the Yard and the
/// Balcony, which stay outdoors.
fn ceiling_height(zone: ZoneId) -> Option<f32> {
    use ZoneId::*;
    match zone {
        WestHall => Some(2.0 * STOREY),
        StartRoom | EastHall | DiningRoom | PowerRoom | Passage | Workshop | Storeroom | GeneratorRoom | Tunnel => Some(STOREY),
        _ => None,
    }
}

pub struct CeilingPrototypePlugin;

impl Plugin for CeilingPrototypePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Roof::from_env()).add_systems(
            Update,
            (cycle, spawn_ceilings, show_ceilings, ambient)
                .chain()
                .after(crate::look_prototype::apply_world)
                .run_if(in_state(GameState::Playing)),
        );
        if let Ok(dir) = std::env::var("ZOMBIES_ROOF_TOUR") {
            std::fs::create_dir_all(&dir).unwrap();
            app.insert_resource(Tour { dir, ..default() })
                .add_systems(OnEnter(GameState::MainMenu), crate::look_prototype::start_run)
                .add_systems(Update, (crate::look_prototype::keep_unhurt, tour).run_if(in_state(GameState::Playing)));
        }
    }
}

fn cycle(keys: Res<ButtonInput<KeyCode>>, mut roof: ResMut<Roof>) {
    let by = if keys.just_pressed(KeyCode::Equal) {
        1
    } else if keys.just_pressed(KeyCode::Minus) {
        -1
    } else {
        return;
    };
    let at = ROOFS.iter().position(|&r| r == *roof).unwrap() as i32;
    *roof = ROOFS[(at + by).rem_euclid(ROOFS.len() as i32) as usize];
    info!("roof: {}", roof.name());
}

/// Once the Greybox's lamps are in (the Map is built), a slab on top of each roofed Zone's walls.
/// It's tagged as its Zone's wall, so the texture prototype gives it the Zone's wall set.
fn spawn_ceilings(
    mut commands: Commands,
    map: Res<MapId>,
    roof: Res<Roof>,
    lamps: Query<(), With<Lamp>>,
    ceilings: Query<(), With<Ceiling>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if *map != MapId::Greybox || lamps.is_empty() || !ceilings.is_empty() {
        return;
    }
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    for &zone in map.layout().zones {
        let Some(height) = ceiling_height(zone) else { continue };
        let (lo, hi) = zone.volume();
        // Over the walls' tops, so no sky shows at the edges.
        let size = Vec3::new(hi.x - lo.x + 0.4, 0.2, hi.z - lo.z + 0.4);
        let centre = Vec3::new((lo.x + hi.x) / 2.0, height + 0.1, (lo.z + hi.z) / 2.0);
        commands.spawn((
            InRun,
            Ceiling,
            Surface { zone: Some(zone), kind: SurfaceKind::Wall },
            Mesh3d(cube.clone()),
            MeshMaterial3d(materials.add(palette(Some(zone)).wall)),
            Transform::from_translation(centre).with_scale(size),
            if *roof == Roof::Open { Visibility::Hidden } else { Visibility::Inherited },
        ));
    }
}

fn show_ceilings(roof: Res<Roof>, added: Query<(), Added<Ceiling>>, mut ceilings: Query<&mut Visibility, With<Ceiling>>) {
    if !roof.is_changed() && added.is_empty() {
        return;
    }
    for mut visibility in &mut ceilings {
        *visibility = if *roof == Roof::Open { Visibility::Hidden } else { Visibility::Inherited };
    }
}

/// Under a ceiling, the sky's ambient is turned down; out under the sky it's dusk's.
fn ambient(
    look: Res<Look>,
    roof: Res<Roof>,
    map: Res<MapId>,
    eye: Query<&GlobalTransform, With<PlayerCamera>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut fogs: Query<&mut bevy::pbr::DistanceFog>,
) {
    if *look != Look::Dusk {
        return;
    }
    let Some(eye) = eye.iter().next().map(GlobalTransform::translation) else { return };
    let indoors = *roof != Roof::Open
        && map.layout().zones.iter().any(|&zone| {
            let (lo, hi) = zone.volume();
            ceiling_height(zone).is_some() && eye.cmpge(lo).all() && eye.cmple(hi + Vec3::Y * STOREY).all()
        });
    let want = if indoors { INDOOR_AMBIENT } else { DUSK_AMBIENT };
    if ambient.brightness != want {
        ambient.brightness = want;
    }
    let fog = if indoors { INDOOR_FOG } else { DUSK_FOG };
    for mut f in &mut fogs {
        if f.color != fog {
            f.color = fog;
        }
    }
}

// --- The screenshot tour ---------------------------------------------------------------------

#[derive(Resource, Default)]
struct Tour {
    dir: String,
    frames: u32,
    view: usize,
    roof: usize,
    settle: u32,
    started: bool,
}

/// (name, feet, looking at, pitch)
const VIEWS: [(&str, Vec3, Vec3, f32); 6] = [
    ("start-room", Vec3::new(0.0, 0.0, 25.0), Vec3::new(0.0, 0.0, 14.0), 0.12),
    ("west-hall", Vec3::new(-20.0, 0.0, 24.0), Vec3::new(-20.0, 0.0, -10.0), 0.2),
    ("dining-room", Vec3::new(-24.0, 0.0, -16.0), Vec3::new(-2.0, 0.0, -24.0), 0.08),
    ("passage", Vec3::new(-12.0, 0.0, -27.0), Vec3::new(-12.0, 0.0, -37.0), 0.05),
    ("workshop", Vec3::new(-32.0, 0.0, -40.0), Vec3::new(-6.0, 0.0, -48.0), 0.08),
    ("yard", Vec3::new(10.0, 0.0, 11.0), Vec3::new(-8.0, 0.0, -10.0), 0.05),
];

const TOUR_WARM_UP: u32 = 240;

fn tour(
    mut commands: Commands,
    mut tour: ResMut<Tour>,
    mut roof: ResMut<Roof>,
    player: Query<Entity, With<LocalPlayer>>,
    mut camera: Query<&mut Transform, (With<PlayerCamera>, Without<LocalPlayer>)>,
    mut body: Query<&mut Transform, (With<LocalPlayer>, Without<PlayerCamera>)>,
    mut teleports: MessageWriter<Teleport>,
    mut exit: MessageWriter<AppExit>,
) {
    tour.frames += 1;
    if tour.frames < TOUR_WARM_UP {
        return;
    }
    if tour.view == VIEWS.len() {
        tour.settle += 1;
        if tour.settle > 30 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let Ok(player) = player.single() else { return };
    let (name, feet, at, pitch) = VIEWS[tour.view];
    let facing = Transform::from_translation(feet).looking_at(Vec3::new(at.x, feet.y, at.z), Vec3::Y).rotation;
    for mut transform in &mut body {
        transform.rotation = facing;
    }
    if tour.settle == 0 {
        teleports.write(Teleport { player, feet, facing: Some(facing) });
        *roof = ROOFS[tour.roof];
    }
    for mut transform in &mut camera {
        transform.rotation = Quat::from_rotation_x(pitch);
    }
    tour.settle += 1;
    let wait = if tour.started { 40 } else { 150 };
    if tour.settle < wait {
        return;
    }
    let path = format!("{}/{}-{}.png", tour.dir, name, ROOFS[tour.roof].name());
    info!("tour: {path}");
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    tour.settle = 0;
    tour.roof += 1;
    if tour.roof == ROOFS.len() {
        tour.roof = 0;
        tour.view += 1;
        tour.started = true;
    }
}
