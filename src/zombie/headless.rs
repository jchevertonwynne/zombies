//! Headless Zombies on the real Map, for measuring how they move frame by frame: the physics, the
//! fixed-step movement and the interpolation, with no window. Zombies are interpolated, so their
//! translation may only be written inside the fixed step (see `crate::player::Teleport`): a write
//! from `Update` is undone whenever the next frame runs a physics step. Other modules' tests use
//! this too, adding their own systems to the `App`.

use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use super::*;
use crate::authority::{Authority, AuthorityPlugin};
use crate::modifier::Modifiers;
use crate::player::{LocalPlayer, Player};
use crate::state::{GameState, Pause};

/// A Zombie as one rendered frame shows it.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub pos: Vec3,
    pub fixed_steps: u32,
}

#[derive(Resource, Default)]
struct Steps(u32);

pub struct Sim {
    pub app: App,
    pub frame_secs: f32,
}

impl Sim {
    /// The Map, the Zombie systems, and a stand-in Player at `PLAYER_START`, rendering at `fps`
    /// while physics runs at its fixed rate. `configure` adds anything else under test.
    pub fn new(fps: f64, configure: impl FnOnce(&mut App)) -> Self {
        Self::new_as(Authority::Host, fps, configure)
    }

    /// The same, running as `authority`.
    pub fn new_as(authority: Authority, fps: f64, configure: impl FnOnce(&mut App)) -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            TransformPlugin,
            PhysicsPlugins::default(),
            StatesPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .insert_state(GameState::Playing)
        .insert_state(authority)
        .add_plugins(AuthorityPlugin)
        .add_sub_state::<Pause>()
        .add_message::<PlayerHit>()
        .add_message::<SpecialCue>()
        .add_message::<NovaCrawlerKilled>()
        .init_resource::<ActivePowerUps>()
        .insert_resource(NavGraph::new())
        .insert_resource(Barriers::default())
        .init_resource::<Steps>()
        .add_plugins(ZombiePlugin)
        .add_systems(FixedUpdate, |mut steps: ResMut<Steps>| steps.0 += 1)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / fps)));
        configure(&mut app);
        app.finish();
        app.cleanup();
        app.world_mut().run_system_once(crate::map::spawn_map).unwrap();
        app.world_mut().run_system_once(load_assets).unwrap();
        let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
        app.world_mut().spawn((
            Player,
            LocalPlayer,
            Modifiers::default(),
            crate::zone::PlayerZone::default(),
            Transform::from_translation(crate::map::PLAYER_START + Vec3::Y * half_height),
        ));
        let mut sim = Self {
            app,
            frame_secs: (1.0 / fps) as f32,
        };
        // Let the colliders settle in.
        sim.run(10);
        sim
    }

    /// A Walker standing at `feet`, or, given `rise` seconds, a Riser buried under it.
    pub fn spawn(&mut self, feet: Vec3, rise: Option<f32>) -> Entity {
        self.app
            .world_mut()
            .run_system_once(move |mut commands: Commands, assets: ZombieAssetsParam| {
                spawn_zombie(&mut commands, &assets, feet, 1000.0, SpeedTier::Walker, rise)
            })
            .unwrap()
    }

    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    pub fn send<M: Message>(&mut self, message: M) {
        self.app.world_mut().write_message(message);
    }

    /// One rendered frame, and where `entity` shows in it.
    pub fn frame(&mut self, entity: Entity) -> Sample {
        self.app.world_mut().resource_mut::<Steps>().0 = 0;
        self.app.update();
        let world = self.app.world();
        Sample {
            pos: world.get::<Transform>(entity).unwrap().translation,
            fixed_steps: world.resource::<Steps>().0,
        }
    }

    pub fn run(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
        }
    }

    /// `frames` rendered frames of `entity`, stopping early if it's gone.
    pub fn track(&mut self, entity: Entity, frames: usize) -> Vec<Sample> {
        let mut samples = Vec::new();
        for _ in 0..frames {
            if self.app.world().get_entity(entity).is_err() {
                break;
            }
            samples.push(self.frame(entity));
        }
        samples
    }

    pub fn frames(&self, secs: f32) -> usize {
        (secs / self.frame_secs).round() as usize
    }
}

