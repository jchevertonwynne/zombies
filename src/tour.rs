//! The screenshot tour, for approving a Map's look: `ZOMBIES_TOUR=<map>` (`greybox`, `cove`, `dam`, `village` or `quarry`) skips the menu, stands the Player at each of the Map's views in turn with no Zombies about,
//! The screenshot tour, for approving a Map's look: `ZOMBIES_TOUR=<map>` (`greybox`, `cove`, `dam` or `polder`) skips the menu, stands the Player at each of the Map's views in turn with no Zombies about,
//! saves a 1920×1080 shot of the game window for each into `ZOMBIES_TOUR_OUT` (default `tour/`),
//! and quits. The route views look from Zone to Zone through the Doors, which are hidden for them.
//! `ZOMBIES_TOUR_TIDE=low|rising|high|falling` holds the Polder's Tide at that phase.
//! The Dam's tour Holds every Generator, so its Zones are lit to judge, unless `ZOMBIES_TOUR_POWER=off`.
//! `ZOMBIES_TOUR_POWER=on` throws a Map's Power Switch, so its machines are lit.
//! `ZOMBIES_TOUR_MODE=gauntlet` plays the tour as a Gauntlet Run, the Player holding a dozen Boons
//! so the HUD has something in its corner. Build with `--release`, and run with a scratch `HOME` so no Run Record is kept.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::PrimaryWindow;

use crate::enemies::zombie::Zombie;
use crate::maps::door::Door;
use crate::maps::generator::Generators;
use crate::maps::power::Power;
use crate::maps::map::{MapId, STOREY};
use crate::run::gauntlet::boon::{BOONS, HeldBoons};
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
    view("yard-crates", (-9.5, 0.0, -4.4), (-13.2, -4.5), -0.12, false),
    view("yard-sandbags", (-2.0, 0.0, -8.5), (-2.0, -12.5), -0.12, false),
    view("east-hall", (24.0, 0.0, 22.0), (18.0, -10.0), -0.05, false),
    view("east-hall-wall", (16.0, 0.0, 4.0), (26.0, 1.0), -0.15, false),
    view("power-room", (2.0, 0.0, -16.0), (24.0, -22.0), -0.05, false),
    view("power-room-wall", (14.0, 0.0, -17.0), (14.0, -26.0), -0.2, false),
    view("tunnel", (14.0, 0.0, -27.0), (14.0, -38.0), -0.1, false),
    view("storeroom", (0.0, 0.0, -40.0), (0.0, -50.0), -0.08, false),
    view("storeroom-wall", (-2.5, 0.0, -42.0), (4.0, -48.0), -0.2, false),
    view("generator-room", (6.0, 0.0, -41.0), (30.0, -47.0), -0.05, false),
    view("generator-room-wall", (24.0, 0.0, -42.0), (22.0, -50.0), -0.2, false),
    view("balcony", (-12.0, 4.0, 8.0), (-12.0, -12.0), 0.0, false),
    view("balcony-roof", (-12.0, 4.0, -11.0), (-12.0, -17.0), -0.3, false),
    view("outside-ground", (-6.0, 0.0, 29.0), (6.0, 28.0), -0.05, false),
    view("outside-wall", (16.0, 0.0, 29.0), (22.0, 26.0), 0.0, false),
    view("gazebo", (5.0, 0.0, -2.0), (5.0, -9.0), 0.0, true),
    view("start-room-window", (7.0, 0.0, 22.0), (7.0, 26.0), -0.05, false),
    view("start-room-door", (-4.0, 0.0, 20.0), (-14.0, 20.0), -0.02, false),
    view("gazebo-door", (5.0, 0.0, 2.0), (5.0, -4.0), 0.0, false),
    view("box", (-13.5, 0.0, -18.5), (-13.5, -14.65), -0.2, false),
    view("perk-juggernog", (-21.5, 0.0, 10.0), (-25.2, 10.0), -0.12, false),
    view("perk-quick-revive", (11.5, 0.0, 21.8), (11.5, 25.2), -0.12, false),
    view("pack-a-punch", (5.0, 0.0, -5.6), (5.0, -9.2), -0.15, false),
    view("power-switch", (22.6, 0.45, -20.0), (25.6, -20.0), 0.0, false),
    view("trap-switch", (-17.8, 0.0, -10.8), (-17.8, -14.0), -0.1, false),
    view("teleporter-pad", (-28.0, 0.0, -41.0), (-28.0, -44.5), -0.4, false),
    view("mainframe", (-10.5, 0.0, 21.5), (-10.5, 25.4), -0.1, false),
    view("route-1-start-to-west-hall", (-4.0, 0.0, 20.0), (-26.0, 20.0), 0.0, true),
    view("route-2-west-hall-to-dining", (-20.0, 0.0, 0.0), (-20.0, -24.0), 0.0, true),
    view("route-3-dining-to-passage", (-12.0, 0.0, -15.0), (-12.0, -36.0), -0.05, true),
    view("route-4-passage-to-workshop", (-12.0, 0.0, -28.0), (-14.0, -48.0), -0.15, true),
    view("route-5-yard-to-start", (0.0, 0.0, 2.0), (0.0, 24.0), 0.0, true),
    view("route-6-yard-to-dining", (-7.0, 0.0, 8.0), (-7.0, -24.0), 0.0, true),
];
const T1: f32 = STOREY;
const T2: f32 = 2.0 * STOREY;

