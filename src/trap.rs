//! Traps: electric fields across two ring doorways, bought from a switch on either side once the
//! Door is open and the Power is on. A running Trap kills every Zombie that walks into it, Unpaid,
//! and Downs a Player without Juggernog. Zombies plan their routes as if Traps weren't there.

use bevy::prelude::*;

use crate::buy::{Buyable, Purchased, Refused};
use crate::door::Door;
use crate::last_stand::LastStand;
use crate::modifier::{Effect, Modifier, Modifiers, Source, Stat, resolve, resolve_points};
use crate::perk::{Perk, Perks};
use crate::player::{Player, PlayerHit, take_hits};
use crate::power::Power;
use crate::sapper::Sabotaged;
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::zombie::{Attack, DamageZombie, Dying, HitZone, Rising, Zombie};
use crate::zone::ZoneId;

/// The Traps: the Door whose doorway each spans, the doorway's centre on the floor (x, z), and
/// where along the wall (x) its two switches sit, one on each face. Both doorways are in walls
/// running along x, so a Trap's width lies along x and its depth along z.
const TRAPS: [((ZoneId, ZoneId), Vec2, f32); 2] = [
    ((ZoneId::WestHall, ZoneId::DiningRoom), Vec2::new(-20.0, -14.0), -17.8),
    ((ZoneId::EastHall, ZoneId::PowerRoom), Vec2::new(20.0, -14.0), 17.8),
];

/// How far each face of a doorway's wall is from its middle.
const WALL_FACE: f32 = 0.2;
const SWITCH_HEIGHT: f32 = 1.3;

/// The shock's slow goes through the stat-modifier seam as its own source.
const SHOCK_SOURCE: Source = Source("Trap Shock");

/// Where a Trap is in its cycle, with the seconds spent in that phase.
#[derive(Debug, Clone, Copy, PartialEq)]
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

/// The Sapper's hit from one entry into a running Trap: damage from its max Health, and the stun.
pub fn sapper_trap_hit(max_health: f32) -> (f32, f32) {
    (max_health * TRAP_SAPPER_HEALTH_FRACTION, TRAP_SAPPER_STUN_SECS)
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
}

impl Trap {
    /// The two Zones whose Door it spans.
    pub fn joins(&self) -> (ZoneId, ZoneId) {
        self.joins
    }

    /// Whether it's running, zapping everything inside.
    pub fn running(&self) -> bool {
        self.phase.running()
    }
}

/// One of a Trap's two switches, the Buyable that runs it.
#[derive(Component)]
struct TrapSwitch {
    trap: Entity,
    use_point: Vec3,
}

/// The lamp on a switch, lit while its Trap can be bought.
#[derive(Component)]
struct SwitchLight;

/// A Zombie that touched a running Trap, dying in `left` seconds.
#[derive(Component)]
pub struct Electrocuted {
    left: f32,
}

/// The Player, shocked by a running Trap through Juggernog: slowed for `left` seconds.
#[derive(Component)]
pub struct Shocked {
    left: f32,
}

/// A Zombie a Trap doesn't kill: the Sapper, which takes `sapper_trap_hit` once per entry instead.
#[derive(Component)]
pub struct ResistsTraps;

/// A `ResistsTraps` Zombie inside a running Trap, so it's struck once per entry.
#[derive(Component)]
struct InTrap;