#[allow(dead_code, reason = "for measuring by hand with --nocapture")]
pub fn print(label: &str, samples: &[Sample], frame_secs: f32) {
    println!("--- {label}");
    let mut prev = samples[0].pos;
    for (i, s) in samples.iter().enumerate() {
        let d = s.pos - prev;
        println!(
            "{i:4} steps={} pos=({:7.3},{:7.3},{:7.3}) speed={:6.2} vy={:6.2}",
            s.fixed_steps,
            s.pos.x,
            s.pos.y,
            s.pos.z,
            d.with_y(0.0).length() / frame_secs,
            d.y / frame_secs,
        );
        prev = s.pos;
    }
}

/// Where a Zombie standing on the floor at height `floor` has its centre.
pub fn standing(floor: f32) -> f32 {
    floor + body_half_height() + 0.02
}

const FRAME_RATES: [f64; 3] = [144.0, 60.0, 90.0];

/// A Riser at one of the Yard's Riser points, with the Player out of the way in the Start Room.
const RISER_AT: Vec3 = Vec3::new(10.0, 0.0, 10.0);

#[test]
#[ignore = "prints a measurement; asserts nothing"]
fn measure() {
    for fps in [144.0, 60.0] {
        let mut sim = Sim::new(fps, |_| {});
        let zombie = sim.spawn(RISER_AT, Some(ZOMBIE_RISE_SECS));
        let frames = sim.frames(ZOMBIE_RISE_SECS + 0.3);
        let samples = sim.track(zombie, frames);
        print(&format!("{fps} fps: a Riser climbing out"), &samples, sim.frame_secs);

        let walking = sim.track(zombie, sim.frames(0.5));
        print(&format!("{fps} fps: walking"), &walking, sim.frame_secs);
        sim.send(DamageZombie {
            player: None,
            zombie,
            damage: 1e6,
            zone: HitZone::Torso,
            attack: Attack::Bullet,
        });
        let dying = sim.track(zombie, sim.frames(ZOMBIE_FALL_SECS + 0.3));
        print(&format!("{fps} fps: falling over"), &dying, sim.frame_secs);

        let crawler = sim.spawn(Vec3::new(4.0, 0.0, 10.0), None);
        sim.run(sim.frames(0.5));
        sim.send(Gibbed { zombie: crawler });
        let lowered = sim.track(crawler, sim.frames(0.4));
        print(&format!("{fps} fps: losing its legs"), &lowered, sim.frame_secs);
    }
}

/// A Riser used to be raised from `Update`, where the step undid each frame's rise until the next
/// write. It climbs steadily out of the floor and stays standing once out, at any frame rate.
#[test]
fn a_riser_climbs_out_steadily_and_stays_up() {
    for fps in FRAME_RATES {
        let mut sim = Sim::new(fps, |_| {});
        let zombie = sim.spawn(RISER_AT, Some(ZOMBIE_RISE_SECS));
        let samples = sim.track(zombie, sim.frames(ZOMBIE_RISE_SECS + 0.5));
        let top = standing(RISER_AT.y);
        let climb = top - samples[0].pos.y;
        // Never sinks back on the way up, and never jumps more than its climb's speed allows in
        // one frame. (Once out, it settles the last 2 cm onto the floor, like any new Zombie.)
        let most = climb / ZOMBIE_RISE_SECS * sim.frame_secs * 1.5;
        let out = (0..samples.len()).max_by(|&a, &b| samples[a].pos.y.total_cmp(&samples[b].pos.y)).unwrap();
        for w in samples[..=out].windows(2) {
            let dy = w[1].pos.y - w[0].pos.y;
            assert!(dy > -1e-3 && dy < most, "{fps} fps: {} to {}", w[0].pos.y, w[1].pos.y);
        }
        // Out, and still standing once it walks off.
        let last = samples.last().unwrap();
        assert!((last.pos.y - top).abs() < 0.02, "{fps} fps: ended at {}", last.pos.y);
        let walking = sim.track(zombie, sim.frames(0.5));
        assert!(walking.iter().all(|s| (s.pos.y - top).abs() < 0.02), "{fps} fps: lost its footing");
    }
}