/// The Quarry: a view of each of its 9 Zones, with a look at the pit's rim against the sky, the
/// Incline's rock cutting, and the Headframe's deck.
const QUARRY: &[View] = &[
    view("pump-house", (5.0, 0.0, -22.0), (-6.0, -29.0), -0.1, false),
    view("quarry-floor", (-15.0, 0.0, -17.0), (10.0, -2.0), 0.05, false),
    view("quarry-floor-east", (17.0, 0.0, -17.0), (-10.0, -4.0), 0.1, false),
    view("pit-sky", (-12.0, 0.0, -14.0), (6.0, 0.0), 0.5, false),
    view("rail-yard", (3.0, T1, 4.0), (15.0, 14.0), -0.05, false),
    view("crusher-shed", (-19.0, T1, 14.0), (-3.0, 4.0), -0.08, false),
    view("winding-house", (2.0, T2, 18.0), (18.0, 26.0), -0.08, false),
    view("offices", (-2.0, T2, 18.0), (-18.0, 26.0), -0.08, false),
    view("headframe", (0.0, T2, 31.0), (0.0, 40.0), 0.25, false),
    view("headframe-deck", (-4.0, 3.0 * STOREY, 37.0), (5.0, 33.0), -0.1, false),
    view("incline", (-25.0, 0.0, -9.0), (-25.0, 25.0), 0.15, false),
    view("incline-top", (-25.0, T2, 28.0), (-25.0, 0.0), -0.12, false),
    view("jump-pad", (6.0, 0.0, -9.0), (6.0, -3.0), -0.35, false),
    view("loading-dock", (22.0, 0.0, -4.0), (30.0, -18.0), -0.05, false),
    view("loading-dock-wagons", (22.0, 0.0, -18.0), (28.0, -4.0), -0.05, false),
];

/// The Dam: a view of each of its 12 Zones, then a Generator's plate from off it and from on it,
/// the Weapon Light down a gallery, and the Spillway Gallery's plate from behind it.
const DAM: &[View] = &[
    view("crest-road", (-20.0, 8.0, 26.0), (15.0, 26.0), 0.12, false),
    view("crest-sky", (-4.0, 8.0, 25.0), (8.0, 22.0), 0.45, false),
    view("tower-head", (-32.0, 8.0, 21.0), (-44.0, 30.0), -0.12, false),
    view("spillway-gatehouse", (32.0, 8.0, 21.0), (44.0, 30.0), -0.12, false),
    view("gate-shaft", (-41.0, 4.0, 4.0), (-41.0, 17.0), 0.2, false),
    view("intake-gallery", (-41.0, 4.0, -1.0), (-12.0, -1.0), -0.03, false),
    view("control-room", (0.0, 4.0, -5.0), (0.0, 7.0), -0.06, false),
    view("spillway-gallery", (13.0, 4.0, -1.0), (41.0, -1.0), -0.03, false),
    view("penstock-tunnel", (0.0, 0.0, -26.0), (0.0, -8.0), 0.1, false),
    view("turbine-hall", (-8.0, 0.0, -30.0), (4.0, -52.0), -0.06, false),
    view("switchgear-room", (12.0, 0.0, -46.0), (25.0, -46.0), -0.05, false),
    view("spillway-stair", (41.0, 0.0, -54.0), (41.0, -34.0), 0.12, false),
    view("drainage-gallery", (-25.0, 0.0, -59.0), (35.0, -59.0), -0.02, false),
    view("intake-plate-off", (-35.0, 4.0, -22.0), (-40.5, -22.0), -0.35, false),
    view("intake-plate-on", (-40.5, 4.0, -22.0), (-30.0, -22.0), -0.1, false),
    view("gallery-light", (-39.0, 4.0, -10.0), (-39.0, -30.0), -0.05, false),
    view("gate-trap-switch", (-38.8, 4.0, -1.5), (-38.8, 2.0), -0.1, false),
    view("spillway-plate-off", (35.0, 4.0, -26.0), (40.5, -26.0), -0.35, false),
];

