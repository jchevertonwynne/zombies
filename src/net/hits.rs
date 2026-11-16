//! Hits and launches over the network, on the reliable stream.
//!
//! A client's Weapons judge their own hits. Each `HitReport` of the `LocalPlayer`'s is sent to the
//! Host, tagged with the tick, naming the Zombie by its id; the Host trusts it and drops only a hit
//! on a Zombie it has already killed or that is gone. The shooter has the blood and the hit sound
//! at once, but the kill and its Points wait for the Host.
//!
//! What flies or waits on its own (a launcher round, a Wonder Weapon's shot, a Lethal, a Claymore,
//! a Monkey Bomb, and a hitscan blast like the Ray Gun's) goes to the Host as a `Throw`. The Host
//! simulates it and every random roll stays there. It tells every client when something goes up
//! (`Launched`: kind, start, velocity) and when something goes off (`Burst`). In between, a client
//! flies the same arc for show: no damage, no physics.

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::play::{Clock, Prediction, Roster, Wire};
use super::protocol::{Peer, ToHost, ToPlayer, encode};
use super::remote::Remote;
use super::state::Tick;
use super::zombies::ZombieIds;
use crate::weapons::ammo_mod::{AmmoModHit, AmmoMods};
use crate::weapons::arsenal::{Burn, Splash, WeaponId};
use crate::weapons::burn::Ignite;
use crate::authority::HostSimulation;
use crate::weapons::combat::HitReport;
use crate::weapons::explosive::{
    Explosion, ExplosiveAssets, Flight, Launch, LauncherRound, Thrown, fireball, world_filter,
};
use crate::players::player::{LocalPlayer, Player};
use crate::state::{InRun, world_runs};
use crate::tuning::*;
use crate::weapons::weapon::apply_hit_reports;
use crate::weapons::wonder::{WonderFired, is_wonder};
use crate::enemies::zombie::{Attack, HitZone, Zombie, ZombieHit, Dying, is_headshot};

/// A bullet or Knife landed on a Zombie, as a client tells the Host.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// The client's tick when it landed.
    pub tick: Tick,
    pub zombie: u16,
    pub zone: HitZone,
    pub damage: f32,
    /// The Knife, or else a bullet.
    pub knife: bool,
    /// The Ammo Mods of the Weapon that landed it, one bit each (`AmmoMods::bits`).
    pub ammo_mods: u8,
    /// How the Weapon sets what it hits alight: damage, seconds between ticks, seconds in all.
    pub burn: Option<[f32; 3]>,
}

/// Something a client's Player threw, planted or fired that the Host is to simulate. Positions and
/// directions are `[x, y, z]`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Throw {
    Launcher {
        origin: [f32; 3],
        direction: [f32; 3],
        speed: f32,
        gravity: f32,
        impact: f32,
        impact_headshot: f32,
        headshots: bool,
        splash: [f32; 3],
        ammo_mods: u8,
        crawlers: bool,
    },
    Wonder {
        origin: [f32; 3],
        direction: [f32; 3],
        weapon: WeaponId,
        upgraded: bool,
        damage: f32,
        splash: Option<[f32; 3]>,
    },
    Frag { from: [f32; 3], aim: [f32; 3], fuse: f32 },
    MonkeyBomb { from: [f32; 3], aim: [f32; 3] },
    Claymore { at: [f32; 3], facing: [f32; 3] },
    /// A blast that goes off where a hitscan line landed (the Ray Gun's).
    Blast { at: [f32; 3], splash: [f32; 3], ammo_mods: u8, crawlers: bool, hurts_player: bool },
}

fn splash_out(splash: Splash) -> [f32; 3] {
    [splash.max, splash.min, splash.radius]
}

fn splash_in([max, min, radius]: [f32; 3]) -> Splash {
    Splash { max, min, radius }
}

fn burn_out(burn: Burn) -> [f32; 3] {
    [burn.damage, burn.tick_secs, burn.secs]
}

/// The `Burn` a client says, if it is one that can tick.
fn burn_in([damage, tick_secs, secs]: [f32; 3]) -> Option<Burn> {
    (damage.is_finite() && secs.is_finite() && secs >= 0.0 && tick_secs.is_finite() && tick_secs > 0.0)
        .then_some(Burn { damage, tick_secs, secs })
}