/// A dying Zombie used to sink from `Update`, so the step undid part of its fall and the body was
/// left floating. It lies on the floor at the end, its fall never reversing.
#[test]
fn a_killed_zombie_falls_all_the_way_to_the_floor() {
    for fps in FRAME_RATES {
        let mut sim = Sim::new(fps, |_| {});
        let zombie = sim.spawn(Vec3::new(0.0, 0.0, 12.0), None);
        sim.run(sim.frames(0.5));
        let killed_at = sim.frame(zombie).pos;
        sim.send(DamageZombie {
            player: None,
            zombie,
            damage: 1e6,
            zone: HitZone::Torso,
            attack: Attack::Bullet,
        });
        let samples = sim.track(zombie, sim.frames(ZOMBIE_FALL_SECS + 0.3));
        // Sinking evenly all the way: turning it over every frame mustn't freeze the sink between
        // physics steps either.
        let drop = body_half_height() - ZOMBIE_RADIUS;
        let per_frame = drop / ZOMBIE_FALL_SECS * sim.frame_secs;
        let lying = standing(0.0) - drop;
        for w in samples[1..].windows(2).filter(|w| w[1].pos.y > lying + 0.05) {
            let dy = w[0].pos.y - w[1].pos.y;
            assert!((dy - per_frame).abs() < per_frame * 0.2, "{fps} fps: {} to {}", w[0].pos.y, w[1].pos.y);
        }
        let last = samples.last().unwrap();
        assert!((last.pos.y - lying).abs() < 0.02, "{fps} fps: ended at {}, not {lying}", last.pos.y);
        // It falls where it was killed, give or take the last stride.
        assert!(last.pos.with_y(0.0).distance(killed_at.with_y(0.0)) < 0.1, "{fps} fps: slid to {}", last.pos);
    }
}

/// Losing its legs used to lower the body from `Update`, which the next step put straight back, so
/// the Crawler hung in the air and dropped. It's down at once, and stays down.
#[test]
fn a_zombie_that_loses_its_legs_drops_at_once() {
    for fps in FRAME_RATES {
        for offset in 0..3 {
            let mut sim = Sim::new(fps, |_| {});
            let zombie = sim.spawn(Vec3::new(4.0, 0.0, 10.0), None);
            sim.run(sim.frames(0.5) + offset);
            sim.send(Gibbed { zombie });
            let samples = sim.track(zombie, sim.frames(0.3));
            let low = standing(0.0) - (body_half_height() - CRAWLER_RADIUS);
            for s in &samples {
                assert!((s.pos.y - low).abs() < 0.03, "{fps} fps, offset {offset}: a frame at {}", s.pos.y);
            }
        }
    }
}

/// Stands the stand-in Player with their feet at `feet`.
fn put_player(sim: &mut Sim, feet: Vec3) {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    sim.world_mut()
        .query_filtered::<&mut Transform, With<Player>>()
        .single_mut(sim.app.world_mut())
        .unwrap()
        .translation = feet + Vec3::Y * half_height;
}

