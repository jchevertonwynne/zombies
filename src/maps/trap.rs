//! Traps: electric fields across two ring doorways, bought from a switch on either side once the
//! Door is open and the Power is on. A running Trap kills every Zombie that walks into it, Unpaid,
//! and Downs a Player without Juggernog. Zombies plan their routes as if Traps weren't there.

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::economy::buy::{Buyable, Purchased, Refused};
use crate::maps::door::Door;
use crate::players::last_stand::LastStand;
use crate::modifier::{Effect, Modifier, Modifiers, Source, Stat, resolve, resolve_points};
use crate::economy::perk::{Perk, Perks};
use crate::players::player::{Locomotion, Player, PlayerHit, take_hits};
use crate::maps::power::Supply;
use crate::enemies::sapper::Sabotaged;
use crate::enemies::target::half_height_above_feet;
use crate::state::{GameState, InRun, world_runs};
use crate::tuning::*;
use crate::enemies::zombie::{Attack, DamageZombie, Dying, Heavyweight, HitZone, Rising, Zombie};
use crate::maps::map::textures::{Shape, Textured, box_uv_mesh};
use crate::maps::map::{MapId, STOREY};
use crate::maps::zone::ZoneId;

/// A Map's Traps: the Door whose doorway each spans, the doorway's centre on its floor (x, y, z),
/// and where along the wall (x) its two switches sit, one on each face. Every doorway is in a wall
/// running along x, so a Trap's width lies along x and its depth along z.
pub(crate) fn traps(map: MapId) -> &'static [((ZoneId, ZoneId), Vec3, f32)] {
    match map {
        MapId::Greybox => {
            const SPOTS: [((ZoneId, ZoneId), Vec3, f32); 2] = [
                ((ZoneId::WestHall, ZoneId::DiningRoom), Vec3::new(-20.0, 0.0, -14.0), -17.8),
                ((ZoneId::EastHall, ZoneId::PowerRoom), Vec3::new(20.0, 0.0, -14.0), 17.8),
            ];
            &SPOTS
        }
        // The Pump House's Door out to the pit floor.
        MapId::Quarry => {
            const SPOTS: [((ZoneId, ZoneId), Vec3, f32); 1] = [((ZoneId::PumpHouse, ZoneId::QuarryFloor), Vec3::new(0.0, 0.0, -20.0), 2.2)];
            &SPOTS
        }
        // The Doors off the High Street into the Smithy and the Schoolhouse, the cut-throughs to
        // the lanes nearest the Market.
        MapId::Village => {
            const SPOTS: [((ZoneId, ZoneId), Vec3, f32); 2] = [
                ((ZoneId::WestEnd, ZoneId::Smithy), Vec3::new(-32.0, 0.0, 6.0), -29.8),
                ((ZoneId::EastEnd, ZoneId::Schoolhouse), Vec3::new(32.0, 0.0, 6.0), 34.2),
            ];
            &SPOTS
        }
        // The doorways down from the Gate Shaft into the Intake Gallery and from the Spillway
        // Gallery onto the Spillway Stair, a storey up, and from the Turbine Hall out to the
        // Drainage Gallery: one in each Section.
        MapId::Dam => {
            const SPOTS: [((ZoneId, ZoneId), Vec3, f32); 3] = [
                ((ZoneId::GateShaft, ZoneId::IntakeGallery), Vec3::new(-41.0, STOREY, 2.0), -38.8),
                ((ZoneId::SpillwayGallery, ZoneId::SpillwayStair), Vec3::new(41.0, STOREY, -32.0), 38.8),
                ((ZoneId::TurbineHall, ZoneId::DrainageGallery), Vec3::new(0.0, 0.0, -56.0), 2.2),
            ];
            &SPOTS
        }
        // The foot of the Chapel Stair, the Long Stair and the Cliff Stair: each Door has a landing
        // before its stairs begin, where the switches are.
        MapId::Cove => {
            const SPOTS: [((ZoneId, ZoneId), Vec3, f32); 3] = [
                ((ZoneId::Washhouse, ZoneId::Chapel), Vec3::new(-50.0, STOREY, 24.0), -47.8),
                ((ZoneId::FishMarket, ZoneId::Piazza), Vec3::new(-12.0, STOREY, 24.0), -9.8),
                ((ZoneId::UpperLane, ZoneId::Battery), Vec3::new(12.0, 2.0 * STOREY, 44.0), 14.2),
            ];
            &SPOTS
        }
        // The Square's way into the Almshouse Court, the foot of the Dyke Steps and the foot of
        // the Coal Stair, all in walls along x.
        MapId::Polder => {
            const SPOTS: [((ZoneId, ZoneId), Vec3, f32); 3] = [
                ((ZoneId::TownSquare, ZoneId::AlmshouseCourt), Vec3::new(-6.0, 0.0, -40.0), -3.8),
                ((ZoneId::HerringQuay, ZoneId::DykeRoad), Vec3::new(-20.0, 0.0, 48.0), -17.8),
                ((ZoneId::CoalYard, ZoneId::PumpingStation), Vec3::new(68.0, 0.0, 48.0), 70.2),
            ];
            &SPOTS
        }
        // The Hall's front doors, the West Corridor's way into the Garden Court and the Court's way
        // into the Refectory, all in walls along x.
        MapId::Sanatorium => {
            const SPOTS: [((ZoneId, ZoneId), Vec3, f32); 3] = [
                ((ZoneId::EntranceHall, ZoneId::Forecourt), Vec3::new(0.0, 0.0, 0.0), -2.2),
                ((ZoneId::WestCorridor, ZoneId::GardenCourt), Vec3::new(-18.0, 0.0, 14.0), -15.8),
                ((ZoneId::GardenCourt, ZoneId::Refectory), Vec3::new(0.0, 0.0, 42.0), 2.2),
            ];
            &SPOTS
        }
        // The Theatre's dais to the Bindery on the Power route (D4), the Flooded Cellar to the Undercroft (D10) and the vault door into the
        // Strong Room (PD), all in walls along x. None is at a Range, because a Range is never a Trap.
        MapId::Archive => {
            const SPOTS: [((ZoneId, ZoneId), Vec3, f32); 3] = [
                ((ZoneId::LectureTheatre, ZoneId::Bindery), Vec3::new(26.0, STOREY, 24.0), 28.2),
                ((ZoneId::FloodedCellar, ZoneId::Undercroft), Vec3::new(-10.0, 0.0, 20.0), -7.8),
                ((ZoneId::FloodedCellar, ZoneId::StrongRoom), Vec3::new(10.0, 0.0, 20.0), 12.2),
            ];
            &SPOTS
        }
    }
}