impl Throw {
    /// What a client says of `thrown`.
    pub fn of(thrown: &Thrown) -> Throw {
        match *thrown {
            Thrown::LauncherRound(round) => Throw::Launcher {
                origin: round.origin.to_array(),
                direction: round.direction.to_array(),
                speed: round.flight.speed,
                gravity: round.flight.gravity,
                impact: round.impact,
                impact_headshot: round.impact_headshot,
                headshots: round.headshots,
                splash: splash_out(round.splash),
                ammo_mods: round.ammo_mods.bits(),
                crawlers: round.crawlers,
            },
            Thrown::Wonder(shot) => Throw::Wonder {
                origin: shot.origin.to_array(),
                direction: shot.direction.to_array(),
                weapon: shot.stats.id,
                upgraded: shot.upgraded,
                damage: shot.damage,
                splash: shot.splash.map(splash_out),
            },
            Thrown::Frag { from, aim, fuse } => Throw::Frag { from: from.to_array(), aim: aim.to_array(), fuse },
            Thrown::MonkeyBomb { from, aim } => Throw::MonkeyBomb { from: from.to_array(), aim: aim.to_array() },
            Thrown::Claymore { at, facing } => Throw::Claymore { at: at.to_array(), facing: facing.to_array() },
        }
    }

    /// The `Launch` this is, or none if it is not one the Host can make sense of (a number that is
    /// not a number, a direction with no length, a Weapon that is not a Wonder Weapon). A blast is
    /// not a `Launch`.
    pub fn launch(self) -> Option<Thrown> {
        let finite = |numbers: &[f32]| numbers.iter().all(|number| number.is_finite());
        match self {
            Throw::Launcher {
                origin, direction, speed, gravity, impact, impact_headshot, headshots, splash, ammo_mods, crawlers,
            } => {
                if !finite(&[speed, gravity, impact, impact_headshot]) || !finite(&splash) || !finite(&origin) {
                    return None;
                }
                Some(Thrown::LauncherRound(LauncherRound {
                    origin: Vec3::from(origin),
                    direction: Dir3::new(Vec3::from(direction)).ok()?,
                    flight: Flight { speed, gravity },
                    impact,
                    impact_headshot,
                    headshots,
                    splash: splash_in(splash),
                    ammo_mods: AmmoMods::from_bits(ammo_mods),
                    crawlers,
                }))
            }
            Throw::Wonder { origin, direction, weapon, upgraded, damage, splash } => {
                let stats = if upgraded { weapon.upgraded() } else { weapon.stats() };
                if !is_wonder(stats.class) || !finite(&origin) || !damage.is_finite() {
                    return None;
                }
                if splash.is_some_and(|splash| !finite(&splash)) {
                    return None;
                }
                Some(Thrown::Wonder(WonderFired {
                    origin: Vec3::from(origin),
                    direction: Dir3::new(Vec3::from(direction)).ok()?,
                    stats,
                    upgraded,
                    damage,
                    splash: splash.map(splash_in),
                }))
            }
            Throw::Frag { from, aim, fuse } => {
                (finite(&from) && finite(&aim) && fuse.is_finite()).then(|| Thrown::Frag {
                    from: Vec3::from(from),
                    aim: Vec3::from(aim),
                    fuse,
                })
            }
            Throw::MonkeyBomb { from, aim } => (finite(&from) && finite(&aim))
                .then(|| Thrown::MonkeyBomb { from: Vec3::from(from), aim: Vec3::from(aim) }),
            Throw::Claymore { at, facing } => (finite(&at) && finite(&facing))
                .then(|| Thrown::Claymore { at: Vec3::from(at), facing: Vec3::from(facing) }),
            Throw::Blast { .. } => None,
        }
    }

    /// The `Explosion` this is, if it is a blast.
    pub fn blast(self, thrower: Entity) -> Option<Explosion> {
        let Throw::Blast { at, splash, ammo_mods, crawlers, hurts_player } = self else {
            return None;
        };
        (at.iter().chain(&splash).all(|number| number.is_finite())).then(|| Explosion {
            at: Vec3::from(at),
            thrower: Some(thrower),
            splash: splash_in(splash),
            hurts_player,
            hurts_zombies: true,
            ammo_mods: AmmoMods::from_bits(ammo_mods),
            crawlers,
        })
    }
}

/// What went up.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A rocket, flying straight.
    Rocket,
    /// A launcher's grenade, dropping under gravity.
    Grenade,
    /// A Wonder Weapon's shot.
    Wonder,
    Frag,
    MonkeyBomb,
    /// A Claymore going down where it is.
    Claymore,
}

impl Kind {
    /// How fast it falls while it flies.
    fn gravity(self) -> f32 {
        match self {
            Kind::Rocket | Kind::Wonder | Kind::Claymore => 0.0,
            Kind::Grenade | Kind::Frag | Kind::MonkeyBomb => GRAVITY,
        }
    }
}

/// The Host tells every client something has gone up.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Launched {
    pub kind: Kind,
    pub start: [f32; 3],
    pub velocity: [f32; 3],
}