/// West Hall's ramp is the only way up to the Balcony, so a Zombie after a Player up there has to
/// climb it.
#[test]
fn a_zombie_walks_up_the_ramp_to_a_player_on_the_balcony() {
    let mut sim = Sim::new(60.0, |_| {});
    put_player(&mut sim, Vec3::new(-12.5, 4.0, -2.0));
    let zombie = sim.spawn(Vec3::new(-15.7, 0.0, 14.0), None);
    // Stops following it if the stuck rule despawns it on the way.
    let (mut top, mut last) = (f32::MIN, Vec3::ZERO);
    for _ in 0..sim.frames(40.0) {
        sim.run(1);
        let Some(at) = sim.world_mut().get::<Transform>(zombie).map(|t| t.translation) else { break };
        (top, last) = (top.max(at.y), at);
    }
    assert!(top > standing(0.0) + 3.5, "got up to {top}, ended at {last}");
}

/// The Raised Floor in `zone`.
fn raised_floor_in(zone: crate::zone::ZoneId) -> crate::map::RaisedFloor {
    *crate::map::RAISED_FLOORS.iter().find(|raised| raised.zone == zone).unwrap()
}

/// Follows `zombie` for up to `secs`, until `there` says it has arrived; whether it did, and the
/// last place it was seen.
fn arrives(sim: &mut Sim, zombie: Entity, secs: f32, there: impl Fn(Vec3) -> bool) -> (bool, Vec3) {
    let mut last = Vec3::ZERO;
    for _ in 0..sim.frames(secs) {
        sim.run(1);
        let Some(at) = sim.world_mut().get::<Transform>(zombie).map(|t| t.translation) else { break };
        last = at;
        if there(at) {
            return (true, at);
        }
    }
    (false, last)
}

/// A Raised Floor's edge is a Ledge too high to step up: a Zombie straight in front of it, after
/// a Player up on it, goes round by the steps or the ramp instead of pushing at the edge.
#[test]
fn a_zombie_goes_round_by_the_steps_to_a_player_on_a_raised_floor() {
    use crate::zone::ZoneId::*;
    // The Raised Floor, where the Zombie starts on the floor facing its edge, and where the
    // Player stands up on it across that edge.
    for (zone, zombie_at, player_at) in [
        (EastHall, Vec2::new(20.6, 13.3), Vec2::new(24.3, 13.3)),
        (PowerRoom, Vec2::new(24.5, -15.6), Vec2::new(24.5, -18.2)),
        (Workshop, Vec2::new(-21.0, -42.2), Vec2::new(-21.0, -39.4)),
    ] {
        let raised = raised_floor_in(zone);
        let on = |at: Vec2| at.cmpgt(raised.min).all() && at.cmplt(raised.max).all();
        assert!(on(player_at) && !on(zombie_at), "{zone:?}: the setup is off the Raised Floor");
        let mut sim = Sim::new(60.0, |_| {});
        put_player(&mut sim, Vec3::new(player_at.x, raised.height, player_at.y));
        let zombie = sim.spawn(Vec3::new(zombie_at.x, 0.0, zombie_at.y), None);
        let (up, last) = arrives(&mut sim, zombie, 15.0, |at| {
            at.y > standing(raised.height) - 0.05 && on(Vec2::new(at.x, at.z))
        });
        assert!(up, "{zone:?}: never got up; last seen at {last}");
    }
}

/// The Prop of `kind` in `zone` whose footprint covers `at` (x, z).
fn prop_at(kind: crate::map::PropKind, zone: crate::zone::ZoneId, at: Vec2) -> crate::map::Prop {
    *crate::map::PROPS
        .iter()
        .find(|prop| prop.kind == kind && prop.zone == zone && (Vec2::new(prop.at.x, prop.at.z) - at).abs().cmple(Vec2::new(prop.size.x, prop.size.z) / 2.0).all())
        .unwrap()
}