/// The Village: a view of each of its 14 Zones, then the Market's cross and a lane from end to end.
const VILLAGE: &[View] = &[
    view("inn", (0.0, 0.0, 8.0), (0.0, 22.0), -0.05, false),
    view("market", (-18.0, 0.0, 2.0), (6.0, -14.0), 0.0, false),
    view("market-fronts", (12.0, 0.0, -14.0), (-6.0, 6.0), 0.12, false),
    view("west-end", (-26.0, 0.0, 0.0), (-70.0, 0.0), 0.0, false),
    view("east-end", (26.0, 0.0, 0.0), (70.0, 0.0), 0.0, false),
    view("back-lane", (-20.0, 0.0, 26.0), (22.0, 26.0), 0.0, false),
    view("mill-lane", (-28.0, 0.0, 26.0), (-70.0, 26.0), 0.0, false),
    view("church-lane", (28.0, 0.0, 26.0), (70.0, 26.0), 0.0, false),
    view("mill", (-54.0, 0.0, 8.0), (-68.0, 20.0), -0.05, false),
    view("smithy", (-25.0, 0.0, 8.0), (-38.0, 20.0), -0.05, false),
    view("bakery", (-9.0, 0.0, 8.0), (-22.0, 20.0), -0.05, false),
    view("schoolhouse", (25.0, 0.0, 8.0), (38.0, 20.0), -0.05, false),
    view("church", (53.0, 0.0, 14.0), (70.0, 14.0), 0.0, false),
    view("orchard", (-44.0, 0.0, 32.0), (-66.0, 50.0), 0.0, false),
    view("graveyard", (44.0, 0.0, 32.0), (66.0, 50.0), 0.0, false),
    view("sky", (0.0, 0.0, -4.0), (6.0, -16.0), 0.6, false),
    view("lamp-row", (-30.0, 0.0, 0.0), (40.0, 0.0), 0.05, false),
];

const COVE: &[View] = &[
    view("strand-start", (-1.0, 0.0, -8.0), (-1.0, 6.0), 0.3, false),
    view("sea-over-the-parapet", (-26.0, 0.0, -9.0), (-26.0, -40.0), 0.12, false),
    view("surf-edge", (-9.0, 0.0, -2.0), (-4.0, -13.0), -0.25, false),
    view("strand-east", (14.0, 0.0, -9.0), (-15.0, 0.0), 0.0, false),
    view("shingle", (-28.0, 0.0, -9.0), (-40.0, 4.0), 0.1, false),
    view("sea-cave", (-47.0, 0.0, 3.0), (-50.0, -10.0), -0.1, false),
    view("up-the-cliff", (-10.0, 0.0, -12.0), (-10.0, 40.0), 0.5, false),
    view("fish-market", (-14.0, 4.0, 10.0), (4.0, 22.0), -0.05, false),
    view("piazza", (-8.0, 8.0, 28.0), (-24.0, 36.0), -0.05, false),
    view("chapel", (-33.0, 8.0, 28.0), (-50.0, 40.0), -0.05, false),
    view("lighthouse-yard", (-14.0, 12.0, 48.0), (-35.0, 58.0), 0.0, false),
    view("lighthouse", (-42.0, 12.0, 52.0), (-52.0, 60.0), 0.3, false),
    view("battery", (10.0, 12.0, 50.0), (27.0, 58.0), 0.0, false),
    view("boathouse", (22.0, 0.0, 3.0), (30.0, -8.0), -0.05, false),
    view("washhouse", (-46.0, 4.0, 9.0), (-52.0, 20.0), -0.05, false),
    view("lower-lane", (-42.0, 4.0, 10.0), (-24.0, 14.0), -0.03, false),
    view("taverna", (9.0, 4.0, 12.0), (30.0, 12.0), -0.05, false),
    view("upper-lane", (-2.0, 8.0, 27.0), (14.0, 38.0), -0.03, false),
    view("pensione", (20.0, 8.0, 38.0), (30.0, 27.0), -0.05, false),
    view("keepers-cottage", (-10.0, 12.0, 48.0), (2.0, 58.0), -0.05, false),
    view("sea-from-the-battery", (20.0, 12.0, 52.0), (20.0, -40.0), 0.02, false),
    view("lantern-gallery", (-42.2, 20.0, 60.0), (-50.0, 50.0), -0.3, false),
    view("lighthouse-line-top", (-42.2, 20.0, 60.0), (-31.0, 61.5), -0.15, false),
    view("lighthouse-line-landing", (-24.0, 12.0, 52.0), (-40.0, 60.0), 0.3, false),
    view("belfry-line-top", (-42.0, 16.0, 41.4), (-17.0, 35.0), -0.2, false),
    view("belfry-line-landing", (-6.0, 8.0, 28.0), (-41.0, 41.0), 0.45, false),
    view("pensione-line-top", (32.5, 12.0, 27.0), (32.5, 12.0), -0.4, false),
    view("pergola-deck", (29.0, 4.0, 9.0), (32.5, 20.0), 0.35, false),
];