/// The Zone a Trap across the Door between `joins` counts as standing in, for the Section that
/// Powers it: the first. Both are in the same Section, if either is.
pub fn trap_zone(joins: (ZoneId, ZoneId)) -> ZoneId {
    joins.0
}

/// How far each face of a doorway's wall is from its middle.
const WALL_FACE: f32 = 0.2;
const SWITCH_HEIGHT: f32 = 1.3;

/// Where every Trap's switches are on `map`: one on each face of its doorway's wall.
#[cfg(test)]
pub(crate) fn switch_spots(map: MapId) -> impl Iterator<Item = Vec3> {
    traps(map).iter().flat_map(|&(_, doorway, switch_x)| {
        [Vec3::Z, Vec3::NEG_Z].map(|facing| Vec3::new(switch_x, doorway.y + SWITCH_HEIGHT, doorway.z) + facing * (WALL_FACE + 0.03))
    })
}

/// The shock's slow goes through the stat-modifier seam as its own source.
const SHOCK_SOURCE: Source = Source("Trap Shock");

/// Where a Trap is in its cycle, with the seconds spent in that phase.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TrapPhase {
    /// Ready to be bought.
    Idle,
    /// The switch is being thrown.
    Flipping(f32),
    Running(f32),
    Cooling(f32),
}

impl TrapPhase {
    /// Throw the switch. Only an idle Trap starts.
    pub fn start(&mut self) {
        if *self == TrapPhase::Idle {
            *self = TrapPhase::Flipping(0.0);
        }
    }

    /// Advance the cycle: flip, run, cool down for `cooldown_secs`, then idle again.
    pub fn advance(&mut self, dt: f32, cooldown_secs: f32) {
        let mut left = dt;
        loop {
            let (elapsed, length, next) = match self {
                TrapPhase::Idle => return,
                TrapPhase::Flipping(elapsed) => (elapsed, TRAP_FLIP_SECS, TrapPhase::Running(0.0)),
                TrapPhase::Running(elapsed) => (elapsed, TRAP_RUN_SECS, TrapPhase::Cooling(0.0)),
                TrapPhase::Cooling(elapsed) => (elapsed, cooldown_secs, TrapPhase::Idle),
            };
            // Time past the end of one phase carries into the next.
            if *elapsed + left < length {
                *elapsed += left;
                return;
            }
            left -= length - *elapsed;
            *self = next;
        }
    }

    pub fn running(self) -> bool {
        matches!(self, TrapPhase::Running(_))
    }
}

/// What a Trap's switches show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapPrompt {
    /// Nothing: the Door is still shut, or the Trap is busy.
    NoPrompt,
    /// "You must turn on the Power first", with no price; the Use key buzzes.
    NeedsPower,
    Buy,
}

/// Whether a Trap can be bought: only once its Door is open and the Power is on, and only when
/// it's idle (not flipping, running or cooling down).
pub fn trap_prompt(door_open: bool, power_on: bool, phase: TrapPhase) -> TrapPrompt {
    if !door_open {
        TrapPrompt::NoPrompt
    } else if !power_on {
        TrapPrompt::NeedsPower
    } else if phase == TrapPhase::Idle {
        TrapPrompt::Buy
    } else {
        TrapPrompt::NoPrompt
    }
}

/// What touching a running Trap does to the Player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerTouch {
    Nothing,
    /// Without Juggernog: straight Down.
    Down,
    /// With Juggernog: `TRAP_PLAYER_DAMAGE` and a `TRAP_SHOCK_SECS` shock.
    Shock,
}