impl Launched {
    /// How `thrown` goes up.
    pub fn of(thrown: &Thrown) -> Launched {
        let (kind, start, velocity) = match *thrown {
            Thrown::LauncherRound(round) => (
                if round.flight.gravity > 0.0 { Kind::Grenade } else { Kind::Rocket },
                round.origin,
                *round.direction * round.flight.speed,
            ),
            Thrown::Wonder(shot) => (Kind::Wonder, shot.origin, *shot.direction * COSMETIC_WONDER_SPEED),
            Thrown::Frag { from, aim, .. } => {
                (Kind::Frag, from, aim * EQUIPMENT_THROW_SPEED + Vec3::Y * EQUIPMENT_THROW_LIFT)
            }
            Thrown::MonkeyBomb { from, aim } => {
                (Kind::MonkeyBomb, from, aim * EQUIPMENT_THROW_SPEED + Vec3::Y * EQUIPMENT_THROW_LIFT)
            }
            Thrown::Claymore { at, .. } => (Kind::Claymore, at, Vec3::ZERO),
        };
        Launched { kind, start: start.to_array(), velocity: velocity.to_array() }
    }
}

/// The Host tells every client something has gone off.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Burst {
    pub at: [f32; 3],
    pub radius: f32,
}

/// What has come in over the reliable stream about hits and launches, not yet read.
#[derive(Resource, Default)]
pub struct Hits {
    hits: Vec<(Peer, Hit)>,
    throws: Vec<(Peer, Throw)>,
    /// Where this machine's own Player's blasts went off, which it has already heard, and seconds
    /// left to wait for the Host's `Burst` of each.
    heard: Vec<(Vec3, f32)>,
    launched: Vec<Launched>,
    bursts: Vec<Burst>,
}

impl Hits {
    /// Takes in what a Player said to the Host, if it is about hits or launches.
    pub fn hear_player(&mut self, peer: Peer, message: &ToHost) {
        match *message {
            ToHost::Hit(hit) => self.hits.push((peer, hit)),
            ToHost::Launch(throw) => self.throws.push((peer, throw)),
            _ => {}
        }
    }

    /// Takes in what the Host said, if it is about launches.
    pub fn hear_host(&mut self, message: &ToPlayer) {
        match *message {
            ToPlayer::Launched(launched) => self.launched.push(launched),
            ToPlayer::Burst(burst) => self.bursts.push(burst),
            _ => {}
        }
    }
}

/// Puts the systems in the app. Each end's run only while that end's resources are there.
pub fn add(app: &mut App) {
    app.add_message::<HitReport>()
        .add_message::<Launch>()
        .add_message::<Explosion>()
        .add_message::<ZombieHit>()
        .add_message::<AmmoModHit>()
        .add_message::<Ignite>()
        .add_systems(
            Update,
            (receive.before(apply_hit_reports), announce)
                .chain()
                .in_set(HostSimulation)
                .run_if(world_runs)
                .run_if(resource_exists::<Roster>),
        )
        .add_systems(
            Update,
            (report, hear, fly_cosmetics)
                .chain()
                .run_if(world_runs)
                .run_if(resource_exists::<Prediction>),
        );
}

// The Host

/// Turns what the Players reported into what the Host simulates: their hits on Zombies still
/// alive into `HitReport`s, and their throws into `Launch`es and blasts.
fn receive(
    mut inbox: ResMut<Hits>,
    ids: Res<ZombieIds>,
    players: Query<(Entity, &Remote), (With<Player>, Without<crate::players::respawn::Disconnected>)>,
    zombies: Query<&Zombie, Without<Dying>>,
    mut reports: MessageWriter<HitReport>,
    mut launches: MessageWriter<Launch>,
    mut explosions: MessageWriter<Explosion>,
    mut ammo_mod_hits: MessageWriter<AmmoModHit>,
    mut ignite: MessageWriter<Ignite>,
) {
    let who = |peer: Peer| players.iter().find(|(_, remote)| remote.peer == peer).map(|(player, _)| player);
    for (peer, hit) in std::mem::take(&mut inbox.hits) {
        let Some(player) = who(peer) else { continue };
        // A Zombie already killed, or gone, takes no more.
        let Some(zombie) = ids.entity(hit.zombie).filter(|&zombie| zombies.get(zombie).is_ok_and(|body| body.health() > 0.0))
        else {
            continue;
        };
        if hit.damage.is_nan() {
            continue;
        }
        reports.write(HitReport {
            player,
            zombie,
            zone: hit.zone,
            damage: hit.damage,
            attack: if hit.knife { Attack::Knife } else { Attack::Bullet },
            ammo_mods: AmmoMods::from_bits(hit.ammo_mods),
            burn: hit.burn.and_then(burn_in),
        });
        // The same rolls the Host's own hits make, for this Player's Cooldowns and Modifiers.
        let ammo_mods = AmmoMods::from_bits(hit.ammo_mods);
        if !ammo_mods.is_empty() {
            ammo_mod_hits.write(AmmoModHit { player, zombie, damage: hit.damage, ammo_mods });
        }
        if let Some(burn) = hit.burn.and_then(burn_in) {
            ignite.write(Ignite { player, zombie, burn });
        }
    }
    for (peer, throw) in std::mem::take(&mut inbox.throws) {
        let Some(player) = who(peer) else { continue };
        if let Some(thrown) = throw.launch() {
            launches.write(Launch { player, thrown });
        } else if let Some(explosion) = throw.blast(player) {
            explosions.write(explosion);
        }
    }
}