/// A Zombie after a Player on the far side of a dining table goes round the table rather than
/// pushing at it.
#[test]
fn a_zombie_goes_round_a_dining_table_to_the_player() {
    use crate::map::PropKind;
    let table = prop_at(PropKind::Table, crate::zone::ZoneId::DiningRoom, Vec2::new(-9.0, -17.9));
    let (north, south) = (table.at.z + table.size.z / 2.0, table.at.z - table.size.z / 2.0);
    let mut sim = Sim::new(60.0, |_| {});
    let player = Vec3::new(table.at.x, 0.0, south - 2.0);
    put_player(&mut sim, player);
    let zombie = sim.spawn(Vec3::new(table.at.x, 0.0, north + 1.5), None);
    let (round, last) = arrives(&mut sim, zombie, 15.0, |at| Vec2::new(at.x - player.x, at.z - player.z).length() < ZOMBIE_REACH);
    assert!(round, "never got round the table; last seen at {last}");
}

/// The Yard's sunken garden has Ledges all round but for its steps: a Zombie down in it, after a
/// Player just over the Ledge, climbs out by the steps.
#[test]
fn a_zombie_climbs_out_of_the_sunken_garden_by_its_steps() {
    let garden = *crate::map::SUNKEN.iter().find(|sunken| sunken.zone == crate::zone::ZoneId::Yard).unwrap();
    let x = (garden.min.x + garden.max.x) / 2.0;
    let mut sim = Sim::new(60.0, |_| {});
    let player = Vec3::new(x, 0.0, garden.max.y + 1.1);
    put_player(&mut sim, player);
    let zombie = sim.spawn(Vec3::new(x, -garden.depth, garden.max.y - 1.0), None);
    let (out, last) = arrives(&mut sim, zombie, 20.0, |at| {
        at.y > standing(0.0) - 0.05 && Vec2::new(at.x - player.x, at.z - player.z).length() < ZOMBIE_REACH
    });
    assert!(out, "never climbed out to the Player; last seen at {last}");
}

/// The Yard's sandbag wall is about the highest a Player can clamber onto, and still only a metre
/// up: Zombies come to it and swipe the Player on top from beside it.
#[test]
fn a_zombie_swipes_a_player_up_on_the_yard_sandbags() {
    let sandbags = prop_at(crate::map::PropKind::Sandbags, crate::zone::ZoneId::Yard, Vec2::new(-2.0, -12.5));
    let mut sim = Sim::new(60.0, |_| {});
    put_player(&mut sim, sandbags.at + Vec3::Y * sandbags.size.y);
    sim.spawn(Vec3::new(sandbags.at.x, 0.0, sandbags.at.z + 5.0), None);
    let mut hits = 0;
    for _ in 0..sim.frames(20.0) {
        sim.run(1);
        hits += sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().count();
    }
    assert!(hits >= 1, "the Zombie never landed a swipe");
}

/// The Pack-a-Punch Gazebo's west wall is thin: a Zombie just outside it, in reach of a Player
/// just inside, must never land a swipe through it.
#[test]
fn a_zombie_cannot_swipe_a_player_through_the_gazebo_wall() {
    let mut sim = Sim::new(60.0, |_| {});
    put_player(&mut sim, Vec3::new(2.6, 0.0, -7.0));
    sim.spawn(Vec3::new(1.5, 0.0, -7.0), None);
    let mut hits = 0;
    for _ in 0..sim.frames(5.0) {
        sim.run(1);
        hits += sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().count();
    }
    assert_eq!(hits, 0, "the Zombie hit the Player through the wall");
}

/// A second Player standing at `feet`.
fn add_player(sim: &mut Sim, feet: Vec3) -> Entity {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    sim.world_mut()
        .spawn((
            Player,
            Modifiers::default(),
            crate::zone::PlayerZone::default(),
            Transform::from_translation(feet + Vec3::Y * half_height),
        ))
        .id()
}

fn hit_players(sim: &mut Sim, secs: f32) -> Vec<Entity> {
    let mut hit = Vec::new();
    for _ in 0..sim.frames(secs) {
        sim.run(1);
        hit.extend(sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().map(|hit| hit.player));
    }
    hit
}