/// The Player touching a running Trap: immune in Last Stand, Downed without Juggernog, shocked
/// with it. A shock already under way is the same touch, so standing inside shocks again only
/// once it ends.
pub fn player_touch(juggernog: bool, last_stand: bool, shocked: bool) -> PlayerTouch {
    if last_stand {
        PlayerTouch::Nothing
    } else if !juggernog {
        PlayerTouch::Down
    } else if shocked {
        PlayerTouch::Nothing
    } else {
        PlayerTouch::Shock
    }
}

/// How long a Zombie lasts after touching a running Trap, for a uniform `roll` in [0, 1).
pub fn kill_delay(roll: f32) -> f32 {
    roll * TRAP_KILL_DELAY_MAX_SECS
}

/// A `Heavyweight`'s hit from one entry into a running Trap: its `share` of its max Health, and
/// the stun.
pub fn heavyweight_trap_hit(max_health: f32, share: f32) -> (f32, f32) {
    (max_health * share, TRAP_SAPPER_STUN_SECS)
}

/// The Sapper's hit from one entry into a running Trap: damage from its max Health, and the stun.
#[cfg(test)]
pub fn sapper_trap_hit(max_health: f32) -> (f32, f32) {
    heavyweight_trap_hit(max_health, crate::enemies::zombie::Heavyweight::SAPPER.trap_share)
}

/// Whether an upright capsule centred at `at`, `radius` round and `half_height` from its centre to
/// either end, touches the box centred at `centre` with half-extents `half`.
pub fn touches(centre: Vec3, half: Vec3, at: Vec3, radius: f32, half_height: f32) -> bool {
    let offset = at - centre;
    let outside = Vec2::new(offset.x.abs() - half.x, offset.z.abs() - half.z).max(Vec2::ZERO);
    outside.length() <= radius && offset.y.abs() <= half.y + half_height
}

/// An electric Trap across the doorway of the Door joining `joins`. A Sabotaged Trap stops and
/// shows no Prompt until it's restored.
#[derive(Component)]
pub struct Trap {
    joins: (ZoneId, ZoneId),
    phase: TrapPhase,
    /// The Player who threw the switch that is running it, whose kills it credits.
    activator: Option<Entity>,
}

impl Trap {
    /// The two Zones whose Door it spans.
    pub fn joins(&self) -> (ZoneId, ZoneId) {
        self.joins
    }

    /// Where it is in its cycle.
    pub fn phase(&self) -> TrapPhase {
        self.phase
    }

    /// Shows the phase the Host says it is in.
    pub fn show(&mut self, phase: TrapPhase) {
        self.phase = phase;
    }

    /// Runs the shown phase on, so it plays out until the Host says what comes next.
    pub fn advance_shown(&mut self, dt: f32) {
        self.phase.advance(dt, TRAP_COOLDOWN_SECS);
    }

    /// Whether it's running, zapping everything inside.
    pub fn running(&self) -> bool {
        self.phase.running()
    }
}

/// One of a Trap's two switches, the Buyable that runs it.
#[derive(Component)]
pub(crate) struct TrapSwitch {
    trap: Entity,
    use_point: Vec3,
}

/// The lamp on a switch, lit while its Trap can be bought.
#[derive(Component)]
struct SwitchLight;

/// A Zombie that touched a running Trap, dying in `left` seconds.
#[derive(Component)]
pub struct Electrocuted {
    /// The Player whose Trap it was.
    player: Option<Entity>,
    left: f32,
}

/// The Player, shocked by a running Trap through Juggernog: slowed for `left` seconds.
#[derive(Component)]
pub struct Shocked {
    left: f32,
}

/// A `Heavyweight` Zombie inside a running Trap, so it's struck once per entry.
#[derive(Component)]
struct InTrap;

/// A running Trap struck a `Heavyweight` Zombie as it entered, which takes `heavyweight_trap_hit`.
#[derive(Message, Debug, Clone, Copy)]
pub struct TrapStruck {
    /// The Player whose Trap it was.
    pub player: Option<Entity>,
    pub zombie: Entity,
}

pub struct TrapPlugin;

impl Plugin for TrapPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TrapStruck>()
            .add_systems(OnEnter(GameState::Playing), spawn_traps)
            .add_systems(
                Update,
                (
                    (
                        (use_switches, run_traps).chain().in_set(HostSimulation),
                        prompt_switches,
                        show_fields,
                    )
                        .chain(),
                    (shock_zombies, electrocute).chain().in_set(HostSimulation),
                    shock_player.before(take_hits).in_set(HostSimulation),
                )
                    .run_if(world_runs),
            );
    }
}

