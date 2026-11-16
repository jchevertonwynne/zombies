//! Headless Zombies on the real Map, for measuring how they move frame by frame: the physics, the
//! fixed-step movement and the interpolation, with no window. Zombies are interpolated, so their
//! translation may only be written inside the fixed step (see `crate::players::player::Teleport`): a write
//! from `Update` is undone whenever the next frame runs a physics step. Other modules' tests use
//! this too, adding their own systems to the `App`.

use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use super::*;
use crate::authority::{Authority, AuthorityPlugin};
use crate::maps::map::MapId;
use crate::modifier::Modifiers;
use crate::players::player::{LocalPlayer, Player};
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
        Self::build(authority, MapId::Greybox, fps, configure)
    }

    /// The same on `map`, with the stand-in Player in its first Seat.
    pub fn on(map: MapId, fps: f64, configure: impl FnOnce(&mut App)) -> Self {
        Self::build(Authority::Host, map, fps, configure)
    }

    fn build(authority: Authority, map: MapId, fps: f64, configure: impl FnOnce(&mut App)) -> Self {
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
        .insert_resource(map)
        .insert_resource(NavGraph::new(map))
        .insert_resource(Barriers::default())
        .init_resource::<Steps>()
        .add_plugins(ZombiePlugin)
        .add_systems(FixedUpdate, |mut steps: ResMut<Steps>| steps.0 += 1)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / fps)));
        configure(&mut app);
        app.finish();
        app.cleanup();
        app.world_mut().run_system_once(crate::maps::map::spawn_map).unwrap();
        app.world_mut().run_system_once(load_assets).unwrap();
        let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
        app.world_mut().spawn((
            Player,
            LocalPlayer,
            Modifiers::default(),
            crate::maps::zone::PlayerZone::start(map),
            Transform::from_translation(map.layout().seat_start(0) + Vec3::Y * half_height),
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

/// A Zombie stands up out of the Cove's surf at a Wading Point over `ZOMBIE_WADE_RISE_SECS`
/// (2 s, against a Riser's 1.5), from under the bed. It can be shot, Headshots counting, but
/// doesn't move or swing until it's up, with a Player right beside it; and once up it's slowed by
/// the Shallow Water until it's ashore.
#[test]
fn a_wading_zombie_rises_in_two_seconds_shootable_but_harmless_then_wades() {
    use crate::maps::water::wading;
    let mut sim = Sim::on(MapId::Cove, 60.0, |_| {});
    let layout = MapId::Cove.layout();
    let bed = layout.wading[1].1;
    assert!(wading(layout, Default::default(), bed), "over the bed is Shallow Water");
    // A Player standing right beside the spot, in the surf.
    put_player(&mut sim, bed + Vec3::new(1.0, 0.0, 1.0));
    let zombie = sim.spawn(bed, Some(ZOMBIE_WADE_RISE_SECS));
    let top = standing(bed.y);
    let start = sim.frame(zombie).pos;
    assert!(start.y < bed.y - 0.5, "starts under the bed, at {}", start.y);

    // Shot while it rises: it takes the damage, and is still rising.
    sim.run(sim.frames(0.5));
    let health = sim.world_mut().get::<Zombie>(zombie).unwrap().health();
    sim.send(DamageZombie { player: None, zombie, damage: 50.0, zone: HitZone::Head, attack: Attack::Bullet });
    sim.run(2);
    assert!(sim.world_mut().get::<Zombie>(zombie).unwrap().health() < health, "a Headshot counts");
    assert!(sim.world_mut().get::<Rising>(zombie).is_some());

    // Neither walking nor swinging until it is up, with a Player an arm's length away.
    let samples = sim.track(zombie, sim.frames(ZOMBIE_WADE_RISE_SECS - 0.7));
    for s in &samples {
        assert!((s.pos.x - start.x).abs() < 0.01 && (s.pos.z - start.z).abs() < 0.01, "it moved to {} while rising", s.pos);
        assert!(s.pos.y < top - 0.02, "up at {} with time to spare", s.pos.y);
    }
    assert!(sim.world_mut().get::<Zombie>(zombie).unwrap().wind_up.is_none());
    assert!(sim.world_mut().get::<Rising>(zombie).is_some(), "still rising a little before 2 s");

    // Up by 2 s, standing on the bed.
    sim.run(sim.frames(0.4));
    assert!(sim.world_mut().get::<Rising>(zombie).is_none(), "up after {ZOMBIE_WADE_RISE_SECS} s");
    let up = sim.frame(zombie).pos;
    assert!((up.y - top).abs() < 0.05, "stands at {} not {top}", up.y);
    // Wading: slowed by the water at its feet.
    sim.run(sim.frames(0.3));
    let factor = sim.world_mut().get::<ZombieMoveFactor>(zombie).expect("slowed in Shallow Water");
    assert!((factor.factor() - WADE_SPEED_MULT).abs() < 1e-4);
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
fn raised_floor_in(zone: crate::maps::zone::ZoneId) -> crate::maps::map::RaisedFloor {
    *crate::maps::map::MapId::Greybox.layout().raised_floors.iter().find(|raised| raised.zone == zone).unwrap()
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
    use crate::maps::zone::ZoneId::*;
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
fn prop_at(kind: crate::maps::map::PropKind, zone: crate::maps::zone::ZoneId, at: Vec2) -> crate::maps::map::Prop {
    *crate::maps::map::MapId::Greybox.layout().props
        .iter()
        .find(|prop| prop.kind == kind && prop.zone == zone && (Vec2::new(prop.at.x, prop.at.z) - at).abs().cmple(Vec2::new(prop.size.x, prop.size.z) / 2.0).all())
        .unwrap()
}

/// A Zombie after a Player on the far side of a dining table goes round the table rather than
/// pushing at it.
#[test]
fn a_zombie_goes_round_a_dining_table_to_the_player() {
    use crate::maps::map::PropKind;
    let table = prop_at(PropKind::Table, crate::maps::zone::ZoneId::DiningRoom, Vec2::new(-9.0, -17.9));
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
    let garden = *crate::maps::map::MapId::Greybox.layout().sunken.iter().find(|sunken| sunken.zone == crate::maps::zone::ZoneId::Yard).unwrap();
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
    let sandbags = prop_at(crate::maps::map::PropKind::Sandbags, crate::maps::zone::ZoneId::Yard, Vec2::new(-2.0, -12.5));
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
            crate::maps::zone::PlayerZone::start(MapId::Greybox),
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
    let stand = crate::players::last_stand::LastStand::begin(&mut crate::weapons::weapon::Loadout::new(crate::weapons::arsenal::WeaponId::M1911));
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
    use crate::maps::barrier::{Barriers, Entering, PlankTorn, enter_through_windows};
    use crate::maps::map::{BarrierId, WINDOWS, window_spawn};

    let barrier = BarrierId(0);
    let mut sim = Sim::new(60.0, |app| {
        app.add_message::<PlankTorn>()
            .add_message::<crate::audio::PlayCue>()
            .insert_resource(crate::run::round::Round::shown(1, None))
            .add_systems(FixedUpdate, enter_through_windows);
    });
    // Open the window.
    for _ in 0..crate::tuning::BARRIER_PLANKS {
        sim.world_mut().resource_mut::<Barriers>().tear(barrier);
    }
    let centre = WINDOWS[barrier.0 as usize].1.xz();
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

/// Opens `barrier`'s window on `map` and sends one Zombie in through it (a Crawler if `crawler`):
/// where its body stood before the vault, every frame's position, and the window's centre and way
/// out.
fn vault_window(map: MapId, barrier: crate::maps::map::BarrierId, crawler: bool) -> (f32, Vec<Vec3>, Vec3, Vec3) {
    use crate::maps::barrier::{Barriers, Entering, PlankTorn, enter_through_windows};
    use crate::maps::map::{WINDOWS, window_centre, window_spawn};

    let mut sim = Sim::on(map, 60.0, |app| {
        app.add_message::<PlankTorn>()
            .add_message::<crate::audio::PlayCue>()
            .insert_resource(crate::run::round::Round::shown(1, None))
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
    let (_, _, out) = WINDOWS[barrier.0 as usize];
    (standing, path, window_centre(barrier), Vec3::new(out.x, 0.0, out.y))
}

/// A Zombie, and a Crawler, get in over `barrier`'s Sill into `zone`: never through it, and on
/// into the room.
fn climbs_in_over_the_sill(barrier: crate::maps::map::BarrierId, zone: crate::maps::zone::ZoneId) {
    let map = zone.map();
    for crawler in [false, true] {
        let (standing, path, centre, out) = vault_window(map, barrier, crawler);
        // Within this far of the wall line, the body is over the Sill.
        let over_sill = crate::maps::map::WALL_THICKNESS / 2.0 + ZOMBIE_RADIUS;
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
        assert_eq!(crate::enemies::nav::zone_at(map, end + Vec3::Y * 0.1), Some(zone), "crawler {crawler}");
    }
}

/// A Zombie, and a Crawler, get in over the Sill: never through it, and on into the room.
#[test]
fn zombies_and_crawlers_climb_in_over_the_window_sill() {
    climbs_in_over_the_sill(crate::maps::map::BarrierId(0), crate::maps::zone::ZoneId::StartRoom);
}

/// The Balcony has a window of its own, up at the first storey: Zombies gather on the roof outside
/// it and climb in onto the Balcony, not down to the ground.
#[test]
fn zombies_climb_in_through_the_balcony_window_onto_the_balcony() {
    use crate::maps::map::{BarrierId, WINDOWS};
    use crate::maps::zone::ZoneId;

    let i = WINDOWS.iter().position(|(zone, ..)| *zone == ZoneId::Balcony).expect("the Balcony has a window");
    climbs_in_over_the_sill(BarrierId(i as u8), ZoneId::Balcony);
}

/// The Headframe's deck has a window of its own, onto the lip of the pit: Zombies gather up there
/// and climb in onto the deck, by Pack-a-Punch.
#[test]
fn zombies_climb_in_through_the_deck_window_onto_the_headframe_deck() {
    use crate::maps::map::{BarrierId, WINDOWS};
    use crate::maps::zone::ZoneId;

    let i = WINDOWS.iter().position(|(zone, ..)| *zone == ZoneId::Headframe).expect("the Headframe has a window");
    climbs_in_over_the_sill(BarrierId(i as u8), ZoneId::Headframe);
}

/// Every spot on every Zone's floors that a Zombie fits on has a path node near it that it can
/// walk straight to, and one it can be walked to from, so a Zombie there, or a Player chased
/// there, is never cut off from the graph by a Prop, a Ledge or a corner.
#[test]
fn every_open_floor_spot_walks_to_and_from_a_path_node() {
    for map in MapId::ALL {
        every_open_floor_spot_walks_to_and_from_a_path_node_on(map);
    }
}

fn every_open_floor_spot_walks_to_and_from_a_path_node_on(map_id: MapId) {
    let layout = map_id.layout();
    let mut sim = Sim::on(map_id, 60.0, |_| {});
    let (checked, cut_off): (usize, Vec<String>) = sim
        .world_mut()
        .run_system_once(move |spatial: SpatialQuery, graph: Res<NavGraph>| {
            let map = SpatialQueryFilter::from_mask(GameLayer::World);
            let half = body_half_height();
            let probe = Collider::sphere(ZOMBIE_RADIUS * 0.8);
            let body = Collider::capsule(ZOMBIE_RADIUS, ZOMBIE_CAPSULE_LENGTH);
            let mut cut_off = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for &zone in layout.zones {
                let (lo, hi) = zone.volume();
                // The Zone's own floor, and a storey up where it's tall enough to have a landing
                // or a deck there.
                let ground = zone.floor();
                let upper = (hi.y > ground + crate::maps::map::STOREY + 1.0).then_some(ground + crate::maps::map::STOREY);
                let floors: Vec<f32> = [ground]
                    .into_iter()
                    .chain(upper)
                    .chain(layout.raised_floors.iter().filter(|raised| raised.zone == zone).map(|raised| raised.height))
                    .chain(layout.sunken.iter().filter(|s| s.zone == zone).map(|s| -s.depth))
                    .collect();
                let mut x = lo.x + 0.75;
                while x < hi.x {
                    let mut z = lo.z + 0.75;
                    while z < hi.z {
                        let on_prop = layout.props.iter().any(|prop| {
                            (Vec2::new(x - prop.at.x, z - prop.at.z).abs() - Vec2::new(prop.size.x, prop.size.z) / 2.0).max_element() < ZOMBIE_RADIUS
                        });
                        for &floor in &floors {
                            let Some(hit) = spatial.cast_ray(Vec3::new(x, floor + 1.0, z), Dir3::NEG_Y, 1.5, true, &map) else {
                                continue;
                            };
                            let feet = Vec3::new(x, floor + 1.0 - hit.distance, z);
                            let centre = feet + Vec3::Y * half;
                            let fits = spatial.shape_intersections(&body, centre + Vec3::Y * 0.05, Quat::IDENTITY, &map).is_empty();
                            // Nobody stands in Deep Water, so nothing there needs a path node.
                            let in_deep = crate::maps::water::water_at(layout, crate::maps::map::TideLevel::Low, feet) == Some(crate::maps::map::Wading::Deep);
                            if on_prop || in_deep || (map_id == MapId::Polder && (feet.y - floor).abs() > 0.3) || feet.y < -3.0 || !fits || crate::enemies::nav::zone_at(map_id, feet + Vec3::Y * 0.1) != Some(zone) {
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
    assert!(checked > 500, "only {checked} spots checked on the {}", map_id.name());
    assert!(cut_off.is_empty(), "cut off from the path nodes on the {}: {cut_off:#?}", map_id.name());
}

/// Pack-a-Punch stands up on the Quarry's Headframe deck, at the end of the longest climb, behind
/// the Power Door at the foot of its stairs. With every Door open, a Zombie from any Zone on the
/// way there walks all the way up to a Player at the machine, never turning back and forth between
/// two path nodes until the stuck rule takes it.
#[test]
fn a_zombie_climbs_to_a_player_at_the_quarry_pack_a_punch() {
    use crate::maps::map::{MapId, STOREY};
    let q = 0.0;
    let deck = 3.0 * STOREY;
    let at_machine = Vec3::new(q + 2.0, deck, 36.2);
    let starts = [
        ("the Rail Yard", Vec3::new(q + 8.0, STOREY, 8.0)),
        ("the Crusher Shed", Vec3::new(q - 16.0, STOREY, 9.0)),
        ("the Winding House", Vec3::new(q + 15.0, 2.0 * STOREY, 25.5)),
        ("the Offices", Vec3::new(q - 6.0, 2.0 * STOREY, 25.5)),
        ("the Headframe's yard", Vec3::new(q + 3.0, 2.0 * STOREY, 29.3)),
    ];
    for (zone, start) in starts {
        let mut sim = Sim::on(MapId::Quarry, 60.0, |_| {});
        open_every_door(&mut sim);
        put_player(&mut sim, at_machine);
        let zombie = sim.spawn(start, None);
        let (arrived, last) = arrives(&mut sim, zombie, 60.0, |at| at.y > standing(deck) - 0.1 && at.z > 34.5);
        assert!(arrived, "from {zombie:?} in {zone}: last seen at {last}");
    }
}

/// The Village's ways round: along the High Street past the barricades, through its buildings
/// between the street and the lanes, round the Market's cross and the Mill's millstones, through
/// the Power's gates along the lanes, among the Orchard's trees and the Graveyard's graves, and
/// up the Church's chancel steps to Pack-a-Punch. A Zombie walks every one to the Player.
#[test]
fn a_zombie_follows_the_player_round_every_way_of_the_village() {
    use crate::maps::map::{MapId};
    let at = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    // Each start within the cleanup distance of the Player, as a spawn would be.
    let ways = [
        ("out of the Inn across the Market", at(0.0, 0.0, 14.0), at(10.0, 0.0, -18.0)),
        ("round the market cross", at(0.0, 0.0, -14.0), at(0.0, 0.0, -2.0)),
        ("along the High Street into the Mill", at(-36.0, 0.0, 0.0), at(-66.0, 0.0, 14.0)),
        ("along the High Street into the Church", at(36.0, 0.0, 0.0), at(56.0, 0.0, 14.0)),
        ("up the chancel to Pack-a-Punch", at(56.0, 0.0, 18.0), at(69.0, 0.45, 11.0)),
        ("through the Smithy to Mill Lane", at(-40.0, 0.0, 0.0), at(-40.0, 0.0, 26.0)),
        ("through the Bakery to the Back Lane", at(-14.0, 0.0, -6.0), at(-20.0, 0.0, 26.0)),
        ("through the Schoolhouse to Church Lane", at(40.0, 0.0, 0.0), at(40.0, 0.0, 26.0)),
        ("along the lanes through the west gate", at(-10.0, 0.0, 26.0), at(-40.0, 0.0, 26.0)),
        ("along the lanes through the east gate", at(10.0, 0.0, 26.0), at(40.0, 0.0, 26.0)),
        ("into the Orchard among the trees", at(-40.0, 0.0, 26.0), at(-66.0, 0.0, 48.0)),
        ("round the mausoleum", at(56.0, 0.0, 33.0), at(56.0, 0.0, 49.0)),
        ("among the graves", at(56.0, 0.0, 26.0), at(45.5, 0.0, 45.0)),
        ("out of the Church into the Graveyard", at(60.0, 0.0, 14.0), at(66.0, 0.0, 50.0)),
    ];
    for (way, start, player) in ways {
        let mut sim = Sim::on(MapId::Village, 60.0, |_| {});
        open_every_door(&mut sim);
        put_player(&mut sim, player);
        let zombie = sim.spawn(start, None);
        let (arrived, last) = arrives(&mut sim, zombie, 90.0, |feet| {
            (feet.y - standing(player.y)).abs() < 0.3 && Vec2::new(feet.x - player.x, feet.z - player.z).length() < 1.5
        });
        assert!(arrived, "{way}: last seen at {last}");
    }
}

/// With every Door on `map` open, a Zombie set down at `start` after a Player standing at
/// `player`: whether it reaches them within 60 s, and the last place it was seen.
pub(crate) fn follows(map: MapId, start: Vec3, player: Vec3) -> (bool, Vec3) {
    let mut sim = Sim::on(map, 60.0, |_| {});
    open_every_door(&mut sim);
    put_player(&mut sim, player);
    let zombie = sim.spawn(start, None);
    arrives(&mut sim, zombie, 60.0, |feet| {
        (feet.y - standing(player.y)).abs() < 0.3 && Vec2::new(feet.x - player.x, feet.z - player.z).length() < 1.5
    })
}

/// Every Door on the Map open, for following a Zombie from Zone to Zone.
fn open_every_door(sim: &mut Sim) {
    use crate::maps::door::{Door, open};
    sim.world_mut()
        .run_system_once(|mut commands: Commands, mut doors: Query<(Entity, &mut Door, &Transform)>| {
            for (entity, mut door, transform) in &mut doors {
                open(&mut commands, entity, &mut door, transform);
            }
        })
        .unwrap();
}

/// The Quarry's ways round besides the climb: up the Incline's ramp and steps and down off its
/// landings, out through it from the terraces, onto the Loading Dock's platform and off it, over
/// the spoil heap, and off the Rail Yard into the pit. A Zombie walks every one to the Player.
#[test]
fn a_zombie_follows_the_player_round_every_way_of_the_quarry() {
    use crate::maps::map::{MapId, STOREY};
    let (t1, t2) = (STOREY, 2.0 * STOREY);
    let at = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    let ways = [
        ("up the Incline to its top", at(0.0, 0.0, -15.0), at(-25.0, t2, 27.5)),
        ("up the Incline's ramp", at(0.0, 0.0, -15.0), at(-27.0, t1, 12.0)),
        ("down the Incline's drops", at(-25.0, t2, 27.5), at(-25.0, 0.0, -5.0)),
        ("from the Crusher Shed down the Incline", at(-10.0, t1, 14.0), at(-25.0, 0.0, -5.0)),
        ("from the Offices down the Incline", at(-10.0, t2, 25.5), at(-25.0, 0.0, -5.0)),
        ("up the Incline into the Crusher Shed", at(-25.0, 0.0, -5.0), at(-10.0, t1, 14.0)),
        ("onto the Loading Dock's platform", at(-5.0, 0.0, -5.0), at(31.5, 1.1, -9.0)),
        ("off the Loading Dock's platform", at(31.5, 1.1, -9.0), at(22.0, 0.0, -18.0)),
        ("from the Loading Dock to the Rail Yard", at(26.0, 0.0, -11.0), at(10.0, t1, 10.0)),
        ("up the spoil heap", at(-5.0, 0.0, -5.0), at(12.5, 1.2, -18.0)),
        ("down the spoil heap", at(12.5, 1.2, -18.0), at(-5.0, 0.0, -5.0)),
        ("off the Rail Yard into the pit", at(8.0, t1, 8.0), at(3.0, 0.0, -5.0)),
    ];
    for (way, start, player) in ways {
        let mut sim = Sim::on(MapId::Quarry, 60.0, |_| {});
        open_every_door(&mut sim);
        put_player(&mut sim, player);
        let zombie = sim.spawn(start, None);
        let (arrived, last) = arrives(&mut sim, zombie, 60.0, |feet| {
            (feet.y - standing(player.y)).abs() < 0.3 && Vec2::new(feet.x - player.x, feet.z - player.z).length() < 1.5
        });
        assert!(arrived, "{way}: last seen at {last}");
    }
}

/// A Zombie Out of Bounds is quietly removed at once, wherever it is, and goes back into the
/// Horde: past the edge of the plan, or fallen well below the lowest floor.
#[test]
fn a_zombie_out_of_bounds_goes_back_into_the_horde_at_once() {
    use crate::run::round::{Round, requeue_cleaned_up};
    let (min, _) = MapId::Greybox.layout().plan;
    let under = MapId::Greybox.layout().lowest_floor() - OUT_OF_BOUNDS_DROP - 1.0;
    for out in [Vec3::new(min.x - 2.0, 0.0, 0.0), Vec3::new(0.0, under, 0.0)] {
        let mut sim = Sim::new(60.0, |app| {
            app.insert_resource(Round::in_progress(3)).add_systems(Update, requeue_cleaned_up.after(clean_up_stuck));
        });
        assert!(sim.world_mut().resource_mut::<Round>().take_from_horde());
        let spawned = sim.world_mut().resource::<Round>().horde_spawned();
        let zombie = sim.spawn(out, None);
        sim.run(2);
        assert!(sim.world_mut().get_entity(zombie).is_err(), "a Zombie at {out} is still there");
        assert_eq!(sim.world_mut().resource::<Round>().horde_spawned(), spawned - 1, "it isn't back in the Horde");
        assert!(sim.world_mut().resource::<Messages<ZombieKilled>>().is_empty(), "it counted as a kill");
    }
}

/// Follows a Walker from `start` after a Player standing at `player` below a drop, for up to
/// `secs`: whether it got down below `under`, and where it was last seen. A drop gated on a Door
/// is followed with every Door open, as the drop only opens with the Door beside it.
fn drops_after(map: MapId, start: Vec3, player: Vec3, under: f32, secs: f32) -> (bool, Vec3) {
    drops_after_with(map, start, player, under, secs, false)
}

fn drops_after_with(map: MapId, start: Vec3, player: Vec3, under: f32, secs: f32, doors_open: bool) -> (bool, Vec3) {
    let mut sim = Sim::on(map, 60.0, |_| {});
    if doors_open {
        open_every_door(&mut sim);
    }
    put_player(&mut sim, player);
    let zombie = sim.spawn(start, None);
    arrives(&mut sim, zombie, secs, |at| at.y < under)
}

/// The Rail Yard is Active as soon as the pit is, through its drops: a Zombie rising there, or
/// climbing in at its window, after a Player down in the pit drops into the pit to get them,
/// before the stuck rule gives up on it.
#[test]
fn a_zombie_in_the_rail_yard_drops_into_the_pit_after_a_player_there() {
    use crate::maps::map::STOREY;
    const Q: f32 = 0.0;
    let player = Vec3::new(Q + 6.0, 0.0, -6.0);
    // Each Riser, the window, and the middle of the yard.
    for start in [
        Vec3::new(Q + 4.0, STOREY, 12.0),
        Vec3::new(Q + 12.0, STOREY, 10.0),
        Vec3::new(Q + 18.5, STOREY, 4.5),
        Vec3::new(Q + 8.0, STOREY, 8.0),
    ] {
        let (down, last) = drops_after(MapId::Quarry, start, player, standing(0.0) + 0.5, 9.0);
        assert!(down, "from {start}: never dropped into the pit; last seen at {last}");
    }
}

/// The same on the Greybox: a Zombie on the Balcony after a Player in the Yard below takes the
/// drop.
#[test]
fn a_zombie_on_the_balcony_takes_the_drop_to_a_player_below() {
    let (down, last) = drops_after(MapId::Greybox, Vec3::new(-12.5, 4.0, -2.0), Vec3::new(-5.5, 0.0, 4.0), 2.0, 9.0);
    assert!(down, "never took the drop; last seen at {last}");
}

/// Every drop on every Map: the one-way edges that aren't a Vent's. A Walker at the top node of
/// one, after a Player standing at its bottom node, takes it and reaches them within a few
/// seconds, well before the stuck rule gives up on it.
#[test]
fn a_zombie_takes_every_drop_to_a_player_below() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for map in MapId::ALL {
        let layout = map.layout();
        let node = |name: &str| {
            let &(_, x, y, z) = layout.nav_nodes.iter().find(|(n, ..)| *n == name).unwrap();
            Vec3::new(x, y, z)
        };
        for &(top, bottom, gate) in layout.nav_edges {
            let door_drop = matches!(gate, crate::enemies::nav::Gate::DoorDrop(..));
            if !(gate == crate::enemies::nav::Gate::OneWay || door_drop) || top.starts_with("vent_") {
                continue;
            }
            let (from, to) = (node(top), node(bottom));
            if from.y - to.y < 1.0 {
                continue;
            }
            checked += 1;
            let (arrived, last) = drops_after_with(map, from, to, to.y + 1.0, ZOMBIE_CLEANUP_SECS - 3.0, door_drop);
            if !arrived {
                failures.push(format!("{} {top} -> {bottom}: last seen at {last}", map.name()));
            }
        }
    }
    assert!(checked >= 8, "only {checked} drops checked");
    assert!(failures.is_empty(), "stalled at a drop: {failures:#?}");
}

/// Three Zombies crossing the Greybox to a Player in the Start Room, every Door open: their
/// feet every frame until each is within a body or two of the Player.
fn greybox_crossing() -> Vec<(&'static str, Vec<Vec3>)> {
    let player = Vec3::new(0.0, 0.0, 20.0);
    let starts = [
        ("a: Dining Room west to Start Room", Vec3::new(-22.0, 0.0, -20.0)),
        ("b: Power Room north to Start Room", Vec3::new(20.0, 0.0, -15.5)),
        ("c: far south wing to Start Room", Vec3::new(-20.0, 0.0, -6.0)),
    ];
    let mut sim = Sim::on(MapId::Greybox, 60.0, |_| {});
    open_every_door(&mut sim);
    put_player(&mut sim, player);
    let zombies: Vec<_> = starts.iter().map(|&(name, at)| (name, sim.spawn(at, None))).collect();
    let mut traces: Vec<Vec<Vec3>> = vec![Vec::new(); zombies.len()];
    for _ in 0..sim.frames(60.0) {
        sim.run(1);
        let mut left = false;
        for (trace, &(_, zombie)) in traces.iter_mut().zip(&zombies) {
            let Some(at) = sim.world_mut().get::<Transform>(zombie).map(|t| t.translation) else { continue };
            if Vec2::new(at.x - player.x, at.z - player.z).length() > 2.0 {
                trace.push(at);
                left = true;
            }
        }
        if !left {
            break;
        }
    }
    zombies.iter().map(|&(name, _)| name).zip(traces).collect()
}

/// Metres walked, and total turning (radians, over half-second strides) along a trace.
fn trace_stats(trace: &[Vec3]) -> (f32, f32) {
    let length = trace.windows(2).map(|w| horizontal(w[1] - w[0]).length()).sum();
    let strides: Vec<Vec3> = trace.iter().step_by(30).map(|p| horizontal(*p)).collect();
    let turning = strides
        .windows(3)
        .map(|w| {
            let (a, b) = ((w[1] - w[0]).normalize_or_zero(), (w[2] - w[1]).normalize_or_zero());
            a.dot(b).clamp(-1.0, 1.0).acos()
        })
        .sum();
    (length, turning)
}

#[test]
#[ignore = "prints a trace of Zombies crossing the Greybox; asserts nothing"]
fn trace_greybox_crossing() {
    let crossing = greybox_crossing();
    let graph = NavGraph::new(MapId::Greybox);
    let (w, h) = (61usize, 61usize);
    let mut grid = vec![vec![' '; w]; h];
    let cell = |p: Vec3| ((p.x + 30.5).floor() as isize, (p.z + 30.5).floor() as isize);
    let mut put = |p: Vec3, c: char| {
        let (x, z) = cell(p);
        if (0..w as isize).contains(&x) && (0..h as isize).contains(&z) {
            grid[z as usize][x as usize] = c;
        }
    };
    for node in graph.nodes.iter().filter(|n| n.at.y.abs() < 0.8) {
        put(node.at, '+');
    }
    for (i, (name, trace)) in crossing.iter().enumerate() {
        let (length, turning) = trace_stats(trace);
        println!("{name}: {:.1} s, {length:.1} m walked, {turning:.2} rad turned", trace.len() as f32 / 60.0);
        let at: Vec<String> = trace.iter().step_by(60).map(|p| format!("({:.1},{:.1})", p.x, p.z)).collect();
        println!("  every second: {}", at.join(" "));
        for p in trace.iter().step_by(6) {
            put(*p, (b'a' + i as u8) as char);
        }
    }
    println!("top-down, x right, z down, 1 m cells, + path node:");
    for row in grid.iter().skip(5).take(51) {
        println!("{}", row.iter().collect::<String>().trim_end());
    }
}

/// A Zombie standing at `feet` on the Greybox, given `waypoints` to follow with its re-plan held
/// off, after one look-ahead: the waypoints it has left.
fn waypoints_after_look(feet: Vec3, waypoints: Vec<Vec3>) -> Vec<Vec3> {
    let mut sim = Sim::on(MapId::Greybox, 60.0, |_| {});
    put_player(&mut sim, Vec3::new(0.0, 0.0, 20.0));
    let zombie = sim.spawn(feet, None);
    // One frame to pick its Player, then its own route in place of the plan.
    sim.run(1);
    let mut route = sim.world_mut().get_mut::<Route>(zombie).unwrap();
    route.waypoints = waypoints;
    route.repath_in = 100.0;
    route.look_in = 0.0;
    route.side_for = None;
    sim.run(1);
    sim.world_mut().get::<Route>(zombie).unwrap().waypoints.clone()
}

#[test]
fn a_zombie_with_a_clear_walk_to_a_later_waypoint_skips_the_ones_before_it() {
    // In the Start Room: the middle, then the east side, then the east window's inside.
    let (start, east, window) = (Vec3::new(0.0, 0.0, 20.0), Vec3::new(12.5, 0.0, 20.0), Vec3::new(7.0, 0.0, 24.5));
    let left = waypoints_after_look(Vec3::new(-3.0, 0.0, 17.0), vec![start, east, window]);
    assert_eq!(left, vec![window], "walks straight to the furthest it can see, never past the last");
}

#[test]
fn a_zombie_never_skips_a_waypoint_through_a_wall_or_onto_another_floor() {
    let (start, dining) = (Vec3::new(0.0, 0.0, 20.0), Vec3::new(-13.0, 0.0, -20.0));
    let left = waypoints_after_look(Vec3::new(-3.0, 0.0, 17.0), vec![start, dining]);
    assert_eq!(left.len(), 2, "a wall is between it and the Dining Room: {left:?}");
    let (bottom, mid) = (Vec3::new(-15.7, 0.0, 10.0), Vec3::new(-15.7, 2.0, 4.0));
    let left = waypoints_after_look(Vec3::new(-15.7, 0.0, 12.0), vec![bottom, mid]);
    assert_eq!(left.len(), 2, "the ramp is climbed node by node: {left:?}");
}

#[test]
fn a_zombie_rounds_the_corners_of_the_greybox_without_more_turning_than_before() {
    // Total turning over half-second strides before this change.
    let before = [3.21, 3.73, 1.40];
    // A Zombie's sideways offset is seeded from its entity, so anything that spawns more of the
    // Map before it (a textured stair tread, a decal) shifts the path a little. Allow for that,
    // while still catching a Zombie that zig-zags more than it used to.
    const NOISE: f32 = 1.1;
    for ((name, trace), before) in greybox_crossing().iter().zip(before) {
        assert!(trace.len() < 60 * 45, "{name}: gave up before reaching the Player");
        let (_, turning) = trace_stats(trace);
        assert!(turning <= before * NOISE, "{name}: turned {turning:.2} rad, was {before}");
    }
}

#[test]
fn zombies_have_their_own_steady_sideways_offsets() {
    let ids: Vec<Entity> = (1..40).map(|i| Entity::from_raw_u32(i).unwrap()).collect();
    assert!(ids.iter().all(|&e| (-1.0..=1.0).contains(&personal(e, 2)) && personal(e, 2) == personal(e, 2)));
    let spread = ids.iter().map(|&e| personal(e, 2)).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    assert!(spread.0 < -0.5 && spread.1 > 0.5, "spread {spread:?}");
}

/// A Crawler, from the Walker standing at `feet`: its legs blown off.
fn spawn_crawler(sim: &mut Sim, feet: Vec3) -> Entity {
    let zombie = sim.spawn(feet, None);
    sim.run(sim.frames(0.5));
    sim.send(Gibbed { zombie });
    sim.run(2);
    assert!(sim.world_mut().get::<Zombie>(zombie).unwrap().crawler);
    zombie
}

/// The Player, who runs the Greybox's loop at a sprint (9 m/s) and stops at the end, is followed by
/// a Crawler (1 m/s) that is left far behind: it isn't cleaned up on the way, and it reaches them.
#[test]
fn a_crawler_follows_a_player_who_ran_round_the_loop_and_reaches_them() {
    let (start, bend, end) = (Vec3::new(20.0, 0.0, -15.5), Vec3::new(0.0, 0.0, 20.0), Vec3::new(-22.0, 0.0, -20.0));
    let leg = start.distance(bend);
    let run = leg + bend.distance(end);
    assert!(run > 60.0);
    let mut sim = Sim::new(60.0, |_| {});
    open_every_door(&mut sim);
    put_player(&mut sim, start);
    let crawler = spawn_crawler(&mut sim, start);
    let mut ran = 0.0;
    let mut reached = None;
    for frame in 0..sim.frames(240.0) {
        ran = (ran + 9.0 * sim.frame_secs).min(run);
        let player = if ran <= leg {
            start.lerp(bend, ran / leg)
        } else {
            bend.lerp(end, (ran - leg) / bend.distance(end))
        };
        put_player(&mut sim, player);
        sim.run(1);
        let Some(at) = sim.world_mut().get::<Transform>(crawler).map(|t| t.translation) else {
            panic!("the Crawler was cleaned up after {:.0} s, with the Player at {player}", frame as f32 * sim.frame_secs)
        };
        if ran >= run && Vec2::new(at.x - end.x, at.z - end.z).length() < 2.0 {
            reached = Some(frame as f32 * sim.frame_secs);
            break;
        }
    }
    assert!(reached.is_some(), "the Crawler never reached the Player");
}

/// Holds `zombie` still (stunned) for `secs`, as a Zombie wedged on geometry: when the stuck rule
/// removed it, if it did.
fn wedged_for(sim: &mut Sim, zombie: Entity, secs: f32) -> Option<f32> {
    sim.world_mut().entity_mut(zombie).insert(Stunned { left: 1000.0 });
    for frame in 0..sim.frames(secs) {
        sim.run(1);
        if sim.world_mut().get_entity(zombie).is_err() {
            return Some(frame as f32 * sim.frame_secs);
        }
    }
    None
}

/// A Zombie that makes no progress along its route, far from the Player or near, is still cleaned
/// up after the clock.
#[test]
fn a_wedged_zombie_is_cleaned_up_after_the_clock() {
    for player in [Vec3::new(10.0, 0.0, 20.0), Vec3::new(-20.0, 0.0, -20.0)] {
        let mut sim = Sim::new(60.0, |_| {});
        open_every_door(&mut sim);
        put_player(&mut sim, player);
        let zombie = sim.spawn(Vec3::new(20.0, 0.0, -15.5), None);
        let gone = wedged_for(&mut sim, zombie, ZOMBIE_CLEANUP_SECS + 3.0);
        let gone = gone.unwrap_or_else(|| panic!("a wedged Zombie survived, Player at {player}"));
        assert!((gone - ZOMBIE_CLEANUP_SECS).abs() < 1.5, "went after {gone} s");
    }
}

/// The last Zombie of a Round, if it's truly lost, still goes back into the Horde, so a Round
/// can't stall waiting on it.
#[test]
fn the_last_zombie_of_a_round_that_is_truly_lost_still_goes_back_into_the_horde() {
    use crate::run::round::{Round, requeue_cleaned_up};
    let mut sim = Sim::new(60.0, |app| {
        app.insert_resource(Round::in_progress(1)).add_systems(Update, requeue_cleaned_up.after(clean_up_stuck));
    });
    open_every_door(&mut sim);
    put_player(&mut sim, Vec3::new(-20.0, 0.0, -20.0));
    assert!(sim.world_mut().resource_mut::<Round>().take_from_horde());
    let spawned = sim.world_mut().resource::<Round>().horde_spawned();
    let zombie = sim.spawn(Vec3::new(20.0, 0.0, -15.5), None);
    assert!(wedged_for(&mut sim, zombie, ZOMBIE_CLEANUP_SECS + 3.0).is_some(), "it was never cleaned up");
    assert_eq!(sim.world_mut().resource::<Round>().horde_spawned(), spawned - 1, "it isn't back in the Horde");
}