/// Tells every Player what went up and what went off on the Host, whoever's it was.
fn announce(
    mut launches: MessageReader<Launch>,
    mut explosions: MessageReader<Explosion>,
    roster: Res<Roster>,
    mut wire: ResMut<Wire>,
) {
    for launch in launches.read() {
        let bytes = encode(&ToPlayer::Launched(Launched::of(&launch.thrown)));
        for &(peer, _) in &roster.0 {
            wire.0.send(peer, bytes.clone());
        }
    }
    for explosion in explosions.read() {
        let bytes = encode(&ToPlayer::Burst(Burst { at: explosion.at.to_array(), radius: explosion.splash.radius }));
        for &(peer, _) in &roster.0 {
            wire.0.send(peer, bytes.clone());
        }
    }
}

// A client

/// Tells the Host of the `LocalPlayer`'s hits and launches, and gives the shooter their hit sound
/// at once. Nothing is paid for it here.
fn report(
    clock: Res<Clock>,
    ids: Res<ZombieIds>,
    mut inbox: ResMut<Hits>,
    mut wire: ResMut<Wire>,
    local: Query<Entity, With<LocalPlayer>>,
    mut reports: MessageReader<HitReport>,
    mut launches: MessageReader<Launch>,
    mut explosions: MessageReader<Explosion>,
    mut landed: MessageWriter<ZombieHit>,
) {
    let Ok(local) = local.single() else { return };
    for report in reports.read().filter(|report| report.player == local) {
        let Some(zombie) = ids.id(report.zombie) else { continue };
        let hit = Hit {
            tick: clock.tick,
            zombie,
            zone: report.zone,
            damage: report.damage,
            knife: report.attack == Attack::Knife,
            ammo_mods: report.ammo_mods.bits(),
            burn: report.burn.map(burn_out),
        };
        wire.0.send(super::protocol::HOST, encode(&ToHost::Hit(hit)));
        landed.write(ZombieHit {
            player: Some(local),
            pays: false,
            headshot: is_headshot(report.zone, report.attack),
        });
    }
    for launch in launches.read().filter(|launch| launch.player == local) {
        wire.0.send(super::protocol::HOST, encode(&ToHost::Launch(Throw::of(&launch.thrown))));
    }
    // Blasts the Host sent are no one's; only the `LocalPlayer`'s own go back.
    for explosion in explosions.read().filter(|explosion| explosion.thrower == Some(local)) {
        let blast = Throw::Blast {
            at: explosion.at.to_array(),
            splash: splash_out(explosion.splash),
            ammo_mods: explosion.ammo_mods.bits(),
            crawlers: explosion.crawlers,
            hurts_player: explosion.hurts_player,
        };
        wire.0.send(super::protocol::HOST, encode(&ToHost::Launch(blast)));
        // Its sound is playing already; the Host's `Burst` only adds the fireball.
        inbox.heard.push((explosion.at, COSMETIC_FLIGHT_SECS));
    }
}

/// Something in the air, or sitting, that the Host said is there. It does nothing but show.
#[derive(Component)]
pub struct Cosmetic {
    velocity: Vec3,
    gravity: f32,
    /// Seconds left before it is let go of if its burst never comes.
    life: f32,
    landed: bool,
}