fn spawn_traps(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    map: Res<MapId>,
) {
    let field = meshes.add(Cuboid::from_size(TRAP_SIZE));
    let field_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.5, 0.8, 1.0, 0.25),
        emissive: LinearRgba::rgb(2.0, 4.0, 8.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let steel = crate::maps::machine_look::STEEL;
    let panel = meshes.add(box_uv_mesh(Vec3::new(0.4, 0.6, 0.06), Vec3::ZERO, Quat::IDENTITY, steel.tile()));
    let panel_mat = materials.add(Color::srgb(0.25, 0.27, 0.3));
    // The panel's lit screen: a pale blue readout, always glowing.
    let screen = meshes.add(Cuboid::new(0.28, 0.14, 0.02));
    let screen_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.7, 1.0),
        emissive: LinearRgba::rgb(0.6, 1.6, 3.0),
        ..default()
    });
    let lamp = meshes.add(Cuboid::new(0.12, 0.12, 0.04));
    let lamp_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.9, 0.3),
        emissive: LinearRgba::rgb(1.0, 6.0, 1.5),
        ..default()
    });

    for &(joins, doorway, switch_x) in traps(*map) {
        let trap = commands
            .spawn((
                InRun,
                Name::new("Trap"),
                Trap {
                    joins,
                    phase: TrapPhase::Idle,
                    activator: None,
                },
                Mesh3d(field.clone()),
                MeshMaterial3d(field_mat.clone()),
                Transform::from_xyz(doorway.x, doorway.y + TRAP_SIZE.y / 2.0, doorway.z),
                Visibility::Hidden,
            ))
            .id();
        // A switch on each face of the wall, facing out into its room.
        for facing in [Vec3::Z, Vec3::NEG_Z] {
            let at = Vec3::new(switch_x, doorway.y + SWITCH_HEIGHT, doorway.z) + facing * (WALL_FACE + 0.03);
            commands.spawn((
                InRun,
                Name::new("Trap Switch"),
                TrapSwitch {
                    trap,
                    use_point: at + facing * 0.1,
                },
                Mesh3d(panel.clone()),
                MeshMaterial3d(panel_mat.clone()),
                Textured { material: steel, shape: Shape::Baked },
                Transform::from_translation(at).looking_to(-facing, Vec3::Y),
                children![
                    (
                        SwitchLight,
                        Mesh3d(lamp.clone()),
                        MeshMaterial3d(lamp_mat.clone()),
                        // The panel's +Z faces out into the room.
                        Transform::from_xyz(0.0, 0.18, 0.04),
                        Visibility::Hidden,
                    ),
                    (Mesh3d(screen.clone()), MeshMaterial3d(screen_mat.clone()), Transform::from_xyz(0.0, -0.08, 0.035)),
                ],
            ));
        }
    }
}

/// A bought switch throws its Trap's switch.
fn use_switches(
    mut purchases: MessageReader<Purchased>,
    switches: Query<&TrapSwitch>,
    mut traps: Query<&mut Trap>,
) {
    for &Purchased { player, buyable } in purchases.read() {
        if let Ok(switch) = switches.get(buyable)
            && let Ok(mut trap) = traps.get_mut(switch.trap)
        {
            trap.phase.start();
            trap.activator = Some(player);
        }
    }
}

/// The cooldown resolves through the modifier seam from the Player's entries. A Sabotaged Trap
/// goes dead at once, even mid-run, and so does one whose Section loses Power.
fn run_traps(
    mut traps: Query<(&mut Trap, Has<Sabotaged>)>,
    player: Query<&Modifiers, With<Player>>,
    supply: Supply,
    time: Res<Time>,
) {
    let cooldown = resolve(Stat::TrapCooldown, TRAP_COOLDOWN_SECS, player.iter().flat_map(|modifiers| &modifiers.0));
    for (mut trap, sabotaged) in &mut traps {
        // A Trap whose Section loses its Generator stops at once.
        let unpowered = supply.cut_off(Some(trap_zone(trap.joins)));
        if sabotaged || unpowered {
            trap.phase = TrapPhase::Idle;
        } else if trap.phase != TrapPhase::Idle {
            trap.phase.advance(time.delta_secs(), cooldown);
        }
    }
}

/// Switches own their Buyables: a buy, the Power refusal, or no Prompt at all (always, while the
/// Trap is Sabotaged). A lit lamp marks a Trap ready to buy.
fn prompt_switches(
    mut commands: Commands,
    switches: Query<(Entity, &TrapSwitch, &Children, Option<&Buyable>, Has<Refused>)>,
    mut lights: Query<&mut Visibility, With<SwitchLight>>,
    traps: Query<(&Trap, Has<Sabotaged>)>,
    doors: Query<&Door>,
    supply: Supply,
    player: Query<&Modifiers, With<Player>>,
) {
    let price = resolve_points(Stat::TrapPrice, TRAP_PRICE, player.iter().flat_map(|modifiers| &modifiers.0));
    for (entity, switch, children, buyable, refused) in &switches {
        let Ok((trap, sabotaged)) = traps.get(switch.trap) else { continue };
        let door_open = doors.iter().any(|door| door.open && door.joins == Some(trap.joins));
        let want = if sabotaged {
            TrapPrompt::NoPrompt
        } else {
            trap_prompt(door_open, supply.powers(Some(trap_zone(trap.joins))), trap.phase)
        };
        for child in children {
            if let Ok(mut light) = lights.get_mut(*child) {
                let shown = if want == TrapPrompt::Buy { Visibility::Inherited } else { Visibility::Hidden };
                light.set_if_neq(shown);
            }
        }
        let current = match (buyable.is_some(), refused) {
            (false, _) => TrapPrompt::NoPrompt,
            (true, true) => TrapPrompt::NeedsPower,
            (true, false) => TrapPrompt::Buy,
        };
        // A changed price (from the modifier seam) re-offers the buy.
        let repriced = want == TrapPrompt::Buy && buyable.is_some_and(|buyable| buyable.price != Some(price));
        if want == current && !repriced {
            continue;
        }
        let buyable = |prompt: &str, price| Buyable {
            use_point: switch.use_point,
            prompt: prompt.into(),
            price,
            range: USE_RANGE,
        };
        let mut switch_entity = commands.entity(entity);
        match want {
            TrapPrompt::NoPrompt => {
                switch_entity.remove::<(Buyable, Refused)>();
            }
            TrapPrompt::NeedsPower => {
                switch_entity.insert((buyable(&supply.refusal(Some(trap_zone(trap.joins))), None), Refused));
            }
            TrapPrompt::Buy => {
                switch_entity
                    .remove::<Refused>()
                    .insert(buyable("Press F to activate the Electric Trap", Some(price)));
            }
        }
    }
}