/// A running Trap struck a `ResistsTraps` Zombie as it entered. The Sapper slice applies
/// `sapper_trap_hit`.
#[derive(Message, Debug, Clone, Copy)]
pub struct TrapStruck {
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
                    (use_switches, run_traps, prompt_switches, show_fields).chain(),
                    (shock_zombies, electrocute).chain(),
                    shock_player.before(take_hits),
                )
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn spawn_traps(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let field = meshes.add(Cuboid::from_size(TRAP_SIZE));
    let field_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.5, 0.8, 1.0, 0.25),
        emissive: LinearRgba::rgb(2.0, 4.0, 8.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let panel = meshes.add(Cuboid::new(0.4, 0.6, 0.06));
    let panel_mat = materials.add(Color::srgb(0.25, 0.27, 0.3));
    let lamp = meshes.add(Cuboid::new(0.12, 0.12, 0.04));
    let lamp_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.9, 0.3),
        emissive: LinearRgba::rgb(1.0, 6.0, 1.5),
        ..default()
    });

    for (joins, doorway, switch_x) in TRAPS {
        let trap = commands
            .spawn((
                InRun,
                Name::new("Trap"),
                Trap {
                    joins,
                    phase: TrapPhase::Idle,
                },
                Mesh3d(field.clone()),
                MeshMaterial3d(field_mat.clone()),
                Transform::from_xyz(doorway.x, TRAP_SIZE.y / 2.0, doorway.y),
                Visibility::Hidden,
            ))
            .id();
        // A switch on each face of the wall, facing out into its room.
        for facing in [Vec3::Z, Vec3::NEG_Z] {
            let at = Vec3::new(switch_x, SWITCH_HEIGHT, doorway.y) + facing * (WALL_FACE + 0.03);
            commands.spawn((
                InRun,
                Name::new("Trap Switch"),
                TrapSwitch {
                    trap,
                    use_point: at + facing * 0.1,
                },
                Mesh3d(panel.clone()),
                MeshMaterial3d(panel_mat.clone()),
                Transform::from_translation(at).looking_to(-facing, Vec3::Y),
                children![(
                    SwitchLight,
                    Mesh3d(lamp.clone()),
                    MeshMaterial3d(lamp_mat.clone()),
                    // The panel's +Z faces out into the room.
                    Transform::from_xyz(0.0, 0.18, 0.04),
                    Visibility::Hidden,
                )],
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
    for Purchased(entity) in purchases.read() {
        if let Ok(switch) = switches.get(*entity)
            && let Ok(mut trap) = traps.get_mut(switch.trap)
        {
            trap.phase.start();
        }
    }
}

/// The cooldown resolves through the modifier seam from the Player's entries. A Sabotaged Trap
/// goes dead at once, even mid-run.
fn run_traps(mut traps: Query<(&mut Trap, Has<Sabotaged>)>, player: Query<&Modifiers, With<Player>>, time: Res<Time>) {
    let cooldown = resolve(Stat::TrapCooldown, TRAP_COOLDOWN_SECS, player.iter().flat_map(|modifiers| &modifiers.0));
    for (mut trap, sabotaged) in &mut traps {
        if sabotaged {
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
    power: Res<Power>,
    player: Query<&Modifiers, With<Player>>,
) {
    let price = resolve_points(Stat::TrapPrice, TRAP_PRICE, player.iter().flat_map(|modifiers| &modifiers.0));
    for (entity, switch, children, buyable, refused) in &switches {
        let Ok((trap, sabotaged)) = traps.get(switch.trap) else { continue };
        let door_open = doors.iter().any(|door| door.open && door.joins == Some(trap.joins));
        let want = if sabotaged {
            TrapPrompt::NoPrompt
        } else {
            trap_prompt(door_open, power.is_on(), trap.phase)
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
                switch_entity.insert((buyable("You must turn on the Power first", None), Refused));
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

/// A Zombie that touches a running Trap is electrocuted and dies a moment later. A Zombie that
/// resists Traps is struck once each time it enters one instead.
fn shock_zombies(
    mut commands: Commands,
    zombies: Query<
        (Entity, &Transform, Has<ResistsTraps>, Has<InTrap>),
        (With<Zombie>, Without<Dying>, Without<Rising>, Without<Electrocuted>),
    >,
    traps: Query<(&Trap, &Transform)>,
    mut struck: MessageWriter<TrapStruck>,
) {
    let volumes = running_volumes(traps.iter());
    let half_height = ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0;
    for (entity, transform, resists, in_trap) in &zombies {
        let inside = volumes
            .iter()
            .any(|&(centre, half)| touches(centre, half, transform.translation, ZOMBIE_RADIUS, half_height));
        match (inside, resists) {
            (true, false) => {
                commands.entity(entity).insert(Electrocuted {
                    left: kill_delay(rand::random()),
                });
            }
            (true, true) if !in_trap => {
                commands.entity(entity).insert(InTrap);
                struck.write(TrapStruck { zombie: entity });
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
                zombie: entity,
                damage: f32::INFINITY,
                zone: HitZone::Torso,
                attack: Attack::Trap,
            });
            commands.entity(entity).remove::<Electrocuted>();
        }
    }
}

/// A running Trap Downs the Player without Juggernog, and shocks them through it, again each
/// time a shock ends with them still inside.
fn shock_player(
    mut commands: Commands,
    player: Single<
        (Entity, &Transform, &Perks, &mut Modifiers, Option<&mut Shocked>, Has<LastStand>),
        With<Player>,
    >,
    traps: Query<(&Trap, &Transform)>,
    mut hits: MessageWriter<PlayerHit>,
    time: Res<Time>,
) {
    let (entity, transform, perks, mut modifiers, shocked, last_stand) = player.into_inner();
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

    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    let inside = running_volumes(traps.iter())
        .iter()
        .any(|&(centre, half)| touches(centre, half, transform.translation, PLAYER_RADIUS, half_height));
    if !inside {
        return;
    }
    match player_touch(perks.has(Perk::Juggernog), last_stand, still_shocked) {
        PlayerTouch::Nothing => {}
        // Enough to empty any Health; the Down itself is the Last Stand module's.
        PlayerTouch::Down => {
            hits.write(PlayerHit { damage: f32::MAX });
        }
        PlayerTouch::Shock => {
            hits.write(PlayerHit {
                damage: TRAP_PLAYER_DAMAGE,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::zone_at;

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
        for ((a, b), doorway, switch_x) in TRAPS {
            let sides: Vec<_> = [WALL_FACE + 0.1, -WALL_FACE - 0.1]
                .into_iter()
                .map(|out| zone_at(Vec3::new(switch_x, SWITCH_HEIGHT, doorway.y + out)))
                .collect();
            assert!(sides.contains(&Some(a)) && sides.contains(&Some(b)), "{} - {}", a.name(), b.name());
            // Clear of the field, so throwing it doesn't touch it.
            assert!((switch_x - doorway.x).abs() - TRAP_SIZE.x / 2.0 > PLAYER_RADIUS);
        }
    }
}
