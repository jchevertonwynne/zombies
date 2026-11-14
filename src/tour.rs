//! The screenshot tour, for approving a Map's look: `ZOMBIES_TOUR=<map>` (only `greybox` has views
//! yet) skips the menu, stands the Player at each of the Map's views in turn with no Zombies about,
//! saves a 1920×1080 shot of the game window for each into `ZOMBIES_TOUR_OUT` (default `tour/`),
//! and quits. The route views look from Zone to Zone through the Doors, which are hidden for them.
//! Build with `--release`, and run with a scratch `HOME` so no Run Record is kept.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::PrimaryWindow;

use crate::enemies::zombie::Zombie;
use crate::maps::door::Door;
use crate::maps::map::MapId;
use crate::players::player::{Health, LocalPlayer, Player, PlayerCamera, Teleport};
use crate::run::mode::Mode;
use crate::state::GameState;

/// A shot: its name, where the Player's feet are, a point on the floor it looks towards, the
/// camera's pitch in radians, and whether the Doors are hidden for it.
type View = (&'static str, Vec3, Vec3, f32, bool);

const fn view(name: &'static str, feet: (f32, f32, f32), at: (f32, f32), pitch: f32, doors_hidden: bool) -> View {
    (name, Vec3::new(feet.0, feet.1, feet.2), Vec3::new(at.0, 0.0, at.1), pitch, doors_hidden)
}

const GREYBOX: &[View] = &[
    view("start-room", (9.0, 0.0, 24.0), (-12.0, 16.0), -0.08, false),
    view("start-room-wall", (4.0, 0.0, 19.0), (-2.0, 25.0), -0.2, false),
    view("west-hall", (-20.0, 0.0, 24.0), (-20.0, -10.0), 0.08, false),
    view("west-hall-wall", (-16.0, 0.0, 4.0), (-26.0, 1.0), -0.15, false),
    view("dining-room", (-24.0, 0.0, -16.0), (-2.0, -24.0), -0.05, false),
    view("dining-room-wall", (-14.0, 0.0, -17.0), (-11.0, -26.0), -0.2, false),
    view("passage", (-12.0, 0.0, -26.5), (-12.0, -38.0), -0.1, false),
    view("passage-wall", (-10.9, 0.0, -30.0), (-13.5, -33.0), -0.25, false),
    view("workshop", (-32.0, 0.0, -40.0), (-6.0, -48.0), -0.05, false),
    view("workshop-wall", (-24.0, 0.0, -42.0), (-20.0, -50.0), -0.2, false),
    view("yard", (10.0, 0.0, 11.0), (-8.0, -10.0), 0.05, false),
    view("yard-house", (4.0, 0.0, 2.0), (-4.0, 14.0), 0.1, false),
    view("route-1-start-to-west-hall", (-4.0, 0.0, 20.0), (-26.0, 20.0), 0.0, true),
    view("route-2-west-hall-to-dining", (-20.0, 0.0, 0.0), (-20.0, -24.0), 0.0, true),
    view("route-3-dining-to-passage", (-12.0, 0.0, -15.0), (-12.0, -36.0), -0.05, true),
    view("route-4-passage-to-workshop", (-12.0, 0.0, -28.0), (-14.0, -48.0), -0.15, true),
    view("route-5-yard-to-start", (0.0, 0.0, 2.0), (0.0, 24.0), 0.0, true),
    view("route-6-yard-to-dining", (-7.0, 0.0, 8.0), (-7.0, -24.0), 0.0, true),
];

/// Frames before the first shot, for every file to load.
const WARM_UP: u32 = 240;
/// Frames each view settles for before its shot.
const SETTLE: u32 = 40;

#[derive(Resource)]
pub struct Tour {
    map: MapId,
    views: &'static [View],
    dir: String,
    frames: u32,
    shot: usize,
    settle: u32,
}

impl Tour {
    /// The tour asked for by `ZOMBIES_TOUR`, if any.
    pub fn from_env() -> Option<Self> {
        let name = std::env::var("ZOMBIES_TOUR").ok()?;
        let (map, views) = match name.to_ascii_lowercase().as_str() {
            "greybox" => (MapId::Greybox, GREYBOX),
            other => panic!("ZOMBIES_TOUR={other}: only greybox has views"),
        };
        let dir = std::env::var("ZOMBIES_TOUR_OUT").unwrap_or_else(|_| "tour".into());
        std::fs::create_dir_all(&dir).expect("ZOMBIES_TOUR_OUT is writable");
        Some(Self { map, views, dir, frames: 0, shot: 0, settle: 0 })
    }
}

pub struct TourPlugin(pub Tour);

impl Plugin for TourPlugin {
    fn build(&self, app: &mut App) {
        let tour = Tour { dir: self.0.dir.clone(), ..self.0 };
        app.insert_resource(tour)
            .add_systems(Startup, set_window)
            .add_systems(OnEnter(GameState::MainMenu), start_run)
            .add_systems(Update, (clear, shoot).run_if(in_state(GameState::Playing)));
    }
}

fn set_window(mut window: Single<&mut Window, With<PrimaryWindow>>) {
    window.resolution.set_scale_factor_override(Some(1.0));
    window.resolution.set(1920.0, 1080.0);
}

fn start_run(tour: Res<Tour>, mut map: ResMut<MapId>, mut mode: ResMut<Mode>, mut next: ResMut<NextState<GameState>>) {
    *map = tour.map;
    *mode = Mode::Classic;
    next.set(GameState::Playing);
}

/// No Zombies in the shots, and no harm to the Player.
fn clear(mut commands: Commands, mut players: Query<&mut Health, With<Player>>, zombies: Query<Entity, With<Zombie>>) {
    for mut health in &mut players {
        health.current = health.max;
    }
    for zombie in &zombies {
        commands.entity(zombie).despawn();
    }
}

fn shoot(
    mut commands: Commands,
    mut tour: ResMut<Tour>,
    mut doors: Query<&mut Visibility, With<Door>>,
    player: Single<Entity, With<LocalPlayer>>,
    mut body: Single<&mut Transform, (With<LocalPlayer>, Without<PlayerCamera>)>,
    mut camera: Query<&mut Transform, (With<PlayerCamera>, Without<LocalPlayer>)>,
    mut teleports: MessageWriter<Teleport>,
    mut exit: MessageWriter<AppExit>,
) {
    tour.frames += 1;
    if tour.frames < WARM_UP {
        return;
    }
    if tour.shot == tour.views.len() {
        tour.settle += 1;
        if tour.settle > SETTLE {
            exit.write(AppExit::Success);
        }
        return;
    }
    let (name, feet, at, pitch, doors_hidden) = tour.views[tour.shot];
    let facing = Transform::from_translation(feet).looking_at(Vec3::new(at.x, feet.y, at.z), Vec3::Y).rotation;
    body.rotation = facing;
    for mut transform in &mut camera {
        transform.rotation = Quat::from_rotation_x(pitch);
    }
    if tour.settle == 0 {
        teleports.write(Teleport { player: *player, feet, facing: Some(facing) });
        for mut visibility in &mut doors {
            *visibility = if doors_hidden { Visibility::Hidden } else { Visibility::Inherited };
        }
    }
    tour.settle += 1;
    if tour.settle < SETTLE {
        return;
    }
    let path = format!("{}/{:02}-{name}.png", tour.dir, tour.shot);
    info!("tour: {path}");
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    tour.settle = 0;
    tour.shot += 1;
}