/// The field shows while the Trap runs.
fn show_fields(mut traps: Query<(&Trap, &mut Visibility)>) {
    for (trap, mut visibility) in &mut traps {
        let shown = if trap.phase.running() { Visibility::Inherited } else { Visibility::Hidden };
        visibility.set_if_neq(shown);
    }
}

/// The running Traps' volumes, as (centre, half-extents).
fn running_volumes<'a>(traps: impl IntoIterator<Item = (&'a Trap, &'a Transform)>) -> Vec<(Vec3, Vec3)> {
    traps
        .into_iter()
        .filter(|(trap, _)| trap.phase.running())
        .map(|(_, transform)| (transform.translation, TRAP_SIZE / 2.0))
        .collect()
}

/// A Zombie that touches a running Trap is electrocuted and dies a moment later. A `Heavyweight`
/// is struck once each time it enters one instead.
fn shock_zombies(
    mut commands: Commands,
    zombies: Query<
        (Entity, &Transform, Has<Heavyweight>, Has<InTrap>),
        (With<Zombie>, Without<Dying>, Without<Rising>, Without<Electrocuted>),
    >,
    traps: Query<(&Trap, &Transform)>,
    mut struck: MessageWriter<TrapStruck>,
) {
    let volumes: Vec<_> = traps
        .iter()
        .filter(|(trap, _)| trap.phase.running())
        .map(|(trap, transform)| (transform.translation, TRAP_SIZE / 2.0, trap.activator))
        .collect();
    let half_height = ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0;
    for (entity, transform, resists, in_trap) in &zombies {
        let touched = volumes
            .iter()
            .find(|&&(centre, half, _)| touches(centre, half, transform.translation, ZOMBIE_RADIUS, half_height));
        let inside = touched.is_some();
        let player = touched.and_then(|&(.., activator)| activator);
        match (inside, resists) {
            (true, false) => {
                commands.entity(entity).insert(Electrocuted {
                    player,
                    left: kill_delay(rand::random()),
                });
            }
            (true, true) if !in_trap => {
                commands.entity(entity).insert(InTrap);
                struck.write(TrapStruck { player, zombie: entity });
            }
            (false, true) if in_trap => {
                commands.entity(entity).remove::<InTrap>();
            }
            _ => {}
        }
    }
}

/// An electrocuted Zombie dies when its moment is up: an Unpaid kill that still counts.
fn electrocute(
    mut commands: Commands,
    mut zombies: Query<(Entity, &mut Electrocuted), Without<Dying>>,
    mut damage: MessageWriter<DamageZombie>,
    time: Res<Time>,
) {
    for (entity, mut electrocuted) in &mut zombies {
        electrocuted.left -= time.delta_secs();
        if electrocuted.left <= 0.0 {
            damage.write(DamageZombie {
                player: electrocuted.player,
                zombie: entity,
                damage: f32::INFINITY,
                zone: HitZone::Torso,
                attack: Attack::Trap,
            });
            commands.entity(entity).remove::<Electrocuted>();
        }
    }
}