/// With two Players up, a Zombie goes after the nearer, and the swipe names them.
#[test]
fn a_zombie_swipes_the_nearer_of_two_players() {
    let mut sim = Sim::new(60.0, |_| {});
    put_player(&mut sim, Vec3::new(11.0, 0.0, -2.0));
    let near = add_player(&mut sim, Vec3::new(-5.0, 0.0, 0.0));
    sim.spawn(Vec3::new(0.0, 0.0, 0.0), None);
    let hit = hit_players(&mut sim, 20.0);
    assert!(!hit.is_empty(), "the Zombie never landed a swipe");
    assert!(hit.iter().all(|&player| player == near), "it swiped the farther Player");
}

/// A Player who goes down is left alone at once, and the Zombie turns on the other.
#[test]
fn a_zombie_leaves_a_player_who_goes_down_for_the_other() {
    let mut sim = Sim::new(60.0, |_| {});
    let far = sim.world_mut().query_filtered::<Entity, With<Player>>().single(sim.app.world()).unwrap();
    put_player(&mut sim, Vec3::new(11.0, 0.0, -2.0));
    let near = add_player(&mut sim, Vec3::new(-5.0, 0.0, 0.0));
    sim.spawn(Vec3::new(-2.0, 0.0, 0.0), None);
    sim.run(sim.frames(0.5));
    let stand = crate::last_stand::LastStand::begin(&mut crate::weapon::Loadout::new(crate::arsenal::WeaponId::M1911));
    sim.world_mut().entity_mut(near).insert(stand);
    let hit = hit_players(&mut sim, 30.0);
    assert!(!hit.is_empty(), "the Zombie never landed a swipe");
    assert!(hit.iter().all(|&player| player == far), "it swiped the Player in Last Stand, or never turned");
}

/// How far a Zombie walks in three seconds of a Run whose Player has the Pause menu open, and
/// whether time stopped.
fn walked_while_paused(run_type: crate::state::RunType) -> (f32, bool) {
    use crate::state::{pause_time, solo};
    let mut sim = Sim::new(60.0, |app| {
        app.insert_resource(run_type)
            .add_systems(OnEnter(Pause::Paused), pause_time.run_if(solo));
    });
    let zombie = sim.spawn(Vec3::new(-3.0, 0.0, 8.0), None);
    let start = sim.world_mut().get::<Transform>(zombie).unwrap().translation;
    sim.world_mut().resource_mut::<NextState<Pause>>().set(Pause::Paused);
    sim.run(sim.frames(3.0));
    let at = sim.world_mut().get::<Transform>(zombie).unwrap().translation;
    let time_paused = sim.world_mut().resource::<Time<Virtual>>().is_paused();
    (at.distance(start), time_paused)
}

/// In Co-op the Pause menu is only an overlay: time and the Zombies go on.
#[test]
fn a_co_op_run_advances_while_a_player_is_paused() {
    let (walked, time_paused) = walked_while_paused(crate::state::RunType::Coop);
    assert!(!time_paused, "Co-op Pause stopped time");
    assert!(walked > 1.0, "the Zombie only walked {walked}");
}

/// Solo Pause stops the world.
#[test]
fn a_solo_run_stands_still_while_paused() {
    let (walked, time_paused) = walked_while_paused(crate::state::RunType::Solo);
    assert!(time_paused);
    assert!(walked < 0.1, "the Zombie walked {walked}");
}