/// The Polder: a view of each of its 16 Zones, then its bridges, the lock gates, the water and the dyke.
const POLDER: &[View] = &[
    view("town-square-pump", (-12.0, 0.0, -34.0), (-4.0, -24.0), -0.05, false),
    view("town-square-canal", (-6.0, 0.0, -16.0), (-6.0, 8.0), 0.05, false),
    view("weigh-house", (-54.0, 0.0, -36.0), (-32.0, -20.0), -0.05, false),
    view("brewery", (-68.0, 0.0, -50.0), (-68.0, -20.0), -0.05, false),
    view("humpback-bridge", (-68.0, 0.0, -21.0), (-68.0, 8.0), 0.1, false),
    view("cheese-market", (22.0, 0.0, -36.0), (44.0, -18.0), -0.05, false),
    view("almshouse-court", (-22.0, 0.0, -44.0), (30.0, -50.0), -0.02, false),
    view("windmill", (-62.0, 0.0, 10.0), (-70.0, 34.0), 0.2, false),
    view("windmill-stage", (-68.0, 4.0, 27.0), (-68.0, 35.0), -0.05, false),
    view("warehouse", (-54.0, 0.0, 22.0), (-40.0, 6.0), -0.05, false),
    view("lock-yard", (-30.0, 0.0, 3.0), (-13.0, 12.0), 0.0, false),
    view("sluice-house", (-4.0, 0.0, 2.5), (0.0, 10.0), -0.05, false),
    view("pack-a-punch", (0.0, 0.0, 15.5), (0.0, 8.0), -0.1, false),
    view("toll-quay", (16.0, 0.0, 4.0), (30.0, 18.0), 0.0, false),
    view("shipyard", (38.0, 0.0, 3.0), (48.0, 14.0), 0.0, false),
    view("coal-yard", (62.0, 0.0, 4.0), (70.0, 30.0), 0.05, false),
    view("herring-quay", (-54.0, 0.0, 38.0), (-10.0, 42.0), 0.0, false),
    view("customs-house", (16.0, 0.0, 38.0), (55.0, 42.0), 0.0, false),
    view("dyke-road", (-76.0, 4.0, 52.0), (20.0, 52.0), 0.0, false),
    view("pumping-station", (48.0, 4.0, 52.0), (78.0, 56.0), 0.0, false),
    view("sea-from-the-dyke", (-20.0, 4.0, 56.0), (-20.0, 66.0), 0.1, false),
    view("market-bridge", (-22.0, 0.0, -18.0), (-22.0, 6.0), 0.0, false),
    view("market-bridge-side", (-16.0, 0.0, -14.0), (-22.0, -5.0), -0.12, false),
    view("market-lever", (-25.0, 0.0, -18.0), (-25.0, -12.5), 0.0, false),
    view("herring-bridge", (-24.0, 0.0, 18.0), (-24.0, 40.0), 0.0, false),
    view("ford-f1-stair", (-43.0, 0.0, -14.0), (-43.0, 0.0), -0.45, false),
    view("inland-gate", (-22.0, 0.0, 2.5), (-8.0, 2.5), -0.1, false),
    view("sea-gate", (22.0, 0.0, 21.5), (8.0, 21.5), -0.1, false),
    view("harbour", (0.0, 0.0, 38.0), (-40.0, 30.0), 0.0, false),
    view("town-canal", (-6.0, 0.0, -14.0), (40.0, -6.0), 0.0, false),
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
    /// Whether to Hold every Generator, so a Map with Darkness is lit.
    power: bool,
    /// Whether to throw the Power Switch, so a Map's machines are lit (`ZOMBIES_TOUR_POWER=on`).
    switched: bool,
    /// The Tide phase to hold the water at (`ZOMBIES_TOUR_TIDE=low|rising|high|falling`).
    tide: Option<crate::maps::tide::TidePhase>,
    shot: usize,
    settle: u32,
}