/// A running Trap Downs a Player without Juggernog, and shocks them through it, again each
/// time a shock ends with them still inside.
fn shock_player(
    mut commands: Commands,
    mut players: Query<
        (Entity, &Transform, &Perks, &mut Modifiers, Option<&mut Shocked>, Has<LastStand>, Option<&Locomotion>),
        With<Player>,
    >,
    traps: Query<(&Trap, &Transform)>,
    mut hits: MessageWriter<PlayerHit>,
    time: Res<Time>,
) {
    let running = running_volumes(traps.iter());
    for (entity, transform, perks, mut modifiers, shocked, last_stand, locomotion) in &mut players {
        let mut still_shocked = false;
        if let Some(mut shocked) = shocked {
            shocked.left -= time.delta_secs();
            if shocked.left <= 0.0 {
                modifiers.0.retain(|entry| entry.source != SHOCK_SOURCE);
                commands.entity(entity).remove::<Shocked>();
            } else {
                still_shocked = true;
            }
        }

        // The Player's current Stance shape, not a hard-coded standing capsule: the height the
        // Trap's volume has to reach is how far their centre sits above their feet right now.
        let half_height = half_height_above_feet(locomotion);
        let Some(&(trap_at, _)) = running
            .iter()
            .find(|&&(centre, half)| touches(centre, half, transform.translation, PLAYER_RADIUS, half_height))
        else {
            continue;
        };
        match player_touch(perks.has(Perk::Juggernog), last_stand, still_shocked) {
            PlayerTouch::Nothing => {}
            // Enough to empty any Health; the Down itself is the Last Stand module's.
            PlayerTouch::Down => {
                hits.write(PlayerHit {
                    player: entity,
                    damage: f32::MAX,
                    from: Some(trap_at),
                });
            }
            PlayerTouch::Shock => {
                hits.write(PlayerHit {
                    player: entity,
                    damage: TRAP_PLAYER_DAMAGE,
                    from: Some(trap_at),
                });
                modifiers.0.retain(|entry| entry.source != SHOCK_SOURCE);
                modifiers.0.push(Modifier {
                    stat: Stat::MoveSpeed,
                    source: SHOCK_SOURCE,
                    effect: Effect::Mult(TRAP_SHOCK_SPEED_MULT),
                });
                commands.entity(entity).insert(Shocked {
                    left: TRAP_SHOCK_SECS,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// The Dam has a Trap in each Section, both sides of its doorway in it.
    #[test]
    fn the_dams_traps_stand_one_in_each_section() {
        use crate::maps::generator::SectionId;
        let dam = MapId::Dam.layout();
        let sections: Vec<_> = traps(MapId::Dam)
            .iter()
            .map(|&((a, b), ..)| {
                assert_eq!(dam.section_of(a), dam.section_of(b), "a Trap between two Sections");
                trap_zone((a, b))
            })
            .map(|zone| dam.section_of(zone))
            .collect();
        assert_eq!(sections, [Some(SectionId::IntakeTower), Some(SectionId::SpillwayGallery), Some(SectionId::TurbineHall)]);
    }

    use super::*;
    use crate::enemies::nav::zone_at;

    fn after(secs: &[f32]) -> TrapPhase {
        let mut phase = TrapPhase::Idle;
        phase.start();
        for &dt in secs {
            phase.advance(dt, TRAP_COOLDOWN_SECS);
        }
        phase
    }

    #[test]
    fn a_trap_needs_its_door_open_and_the_power_on() {
        assert_eq!(trap_prompt(false, false, TrapPhase::Idle), TrapPrompt::NoPrompt);
        assert_eq!(trap_prompt(false, true, TrapPhase::Idle), TrapPrompt::NoPrompt);
        assert_eq!(trap_prompt(true, false, TrapPhase::Idle), TrapPrompt::NeedsPower);
        assert_eq!(trap_prompt(true, true, TrapPhase::Idle), TrapPrompt::Buy);
    }

    #[test]
    fn a_busy_trap_cant_be_bought() {
        for phase in [TrapPhase::Flipping(0.1), TrapPhase::Running(5.0), TrapPhase::Cooling(30.0)] {
            assert_eq!(trap_prompt(true, true, phase), TrapPrompt::NoPrompt);
        }
    }

    #[test]
    fn a_trap_flips_runs_for_40_seconds_then_cools_down_for_60() {
        assert!(!after(&[TRAP_FLIP_SECS - 0.01]).running());
        assert!(after(&[TRAP_FLIP_SECS]).running());
        assert!(after(&[TRAP_FLIP_SECS, TRAP_RUN_SECS - 0.1]).running());
        assert!(matches!(after(&[TRAP_FLIP_SECS, TRAP_RUN_SECS]), TrapPhase::Cooling(_)));
        assert!(matches!(after(&[TRAP_FLIP_SECS, TRAP_RUN_SECS, TRAP_COOLDOWN_SECS - 0.1]), TrapPhase::Cooling(_)));
        assert_eq!(after(&[TRAP_FLIP_SECS, TRAP_RUN_SECS, TRAP_COOLDOWN_SECS]), TrapPhase::Idle);
        assert_eq!((TRAP_PRICE, TRAP_FLIP_SECS, TRAP_RUN_SECS, TRAP_COOLDOWN_SECS), (1000, 0.5, 40.0, 60.0));
    }

    #[test]
    fn a_shorter_cooldown_idles_the_trap_sooner() {
        let mut phase = TrapPhase::Cooling(0.0);
        phase.advance(47.5, 48.0);
        assert!(matches!(phase, TrapPhase::Cooling(_)));
        phase.advance(0.5, 48.0);
        assert_eq!(phase, TrapPhase::Idle);
    }

    #[test]
    fn time_past_a_phase_carries_into_the_next() {
        assert_eq!(after(&[TRAP_FLIP_SECS + 1.0]), TrapPhase::Running(1.0));
    }

    #[test]
    fn throwing_a_busy_switch_does_nothing() {
        let mut phase = after(&[TRAP_FLIP_SECS + 10.0]);
        phase.start();
        assert_eq!(phase, TrapPhase::Running(10.0));
    }

    #[test]
    fn without_juggernog_a_touch_downs_the_player() {
        assert_eq!(player_touch(false, false, false), PlayerTouch::Down);
    }

    #[test]
    fn with_juggernog_a_touch_shocks_and_repeats_once_the_shock_ends() {
        assert_eq!(player_touch(true, false, false), PlayerTouch::Shock);
        assert_eq!(player_touch(true, false, true), PlayerTouch::Nothing);
    }

    #[test]
    fn the_player_is_immune_in_last_stand() {
        for juggernog in [false, true] {
            assert_eq!(player_touch(juggernog, true, false), PlayerTouch::Nothing);
        }
    }

    #[test]
    fn zombies_die_within_a_second_and_a_quarter() {
        assert_eq!(kill_delay(0.0), 0.0);
        assert!(kill_delay(0.999) < TRAP_KILL_DELAY_MAX_SECS);
        assert!((kill_delay(0.5) - 0.625).abs() < 1e-6);
    }

    #[test]
    fn the_sapper_takes_a_fifth_of_its_max_health_and_a_two_second_stun() {
        assert_eq!(sapper_trap_hit(1000.0), (200.0, 2.0));
    }

    #[test]
    fn touching_means_overlapping_the_volume() {
        let (centre, half) = (Vec3::new(0.0, 1.5, 0.0), TRAP_SIZE / 2.0);
        let at = |x, z| Vec3::new(x, 1.0, z);
        assert!(touches(centre, half, at(0.0, 0.0), 0.35, 0.9));
        // Just brushing the edge, across the doorway and through it.
        assert!(touches(centre, half, at(1.5, 0.0), 0.35, 0.9));
        assert!(touches(centre, half, at(0.0, 1.55), 0.35, 0.9));
        assert!(!touches(centre, half, at(1.6, 0.0), 0.35, 0.9));
        assert!(!touches(centre, half, at(1.5, 1.5), 0.35, 0.9));
        // On the Balcony overhead.
        assert!(!touches(centre, half, Vec3::new(0.0, 5.0, 0.0), 0.35, 0.9));
    }

    #[test]
    fn each_trap_has_a_switch_in_both_zones_its_door_joins() {
        for &((a, b), doorway, switch_x) in MapId::ALL.iter().flat_map(|&map| traps(map)) {
            let sides: Vec<_> = [WALL_FACE + 0.1, -WALL_FACE - 0.1]
                .into_iter()
                .map(|out| zone_at(a.map(), Vec3::new(switch_x, doorway.y + SWITCH_HEIGHT, doorway.z + out)))
                .collect();
            assert!(sides.contains(&Some(a)) && sides.contains(&Some(b)), "{} - {}", a.name(), b.name());
            // Clear of the field, so throwing it doesn't touch it.
            assert!((switch_x - doorway.x).abs() - TRAP_SIZE.x / 2.0 > PLAYER_RADIUS);
        }
    }
}

/// Traps with two Players: the buyer pays, anyone walking in is Downed, and the kills pay nobody.
#[cfg(test)]
mod two_player_tests {
    use super::*;
    use crate::economy::buy::UseRequest;
    use crate::players::player::two_players::{balance, earned, feet, place, two_players_with};
    use crate::enemies::zombie::headless::Sim;

    /// A Trap standing on the Start Room's floor at the origin, with a switch to buy it.
    fn setup() -> (Sim, Entity, Entity, Entity, Entity) {
        let (mut sim, local, teammate) = two_players_with(|app| {
            app.add_message::<TrapStruck>().add_systems(
                Update,
                ((use_switches, run_traps).chain(), (shock_zombies, electrocute).chain(), shock_player),
            );
        });
        for player in [local, teammate] {
            sim.world_mut().entity_mut(player).insert(Perks::default());
            sim.world_mut().get_mut::<crate::economy::points::Points>(player).unwrap().balance = 10_000;
        }
        let trap = sim
            .world_mut()
            .spawn((
                Trap { joins: (ZoneId::StartRoom, ZoneId::StartRoom), phase: TrapPhase::Idle, activator: None },
                Transform::from_xyz(0.0, TRAP_SIZE.y / 2.0, 0.0),
            ))
            .id();
        let switch = sim
            .world_mut()
            .spawn((
                TrapSwitch { trap, use_point: Vec3::ZERO },
                Buyable { use_point: Vec3::ZERO, prompt: String::new(), price: Some(TRAP_PRICE), range: USE_RANGE },
            ))
            .id();
        (sim, local, teammate, trap, switch)
    }

    /// Moves the Trap to where `player` stands, so they're inside it.
    fn step_into(sim: &mut Sim, trap: Entity, player: Entity) {
        let at = sim.world_mut().get::<Transform>(player).unwrap().translation;
        sim.world_mut().get_mut::<Transform>(trap).unwrap().translation = at;
    }

    fn downed(sim: &mut Sim, frames: usize) -> Vec<Entity> {
        let mut hit = Vec::new();
        for _ in 0..frames {
            sim.run(1);
            hit.extend(sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().map(|hit| hit.player));
        }
        hit
    }

    #[test]
    fn the_buyer_pays_and_the_trap_is_theirs() {
        let (mut sim, local, teammate, trap, switch) = setup();
        let (local_before, teammate_before) = (balance(&mut sim, local), balance(&mut sim, teammate));
        sim.send(UseRequest { player: teammate, buyable: switch, hold: false });
        sim.run(2);
        assert_eq!(balance(&mut sim, teammate), teammate_before - TRAP_PRICE);
        assert_eq!(balance(&mut sim, local), local_before);
        assert_eq!(sim.world_mut().get::<Trap>(trap).unwrap().activator, Some(teammate));
        sim.run(sim.frames(TRAP_FLIP_SECS + 0.5));
        assert!(sim.world_mut().get::<Trap>(trap).unwrap().running());
    }

    #[test]
    fn a_running_trap_downs_any_player_who_walks_in_the_buyer_included() {
        let (mut sim, local, teammate, trap, _) = setup();
        let mut trap_state = sim.world_mut().get_mut::<Trap>(trap).unwrap();
        trap_state.phase = TrapPhase::Running(0.0);
        trap_state.activator = Some(teammate);
        // Stand the buyer well clear of the other Player.
        let clear = feet(&mut sim, local) + Vec3::new(6.0, 0.0, 0.0);
        place(&mut sim, teammate, clear);
        assert!(downed(&mut sim, 5).is_empty(), "nobody is in it yet");

        step_into(&mut sim, trap, local);
        let hit = downed(&mut sim, 3);
        assert!(hit.contains(&local) && !hit.contains(&teammate), "the Player who walked in is Downed alone");

        step_into(&mut sim, trap, teammate);
        assert!(downed(&mut sim, 3).contains(&teammate), "the buyer walked in and wasn't Downed");
    }

    #[test]
    fn a_running_trap_tests_the_players_current_stance_shape() {
        use crate::players::player::{MoveInput, Stance};
        let (mut sim, _local, teammate, trap, _) = setup();
        sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().crouch = true;
        sim.run(10);
        assert_eq!(sim.world_mut().get::<Locomotion>(teammate).unwrap().stance, Stance::Crouched);
        {
            let mut trap_state = sim.world_mut().get_mut::<Trap>(trap).unwrap();
            trap_state.phase = TrapPhase::Running(0.0);
            trap_state.activator = Some(teammate);
        }
        // Hover the volume over the teammate's head, clear of the local Player: out of the
        // Crouched body's reach — nothing about the Stance reaches it — where the standing
        // capsule the Trap used to test would still touch.
        let at = sim.world_mut().get::<Transform>(teammate).unwrap().translation;
        let (x, z) = (at.x, at.z);
        *sim.world_mut().get_mut::<Transform>(trap).unwrap() = Transform::from_xyz(x, 3.0, z);
        assert!(downed(&mut sim, 5).is_empty(), "the running Trap reached a Crouched body it doesn't touch");
        // Down on the floor where the Trap stands, the same Crouched body is inside it: nobody
        // ducks under a running Trap.
        *sim.world_mut().get_mut::<Transform>(trap).unwrap() = Transform::from_xyz(x, TRAP_SIZE.y / 2.0, z);
        assert!(downed(&mut sim, 3).contains(&teammate), "the running Trap let a Crouched Player duck under");
    }

    /// A running Trap in a Section stops at once when its Generator is Lost.
    #[test]
    fn a_running_trap_stops_at_once_when_its_section_loses_power() {
        use crate::maps::generator::{Generators, SectionId};
        use crate::maps::map::MapId;
        let (mut sim, _, _, trap, _) = setup();
        let turbine = MapId::Dam.layout().generators.iter().position(|g| g.section == SectionId::TurbineHall).unwrap();
        let mut generators = Generators::for_map(MapId::Dam);
        generators.hold(turbine);
        sim.world_mut().insert_resource(generators);
        let mut running = sim.world_mut().get_mut::<Trap>(trap).unwrap();
        running.joins = (ZoneId::TurbineHall, ZoneId::DrainageGallery);
        running.phase = TrapPhase::Running(0.0);
        sim.run(sim.frames(1.0));
        assert!(sim.world_mut().get::<Trap>(trap).unwrap().running(), "Powered, it runs on");
        sim.world_mut().resource_mut::<Generators>().lose(turbine);
        sim.run(1);
        assert_eq!(sim.world_mut().get::<Trap>(trap).unwrap().phase, TrapPhase::Idle);
    }

    #[test]
    fn a_trap_kill_pays_nobody() {
        let (mut sim, local, teammate, trap, _) = setup();
        let mut trap_state = sim.world_mut().get_mut::<Trap>(trap).unwrap();
        trap_state.phase = TrapPhase::Running(0.0);
        trap_state.activator = Some(teammate);
        sim.spawn(Vec3::ZERO, None);
        sim.run(sim.frames(TRAP_KILL_DELAY_MAX_SECS + 1.0));
        let world = sim.world_mut();
        assert_eq!(world.query_filtered::<(), (With<Zombie>, Without<Dying>)>().iter(world).count(), 0, "it lived");
        assert_eq!(earned(&mut sim, teammate), 0, "the buyer was paid for a Trap kill");
        assert_eq!(earned(&mut sim, local), 0);
    }
}