/// Puts up what the Host said went up, and takes down and shows what it said went off.
fn hear(
    mut commands: Commands,
    mut inbox: ResMut<Hits>,
    assets: Option<Res<ExplosiveAssets>>,
    cosmetics: Query<(Entity, &Transform), With<Cosmetic>>,
    mut explosions: MessageWriter<Explosion>,
    time: Res<Time>,
) {
    inbox.heard.retain_mut(|(_, left)| {
        *left -= time.delta_secs();
        *left > 0.0
    });
    for launched in std::mem::take(&mut inbox.launched) {
        let start = Vec3::from(launched.start);
        let mut thing = commands.spawn((
            InRun,
            Cosmetic {
                velocity: Vec3::from(launched.velocity),
                gravity: launched.kind.gravity(),
                life: COSMETIC_FLIGHT_SECS,
                landed: launched.kind == Kind::Claymore,
            },
            Transform::from_translation(start),
            Visibility::default(),
        ));
        if let Some(assets) = &assets {
            match launched.kind {
                Kind::Rocket | Kind::Grenade | Kind::Wonder => {
                    thing.insert((Mesh3d(assets.projectile.clone()), MeshMaterial3d(assets.casing.clone())));
                    if let Ok(direction) = Dir3::new(Vec3::from(launched.velocity)) {
                        thing.insert(Transform::from_translation(start).with_rotation(Quat::from_rotation_arc(Vec3::Y, *direction)));
                    }
                }
                Kind::Frag | Kind::MonkeyBomb => {
                    thing.insert((
                        Mesh3d(assets.ball.clone()),
                        MeshMaterial3d(assets.casing.clone()),
                        Transform::from_translation(start).with_scale(Vec3::splat(THROWN_RADIUS)),
                    ));
                }
                Kind::Claymore => {
                    thing.insert((
                        Mesh3d(assets.ball.clone()),
                        MeshMaterial3d(assets.casing.clone()),
                        Transform::from_translation(start).with_scale(Vec3::new(0.15, 0.06, 0.04)),
                    ));
                }
            }
        }
    }
    let mut taken: Vec<Entity> = Vec::new();
    for burst in std::mem::take(&mut inbox.bursts) {
        let at = Vec3::from(burst.at);
        // The thing that went off is the nearest still up.
        let nearest = cosmetics
            .iter()
            .filter(|(entity, _)| !taken.contains(entity))
            .map(|(entity, transform)| (entity, transform.translation.distance(at)))
            .filter(|&(_, distance)| distance <= COSMETIC_BURST_MATCH_METRES)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((entity, _)) = nearest {
            taken.push(entity);
            commands.entity(entity).despawn();
        }
        if let Some(assets) = &assets {
            commands.spawn(fireball(assets, at, burst.radius));
        }
        // For the sound, unless the shooter has heard it; no one's blast, so it is not sent back.
        if let Some(heard) = inbox.heard.iter().position(|&(went, _)| went.distance(at) < HEARD_BLAST_MATCH_METRES) {
            inbox.heard.swap_remove(heard);
            continue;
        }
        explosions.write(Explosion {
            at,
            thrower: None,
            splash: Splash { max: 0.0, min: 0.0, radius: burst.radius },
            hurts_player: false,
            hurts_zombies: true,
            ammo_mods: AmmoMods::NONE,
            crawlers: false,
        });
    }
}