/// Three Zombies that vault the same window together must land apart, then go on after the
/// Player, rather than ending up inside each other and stuck.
#[test]
fn zombies_that_vault_a_window_together_land_apart_and_move_on() {
    use crate::barrier::{Barriers, Entering, PlankTorn, enter_through_windows};
    use crate::map::{BarrierId, WINDOWS, window_spawn};

    let barrier = BarrierId(0);
    let mut sim = Sim::new(60.0, |app| {
        app.add_message::<PlankTorn>()
            .add_message::<crate::audio::PlayCue>()
            .insert_resource(crate::round::Round::shown(1, None))
            .add_systems(FixedUpdate, enter_through_windows);
    });
    // Open the window.
    for _ in 0..crate::tuning::BARRIER_PLANKS {
        sim.world_mut().resource_mut::<Barriers>().tear(barrier);
    }
    let centre = WINDOWS[barrier.0 as usize].1;
    let spawn = window_spawn(barrier);
    let zombies: Vec<Entity> = [-0.55, 0.0, 0.55]
        .into_iter()
        .map(|along| {
            // The window faces +Z, so along the wall is X.
            let zombie = sim.spawn(spawn + Vec3::X * along, None);
            sim.world_mut().entity_mut(zombie).insert(Entering::new(barrier));
            zombie
        })
        .collect();
    // Long enough to walk to their spots, vault, and set off after the Player.
    sim.run(sim.frames(6.0));

    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    let at: Vec<Vec3> = zombies
        .iter()
        .map(|&z| sim.world_mut().get::<Transform>(z).expect("zombie still alive").translation)
        .collect();
    for (i, a) in at.iter().enumerate() {
        assert!(flat(*a).distance(centre) > 2.5, "zombie {i} is still at the window: {a} (window {centre})");
        for (j, b) in at.iter().enumerate().skip(i + 1) {
            let apart = flat(*a).distance(flat(*b));
            assert!(apart >= ZOMBIE_RADIUS * 2.0 - 0.05, "zombies {i} and {j} overlap, {apart} apart: {a} and {b}");
        }
    }
}

/// Opens window 0 and sends one Zombie in through it (a Crawler if `crawler`): where its body
/// stood before the vault, every frame's position, and the window's centre and way out.
fn vault_window_0(crawler: bool) -> (f32, Vec<Vec3>, Vec3, Vec3) {
    use crate::barrier::{Barriers, Entering, PlankTorn, enter_through_windows};
    use crate::map::{BarrierId, WINDOWS, window_spawn};

    let barrier = BarrierId(0);
    let mut sim = Sim::new(60.0, |app| {
        app.add_message::<PlankTorn>()
            .add_message::<crate::audio::PlayCue>()
            .insert_resource(crate::round::Round::shown(1, None))
            .add_systems(FixedUpdate, enter_through_windows);
    });
    for _ in 0..crate::tuning::BARRIER_PLANKS {
        sim.world_mut().resource_mut::<Barriers>().tear(barrier);
    }
    let zombie = sim.spawn(window_spawn(barrier), None);
    sim.run(sim.frames(0.5));
    if crawler {
        sim.send(Gibbed { zombie });
        sim.run(sim.frames(0.5));
    }
    let standing = sim.world_mut().get::<Transform>(zombie).unwrap().translation.y;
    sim.world_mut().entity_mut(zombie).insert(Entering::new(barrier));
    let path = sim.track(zombie, sim.frames(6.0)).into_iter().map(|sample| sample.pos).collect();
    let (_, centre, out) = WINDOWS[barrier.0 as usize];
    (standing, path, Vec3::new(centre.x, 0.0, centre.y), Vec3::new(out.x, 0.0, out.y))
}

/// A Zombie, and a Crawler, get in over the Sill: never through it, and on into the room.
#[test]
fn zombies_and_crawlers_climb_in_over_the_window_sill() {
    for crawler in [false, true] {
        let (standing, path, centre, out) = vault_window_0(crawler);
        // Within this far of the wall line, the body is over the Sill.
        let over_sill = crate::map::WALL_THICKNESS / 2.0 + ZOMBIE_RADIUS;
        let mut crossed = false;
        for at in &path {
            let across = (*at - centre).dot(out);
            if across.abs() <= over_sill {
                crossed = true;
                let feet = at.y - standing;
                assert!(
                    feet >= crate::tuning::WINDOW_SILL_HEIGHT - 0.01,
                    "crawler {crawler}: feet {feet} off the floor {across} m from the wall line"
                );
            }
        }
        assert!(crossed, "crawler {crawler}: never crossed the wall");
        let end = *path.last().unwrap();
        // Past its landing (1.2 m in): it has set off after the Player.
        assert!((end - centre).dot(out) < -2.5, "crawler {crawler}: still at the window, at {end}");
        assert_eq!(crate::nav::zone_at(end + Vec3::Y * 0.1), Some(crate::zone::ZoneId::StartRoom), "crawler {crawler}");
    }
}