impl Tour {
    /// The tour asked for by `ZOMBIES_TOUR`, if any.
    pub fn from_env() -> Option<Self> {
        let name = std::env::var("ZOMBIES_TOUR").ok()?;
        let (map, views) = match name.to_ascii_lowercase().as_str() {
            "greybox" => (MapId::Greybox, GREYBOX),
            "dam" => (MapId::Dam, DAM),
            "cove" => (MapId::Cove, COVE),
            "village" => (MapId::Village, VILLAGE),
            "quarry" => (MapId::Quarry, QUARRY),
            "polder" => (MapId::Polder, POLDER),
            other => panic!("ZOMBIES_TOUR={other}: only greybox, dam, cove, village, quarry and polder have views"),
        };
        let dir = std::env::var("ZOMBIES_TOUR_OUT").unwrap_or_else(|_| "tour".into());
        std::fs::create_dir_all(&dir).expect("ZOMBIES_TOUR_OUT is writable");
        let switched = std::env::var("ZOMBIES_TOUR_POWER").is_ok_and(|power| power.eq_ignore_ascii_case("on"));
        let power = map == MapId::Dam && !std::env::var("ZOMBIES_TOUR_POWER").is_ok_and(|power| power.eq_ignore_ascii_case("off"));
        use crate::maps::tide::TidePhase;
        let tide = std::env::var("ZOMBIES_TOUR_TIDE").ok().map(|phase| match phase.to_ascii_lowercase().as_str() {
            "low" => TidePhase::Low,
            "rising" => TidePhase::Rising,
            "high" => TidePhase::High,
            "falling" => TidePhase::Falling,
            other => panic!("ZOMBIES_TOUR_TIDE={other}: low, rising, high or falling"),
        });
        Some(Self { map, views, dir, frames: 0, power, switched, tide, shot: 0, settle: 0 })
    }
}

pub struct TourPlugin(pub Tour);

impl Plugin for TourPlugin {
    fn build(&self, app: &mut App) {
        let tour = Tour { dir: self.0.dir.clone(), ..self.0 };
        app.insert_resource(tour)
            .add_systems(Startup, set_window)
            .add_systems(OnEnter(GameState::MainMenu), start_run)
            .add_systems(Update, (clear, shoot, give_boons, hold_generators, throw_switch, hold_tide).run_if(in_state(GameState::Playing)));
    }
}

/// Holds the Tide at the phase `ZOMBIES_TOUR_TIDE` asks for, so a shot shows the water there.
fn hold_tide(tour: Res<Tour>, mut tide: ResMut<crate::maps::tide::Tide>) {
    if let Some(phase) = tour.tide {
        *tide = crate::maps::tide::Tide { phase, turn: crate::maps::tide::TurnState::Still };
    }
}

fn set_window(mut window: Single<&mut Window, With<PrimaryWindow>>) {
    window.resolution.set_scale_factor_override(Some(1.0));
    window.resolution.set(1920.0, 1080.0);
}

fn start_run(tour: Res<Tour>, mut map: ResMut<MapId>, mut mode: ResMut<Mode>, mut next: ResMut<NextState<GameState>>) {
    *map = tour.map;
    *mode = if std::env::var("ZOMBIES_TOUR_MODE").is_ok_and(|mode| mode.eq_ignore_ascii_case("gauntlet")) {
        Mode::Gauntlet
    } else {
        Mode::Classic
    };
    next.set(GameState::Playing);
}

/// In a Gauntlet tour, a dozen Boons for the Player once, so the HUD's corner has something in it.
fn give_boons(mode: Res<Mode>, mut done: Local<bool>, mut players: Query<&mut HeldBoons, With<LocalPlayer>>) {
    if *done || *mode != Mode::Gauntlet {
        return;
    }
    for mut held in &mut players {
        for boon in BOONS.iter().take(12) {
            held.add(boon);
        }
        *done = true;
    }
}

/// In a tour that asks for it, every Generator Held once, so every Zone is Powered.
fn hold_generators(tour: Res<Tour>, mut done: Local<bool>, mut generators: ResMut<Generators>) {
    if *done || !tour.power || !generators.on_map() {
        return;
    }
    for index in 0..generators.states().len() {
        generators.hold(index);
    }
    *done = true;
}

/// In a tour that asks for it, the Power on once, so the machines are lit.
fn throw_switch(tour: Res<Tour>, power: Option<ResMut<Power>>) {
    if let Some(mut power) = power
        && tour.switched
    {
        power.switch_on();
    }
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