/// Flies what the Host said is up along its arc, stopping where it meets the world.
fn fly_cosmetics(
    mut commands: Commands,
    mut flying: Query<(Entity, &mut Cosmetic, &mut Transform)>,
    spatial: SpatialQuery,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let filter = world_filter();
    for (entity, mut thing, mut transform) in &mut flying {
        thing.life -= dt;
        if thing.life <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        if thing.landed {
            continue;
        }
        let step = thing.velocity * dt;
        if let Ok((direction, length)) = Dir3::new_and_length(step)
            && let Some(hit) = spatial.cast_ray(transform.translation, direction, length, true, &filter)
        {
            transform.translation += *direction * hit.distance;
            thing.landed = true;
            continue;
        }
        transform.translation += step;
        let gravity = thing.gravity;
        thing.velocity.y -= gravity * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::gizmos::AppGizmoBuilder;
    use crate::weapons::ammo_mod::{AmmoMod, AmmoModPlugin};
    use crate::weapons::burn::{Burning, BurnPlugin};
    use crate::net::movement::{IDLE, Run};
    use crate::players::player::{PlayerCamera, Seat};
    use crate::economy::points::{Points, PointsPlugin};
    use crate::enemies::zombie::{ZombieKilled, apply_damage};

    /// What the Host's Weapons would have done for the Players: hits become damage.
    fn extra(app: &mut App) {
        app.add_plugins((PointsPlugin, AmmoModPlugin, BurnPlugin))
            .init_asset::<bevy::gizmos::GizmoAsset>()
            .init_gizmo_group::<DefaultGizmoConfigGroup>()
            .add_message::<ZombieKilled>()
            .add_message::<crate::enemies::zombie::StunZombie>()
            .add_message::<crate::audio::PlayCue>()
            .insert_resource(Noted::default())
            .add_systems(Update, (apply_hit_reports.in_set(HostSimulation).before(apply_damage), note));
    }

    #[derive(Resource, Default)]
    struct Noted {
        launches: Vec<(Entity, Thrown)>,
        kills: u32,
        /// Who each kill was credited to, and by what.
        killers: Vec<(Option<Entity>, Attack)>,
        /// Blasts heard on this machine, whoever's.
        blasts: u32,
    }

    fn note(
        mut noted: ResMut<Noted>,
        mut launches: MessageReader<Launch>,
        mut kills: MessageReader<ZombieKilled>,
        mut blasts: MessageReader<Explosion>,
    ) {
        noted.blasts += blasts.read().count() as u32;
        for launch in launches.read() {
            noted.launches.push((launch.player, launch.thrown));
        }
        for kill in kills.read() {
            noted.kills += 1;
            noted.killers.push((kill.player, kill.attack));
        }
    }

    fn run(clients: u8) -> Run {
        let mut run = Run::with(clients, 2, 0.0, 0, &extra);
        for app in &mut run.apps {
            let world = app.world_mut();
            let players: Vec<Entity> = world.query_filtered::<Entity, With<Player>>().iter(world).collect();
            for player in players {
                world.entity_mut(player).insert(Points::default());
            }
        }
        run
    }

    /// `seat`'s Player as machine `on` has it.
    fn player(run: &mut Run, on: usize, seat: u8) -> Entity {
        let world = run.apps[on].world_mut();
        world
            .query_filtered::<(Entity, &Seat), With<Player>>()
            .iter(world)
            .find(|(_, held)| held.0 == seat)
            .unwrap()
            .0
    }

    fn earned(run: &mut Run, on: usize, seat: u8) -> u64 {
        let entity = player(run, on, seat);
        run.apps[on].world().get::<Points>(entity).unwrap().earned
    }

    fn balance(run: &mut Run, on: usize, seat: u8) -> u64 {
        let entity = player(run, on, seat);
        run.apps[on].world().get::<Points>(entity).unwrap().balance
    }

    /// The machine `on` shoots the Zombie the Host knows as `zombie`, in the head.
    fn shoot(run: &mut Run, on: usize, zombie: Entity, damage: f32) {
        let id = run.apps[0].world().resource::<ZombieIds>().id(zombie).unwrap();
        let shooter = player(run, on, on as u8);
        let world = run.apps[on].world_mut();
        let puppet = world.resource::<ZombieIds>().entity(id).expect("the client has the Zombie");
        world.write_message(HitReport { player: shooter, zombie: puppet, zone: HitZone::Head, damage, attack: Attack::Bullet, ammo_mods: AmmoMods::NONE, burn: None });
    }

    /// The machine `on` lands `times` bullets of `damage` from a Weapon with `ammo_mods` and `burn`.
    fn shoot_with(run: &mut Run, on: usize, zombie: Entity, times: u32, damage: f32, ammo_mods: AmmoMods, burn: Option<Burn>) {
        let id = run.apps[0].world().resource::<ZombieIds>().id(zombie).unwrap();
        let shooter = player(run, on, on as u8);
        let world = run.apps[on].world_mut();
        let puppet = world.resource::<ZombieIds>().entity(id).expect("the client has the Zombie");
        for _ in 0..times {
            world.write_message(HitReport { player: shooter, zombie: puppet, zone: HitZone::Torso, damage, attack: Attack::Bullet, ammo_mods, burn });
        }
    }

    fn health(run: &Run, zombie: Entity) -> f32 {
        run.apps[0].world().get::<Zombie>(zombie).unwrap().health()
    }

    fn kills(run: &Run) -> u32 {
        run.apps[0].world().resource::<Noted>().kills
    }

    #[test]
    fn a_hit_on_a_client_damages_the_host_zombie_and_the_kill_pays_the_client() {
        let mut run = run(1);
        let zombie = run.zombie_with(crate::maps::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0), 1000.0);
        run.step(IDLE, 40);
        shoot(&mut run, 1, zombie, 100.0);
        run.step(IDLE, 20);
        assert!(health(&run, zombie) < 950.0, "still {}", health(&run, zombie));
        assert!(earned(&mut run, 0, 1) > 0, "the hit pays the shooter");
        assert_eq!(earned(&mut run, 0, 0), 0);
        let hit_only = earned(&mut run, 0, 1);
        assert_eq!(kills(&run), 0);
        shoot(&mut run, 1, zombie, 5000.0);
        run.step(IDLE, 20);
        assert_eq!(kills(&run), 1);
        assert!(earned(&mut run, 0, 1) > hit_only, "the kill pays the shooter");
        assert_eq!(earned(&mut run, 0, 0), 0);
    }

    #[test]
    fn a_clients_ammo_mod_procs_on_the_host_and_its_kill_pays_the_client() {
        let mut run = run(1);
        // The Host's roll wants a camera to cast Dead Wire's sight lines from.
        run.apps[0].world_mut().spawn((PlayerCamera, GlobalTransform::default()));
        let zombie = run.zombie_with(crate::maps::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0), 1.0e6);
        run.step(IDLE, 40);
        // Each bullet does next to nothing; only a proc can kill it.
        let ammo_mods = AmmoMods::NONE.with(AmmoMod::BlastFurnace);
        shoot_with(&mut run, 1, zombie, 200, 1.0, ammo_mods, None);
        run.step(IDLE, 40);
        assert_eq!(kills(&run), 1, "the proc killed it");
        let host_copy = player(&mut run, 0, 1);
        assert_eq!(run.apps[0].world().resource::<Noted>().killers, [(Some(host_copy), Attack::AmmoMod(crate::enemies::zombie::Pay::Paid))]);
        assert!(earned(&mut run, 0, 1) > 0);
        assert_eq!(earned(&mut run, 0, 0), 0);
        // And it is that Player's own Cooldown that the proc started.
        let world = run.apps[0].world();
        assert!(!world.get::<crate::weapons::ammo_mod::Cooldowns>(host_copy).unwrap().ready(AmmoMod::BlastFurnace));
        let hosts_own = player(&mut run, 0, 0);
        assert!(run.apps[0].world().get::<crate::weapons::ammo_mod::Cooldowns>(hosts_own).unwrap().ready(AmmoMod::BlastFurnace));
    }

    #[test]
    fn a_clients_fire_bullet_ignites_the_host_zombie() {
        let mut run = run(1);
        let zombie = run.zombie_with(crate::maps::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0), 1000.0);
        run.step(IDLE, 40);
        shoot_with(&mut run, 1, zombie, 1, 1.0, AmmoMods::NONE, Some(Burn { damage: 20.0, tick_secs: 0.5, secs: 4.0 }));
        run.step(IDLE, 20);
        assert!(run.apps[0].world().get::<Burning>(zombie).is_some(), "set alight");
        let early = health(&run, zombie);
        run.step(IDLE, 120);
        assert!(health(&run, zombie) < early - 20.0, "burned from {early} to {}", health(&run, zombie));
        // A burn that cannot tick is not believed.
        assert!(burn_in([5.0, 0.0, 3.0]).is_none() && burn_in([f32::NAN, 0.5, 3.0]).is_none());
    }

    #[test]
    fn a_client_hears_its_own_blast_once() {
        let mut run = run(1);
        run.step(IDLE, 10);
        let at = crate::maps::map::PLAYER_START + Vec3::new(5.0, 1.0, 5.0);
        let thrower = player(&mut run, 1, 1);
        run.apps[1].world_mut().write_message(Explosion {
            at,
            thrower: Some(thrower),
            splash: Splash { max: 1.0, min: 1.0, radius: 3.0 },
            hurts_player: false,
            hurts_zombies: true,
            ammo_mods: AmmoMods::NONE,
            crawlers: false,
        });
        // The local blast, then the Host's burst of it.
        run.step(IDLE, 30);
        let heard = run.apps[1].world().resource::<Noted>().blasts;
        assert_eq!(heard, 1, "one sound, not two");
    }

    #[test]
    fn two_clients_hitting_the_same_zombie_kill_it_once_and_only_the_first_is_paid() {
        let mut run = run(2);
        let zombie = run.zombie_with(crate::maps::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0), 100.0);
        run.step(IDLE, 40);
        shoot(&mut run, 1, zombie, 500.0);
        run.step(IDLE, 15);
        assert_eq!(kills(&run), 1);
        // The second shot was already in the air when the Zombie fell, but reaches the Host after.
        shoot(&mut run, 2, zombie, 500.0);
        run.step(IDLE, 20);
        assert_eq!(kills(&run), 1);
        assert!(earned(&mut run, 0, 1) > 0);
        assert_eq!(earned(&mut run, 0, 2), 0, "a hit on a dead Zombie pays no one");
    }

    #[test]
    fn two_clients_each_get_their_own_points_for_their_own_kills() {
        let mut run = run(2);
        let start = crate::maps::map::PLAYER_START;
        let a = run.zombie_with(start + Vec3::new(8.0, 0.0, 4.0), 100.0);
        let b = run.zombie_with(start + Vec3::new(10.0, 0.0, 4.0), 100.0);
        run.step(IDLE, 40);
        shoot(&mut run, 1, a, 500.0);
        shoot(&mut run, 2, b, 500.0);
        run.step(IDLE, 30);
        assert_eq!(kills(&run), 2);
        let (one, two) = (earned(&mut run, 0, 1), earned(&mut run, 0, 2));
        assert!(one > 0 && one == two, "{one} and {two}");
        // And each client is told its own.
        assert_eq!(balance(&mut run, 1, 1), one);
        assert_eq!(balance(&mut run, 2, 2), two);
    }

    #[test]
    fn a_hit_on_a_zombie_the_host_no_longer_has_is_dropped() {
        let mut run = run(1);
        let zombie = run.zombie_with(crate::maps::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0), 1000.0);
        run.step(IDLE, 40);
        shoot(&mut run, 1, zombie, 100.0);
        // It is gone from the Host before the report gets there.
        run.apps[0].world_mut().entity_mut(zombie).despawn();
        run.step(IDLE, 20);
        assert_eq!(earned(&mut run, 0, 1), 0);
    }

    #[test]
    fn what_a_client_throws_is_simulated_on_the_host_and_flown_for_show_everywhere() {
        let mut run = run(2);
        run.step(IDLE, 10);
        let from = crate::maps::map::PLAYER_START + Vec3::new(0.0, 1.5, 0.0);
        let thrower = player(&mut run, 1, 1);
        run.apps[1].world_mut().write_message(Launch {
            player: thrower,
            thrown: Thrown::Frag { from, aim: Vec3::X, fuse: 3.0 },
        });
        run.step(IDLE, 10);
        // The Host was asked, for the Player it knows.
        let host_thrower = player(&mut run, 0, 1);
        let noted = run.apps[0].world().resource::<Noted>();
        assert_eq!(noted.launches.len(), 1);
        assert_eq!(noted.launches[0].0, host_thrower);
        assert!(matches!(noted.launches[0].1, Thrown::Frag { fuse: 3.0, .. }));
        // Everyone, the thrower too, sees it fly the same way.
        let mut at = Vec::new();
        for on in 1..=2 {
            let world = run.apps[on].world_mut();
            let found: Vec<_> = world.query::<(&Cosmetic, &Transform)>().iter(world).map(|(_, t)| t.translation).collect();
            assert_eq!(found.len(), 1, "machine {on}");
            assert!(found[0].x > from.x + 1.0, "machine {on} has it at {}", found[0]);
            at.push(found[0]);
        }
        assert!(at[0].distance(at[1]) < 1.0, "{at:?}");
        // The Host says it goes off, and it is gone.
        run.apps[0].world_mut().write_message(Explosion {
            at: at[0],
            thrower: None,
            splash: Splash { max: 1.0, min: 1.0, radius: 3.0 },
            hurts_player: false,
            hurts_zombies: true,
            ammo_mods: AmmoMods::NONE,
            crawlers: false,
        });
        run.step(IDLE, 10);
        for on in 1..=2 {
            let world = run.apps[on].world_mut();
            assert_eq!(world.query::<&Cosmetic>().iter(world).count(), 0, "machine {on}");
        }
    }

    #[test]
    fn a_throw_survives_the_wire_and_nonsense_is_not_believed() {
        use super::super::protocol::decode;
        let round = |direction| Throw::Launcher {
            origin: [1.0, 2.0, 3.0],
            direction,
            speed: 30.0,
            gravity: 18.0,
            impact: 100.0,
            impact_headshot: 2.0,
            headshots: true,
            splash: [500.0, 100.0, 4.0],
            ammo_mods: 0b11,
            crawlers: true,
        };
        let round_ahead = round([0.0, 0.0, -1.0]);
        assert_eq!(decode::<ToHost>(&encode(&ToHost::Launch(round_ahead))).unwrap(), ToHost::Launch(round_ahead));
        assert!(matches!(round_ahead.launch(), Some(Thrown::LauncherRound(_))));
        assert!(round([0.0; 3]).launch().is_none());
        let nan = Throw::Frag { from: [f32::NAN, 0.0, 0.0], aim: [1.0, 0.0, 0.0], fuse: 1.0 };
        assert!(nan.launch().is_none());
        let blast = Throw::Blast { at: [0.0; 3], splash: [1.0, 1.0, 1.0], ammo_mods: 0, crawlers: false, hurts_player: true };
        assert!(blast.launch().is_none() && blast.blast(Entity::PLACEHOLDER).is_some());
        let hit = Hit {
            tick: 4,
            zombie: 9,
            zone: HitZone::Head,
            damage: f32::INFINITY,
            knife: true,
            ammo_mods: 0b101,
            burn: Some([3.0, 0.5, 4.0]),
        };
        assert_eq!(decode::<ToHost>(&encode(&ToHost::Hit(hit))).unwrap(), ToHost::Hit(hit));
    }
}