/// Every spot on every Zone's floors that a Zombie fits on has a path node near it that it can
/// walk straight to, and one it can be walked to from, so a Zombie there, or a Player chased
/// there, is never cut off from the graph by a Prop, a Ledge or a corner.
#[test]
fn every_open_floor_spot_walks_to_and_from_a_path_node() {
    use crate::map::{PROPS, RAISED_FLOORS, SUNKEN};
    use crate::zone::ZoneId;

    let mut sim = Sim::new(60.0, |_| {});
    let (checked, cut_off): (usize, Vec<String>) = sim
        .world_mut()
        .run_system_once(|spatial: SpatialQuery, graph: Res<NavGraph>| {
            let map = SpatialQueryFilter::from_mask(GameLayer::World);
            let half = body_half_height();
            let probe = Collider::sphere(ZOMBIE_RADIUS * 0.8);
            let body = Collider::capsule(ZOMBIE_RADIUS, ZOMBIE_CAPSULE_LENGTH);
            let mut cut_off = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for zone in ZoneId::ALL {
                let (lo, hi) = zone.volume();
                let ground = if zone == ZoneId::Balcony { 4.0 } else { 0.0 };
                let floors: Vec<f32> = [ground]
                    .into_iter()
                    .chain(RAISED_FLOORS.iter().filter(|raised| raised.zone == zone).map(|raised| raised.height))
                    .chain(SUNKEN.iter().filter(|s| s.zone == zone).map(|s| -s.depth))
                    .collect();
                let mut x = lo.x + 0.75;
                while x < hi.x {
                    let mut z = lo.z + 0.75;
                    while z < hi.z {
                        let on_prop = PROPS.iter().any(|prop| {
                            (Vec2::new(x - prop.at.x, z - prop.at.z).abs() - Vec2::new(prop.size.x, prop.size.z) / 2.0).max_element() < ZOMBIE_RADIUS
                        });
                        for &floor in &floors {
                            let Some(hit) = spatial.cast_ray(Vec3::new(x, floor + 1.0, z), Dir3::NEG_Y, 1.5, true, &map) else {
                                continue;
                            };
                            let feet = Vec3::new(x, floor + 1.0 - hit.distance, z);
                            let centre = feet + Vec3::Y * half;
                            let fits = spatial.shape_intersections(&body, centre + Vec3::Y * 0.05, Quat::IDENTITY, &map).is_empty();
                            if on_prop || !fits || crate::nav::zone_at(feet + Vec3::Y * 0.1) != Some(zone) {
                                continue;
                            }
                            if !seen.insert((feet * 10.0).round().as_ivec3()) {
                                continue;
                            }
                            let walk = |from: Vec3, to: Vec3| clear_walk(&spatial, &probe, from, to, half);
                            let out = graph.nearest_where(feet, |node| walk(centre, node.at + Vec3::Y * half));
                            let back = graph.nearest_where(feet, |node| walk(node.at + Vec3::Y * half, centre));
                            if out.is_none() || back.is_none() {
                                cut_off.push(format!("{} {feet} (out {}, back {})", zone.name(), out.is_some(), back.is_some()));
                            }
                        }
                        z += 1.5;
                    }
                    x += 1.5;
                }
            }
            (seen.len(), cut_off)
        })
        .unwrap();
    assert!(checked > 1000, "only {checked} spots checked");
    assert!(cut_off.is_empty(), "cut off from the path nodes: {cut_off:#?}");
}
