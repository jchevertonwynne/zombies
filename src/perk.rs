//! Perks: bought from their Perk Machines, drunk, then held until the Player's next Down. Each
//! Perk is its own source of stat-modifier entries on the Player.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::buy::{Buyable, Purchased, Refused};
use crate::layers::GameLayer;
use crate::modifier::{Effect, Modifier, Modifiers, Source, Stat};
use crate::player::Player;
use crate::power::Power;
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::weapon::Loadout;

/// Quick Revive joins these with the Last Stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perk {
    Juggernog,
    SpeedCola,
    DoubleTap,
}

impl Perk {
    pub const ALL: [Perk; 3] = [Perk::Juggernog, Perk::SpeedCola, Perk::DoubleTap];

    pub fn name(self) -> &'static str {
        match self {
            Perk::Juggernog => "Juggernog",
            Perk::SpeedCola => "Speed Cola",
            Perk::DoubleTap => "Double Tap",
        }
    }

    pub fn price(self) -> u64 {
        match self {
            Perk::Juggernog => PERK_PRICE_JUGGERNOG,
            Perk::SpeedCola => PERK_PRICE_SPEED_COLA,
            Perk::DoubleTap => PERK_PRICE_DOUBLE_TAP,
        }
    }

    /// Whether its machine waits for the Power.
    pub fn needs_power(self) -> bool {
        match self {
            Perk::Juggernog | Perk::SpeedCola | Perk::DoubleTap => true,
        }
    }

    /// The stat-modifier entries the Perk puts on the Player while held.
    pub fn modifiers(self) -> &'static [Modifier] {
        const fn one(perk: &'static str, stat: Stat, effect: Effect) -> Modifier {
            Modifier {
                stat,
                source: Source(perk),
                effect,
            }
        }
        const JUGGERNOG: &[Modifier] = &[one(
            "Juggernog",
            Stat::MaxHealth,
            Effect::Flat(JUGGERNOG_MAX_HEALTH - PLAYER_MAX_HEALTH),
        )];
        const SPEED_COLA: &[Modifier] = &[one(
            "Speed Cola",
            Stat::ReloadTime,
            Effect::Mult(SPEED_COLA_RELOAD_MULT),
        )];
        const DOUBLE_TAP: &[Modifier] = &[one(
            "Double Tap",
            Stat::FireInterval,
            Effect::Mult(DOUBLE_TAP_FIRE_INTERVAL_MULT),
        )];
        match self {
            Perk::Juggernog => JUGGERNOG,
            Perk::SpeedCola => SPEED_COLA,
            Perk::DoubleTap => DOUBLE_TAP,
        }
    }

    /// Where its machine stands, against a wall of its Zone, and the way it faces.
    fn machine_spot(self) -> (Vec3, Vec3) {
        match self {
            // West Hall's outer wall.
            Perk::Juggernog => (Vec3::new(-25.2, 0.0, 10.0), Vec3::X),
            // The Dining Room's south wall.
            Perk::SpeedCola => (Vec3::new(-13.0, 0.0, -25.2), Vec3::Z),
            // The Power Room's south wall.
            Perk::DoubleTap => (Vec3::new(13.0, 0.0, -25.2), Vec3::Z),
        }
    }

    fn colour(self) -> Color {
        match self {
            Perk::Juggernog => Color::srgb(0.75, 0.1, 0.1),
            Perk::SpeedCola => Color::srgb(0.1, 0.6, 0.2),
            Perk::DoubleTap => Color::srgb(0.85, 0.55, 0.1),
        }
    }
}

/// What a Perk Machine's Prompt shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachinePrompt {
    /// Nothing: the Perk is held already, or one is being drunk.
    NoPrompt,
    /// "You must turn on the Power first", and the Use key does nothing.
    NeedsPower,
    /// The buy, with its price.
    Buy,
}

/// A Perk Machine's Prompt, given the Power and what the Player holds. There's no Perk limit.
pub fn machine_prompt(perk: Perk, power_on: bool, owned: bool, drinking: bool) -> MachinePrompt {
    if owned || drinking {
        MachinePrompt::NoPrompt
    } else if perk.needs_power() && !power_on {
        MachinePrompt::NeedsPower
    } else {
        MachinePrompt::Buy
    }
}

/// The Perks the Player holds. A Down takes them all.
#[derive(Component, Default)]
pub struct Perks(pub Vec<Perk>);

impl Perks {
    pub fn has(&self, perk: Perk) -> bool {
        self.0.contains(&perk)
    }
}

/// The Player is drinking a Perk: no Weapon, no aiming, no sprinting, no Knife. The Perk applies
/// when the drink ends; a Down before then loses it and the Points.
#[derive(Component)]
pub struct Drinking {
    pub perk: Perk,
    elapsed: f32,
}

impl Drinking {
    pub fn new(perk: Perk) -> Self {
        Self { perk, elapsed: 0.0 }
    }

    /// Advance the drink. True once it's finished.
    pub fn advance(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        self.elapsed >= PERK_DRINK_SECS
    }
}

/// Run condition: the Player's hands are free for their Weapon and Knife.
pub fn hands_free(drinking: Query<(), (With<Player>, With<Drinking>)>) -> bool {
    drinking.is_empty()
}

/// The machine that sells one Perk.
#[derive(Component)]
pub struct PerkMachine {
    pub perk: Perk,
    use_point: Vec3,
}

/// The sign on a machine, lit once the Power is on.
#[derive(Component)]
struct MachineLight;

pub struct PerkPlugin;

impl Plugin for PerkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), spawn_machines)
            .add_systems(Update, give_perks)
            .add_systems(
                Update,
                (buy_perks, drink, prompt_machines, light_machines)
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn spawn_machines(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let body = Cuboid::new(1.2, 2.2, 0.9);
    let sign = Cuboid::new(1.0, 0.4, 0.05);
    for perk in Perk::ALL {
        let (at, facing) = perk.machine_spot();
        let centre = at + Vec3::Y * body.half_size.y;
        let lit = materials.add(StandardMaterial {
            base_color: perk.colour(),
            emissive: LinearRgba::from(perk.colour()) * 6.0,
            ..default()
        });
        commands
            .spawn((
                InRun,
                Name::new(perk.name()),
                PerkMachine {
                    perk,
                    use_point: centre + facing * (body.half_size.z + 0.1),
                },
                Mesh3d(meshes.add(body)),
                MeshMaterial3d(materials.add(perk.colour().darker(0.25))),
                Transform::from_translation(centre).looking_to(-facing, Vec3::Y),
                RigidBody::Static,
                Collider::from(body),
                CollisionLayers::new(GameLayer::World, LayerMask::ALL),
            ))
            // The machine's +Z faces out into the room.
            .with_child((
                MachineLight,
                Mesh3d(meshes.add(sign)),
                MeshMaterial3d(lit),
                Transform::from_xyz(0.0, 0.75, body.half_size.z + 0.03),
                Visibility::Hidden,
            ));
    }
}

fn give_perks(mut commands: Commands, players: Query<Entity, Added<Player>>) {
    for player in &players {
        commands.entity(player).insert(Perks::default());
    }
}

/// A bought Perk starts the drink, putting the Weapon away mid-Reload if need be.
fn buy_perks(
    mut commands: Commands,
    mut purchases: MessageReader<Purchased>,
    machines: Query<&PerkMachine>,
    player: Single<(Entity, &mut Loadout), With<Player>>,
) {
    let (entity, mut loadout) = player.into_inner();
    for Purchased(machine) in purchases.read() {
        if let Ok(machine) = machines.get(*machine) {
            loadout.reload = None;
            commands.entity(entity).insert(Drinking::new(machine.perk));
        }
    }
}

/// The Perk applies when the drink ends.
fn drink(
    mut commands: Commands,
    player: Single<(Entity, &mut Drinking, &mut Perks, &mut Modifiers), With<Player>>,
    time: Res<Time>,
) {
    let (entity, mut drinking, mut perks, mut modifiers) = player.into_inner();
    if !drinking.advance(time.delta_secs()) {
        return;
    }
    let perk = drinking.perk;
    if !perks.has(perk) {
        perks.0.push(perk);
        modifiers.0.extend_from_slice(perk.modifiers());
    }
    commands.entity(entity).remove::<Drinking>();
}

/// Machines own their Buyables: a buy, the Power refusal, or no Prompt at all.
fn prompt_machines(
    mut commands: Commands,
    machines: Query<(Entity, &PerkMachine, Has<Buyable>, Has<Refused>)>,
    player: Single<(&Perks, Has<Drinking>), With<Player>>,
    power: Res<Power>,
) {
    let (perks, drinking) = *player;
    for (entity, machine, has_buyable, refused) in &machines {
        let perk = machine.perk;
        let want = machine_prompt(perk, power.is_on(), perks.has(perk), drinking);
        let current = match (has_buyable, refused) {
            (false, _) => MachinePrompt::NoPrompt,
            (true, true) => MachinePrompt::NeedsPower,
            (true, false) => MachinePrompt::Buy,
        };
        if want == current {
            continue;
        }
        let mut machine_entity = commands.entity(entity);
        let buyable = |prompt: String, price| Buyable {
            use_point: machine.use_point,
            prompt,
            price,
            range: USE_RANGE,
        };
        match want {
            MachinePrompt::NoPrompt => {
                machine_entity.remove::<(Buyable, Refused)>();
            }
            MachinePrompt::NeedsPower => {
                machine_entity.insert((
                    buyable("You must turn on the Power first".into(), None),
                    Refused,
                ));
            }
            MachinePrompt::Buy => {
                machine_entity.remove::<Refused>().insert(buyable(
                    format!("Press F to buy {}", perk.name()),
                    Some(perk.price()),
                ));
            }
        }
    }
}

fn light_machines(power: Res<Power>, mut lights: Query<&mut Visibility, With<MachineLight>>) {
    if !power.is_changed() || !power.is_on() {
        return;
    }
    for mut light in &mut lights {
        *light = Visibility::Inherited;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::resolve;

    fn held(perks: &[Perk]) -> Vec<Modifier> {
        perks
            .iter()
            .flat_map(|perk| perk.modifiers())
            .copied()
            .collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn juggernog_raises_max_health_to_160() {
        let entries = held(&[Perk::Juggernog]);
        assert!(close(
            resolve(Stat::MaxHealth, PLAYER_MAX_HEALTH, &entries),
            160.0
        ));
    }

    #[test]
    fn juggernog_survives_a_swipe_that_would_down_a_player_without_it() {
        let without = resolve(Stat::MaxHealth, PLAYER_MAX_HEALTH, &[]);
        let with = resolve(
            Stat::MaxHealth,
            PLAYER_MAX_HEALTH,
            &held(&[Perk::Juggernog]),
        );
        assert!(without - 2.0 * ZOMBIE_SWIPE_DAMAGE <= 0.0);
        assert!(with - 2.0 * ZOMBIE_SWIPE_DAMAGE > 0.0);
    }

    #[test]
    fn speed_cola_halves_reload_time() {
        let entries = held(&[Perk::SpeedCola]);
        assert!(close(resolve(Stat::ReloadTime, 1.8, &entries), 0.9));
    }

    #[test]
    fn double_tap_cuts_the_fire_interval_by_a_quarter() {
        let entries = held(&[Perk::DoubleTap]);
        assert!(close(resolve(Stat::FireInterval, 0.2, &entries), 0.15));
    }

    #[test]
    fn each_perk_only_touches_its_own_stat() {
        let entries = held(&Perk::ALL);
        assert!(close(
            resolve(Stat::MaxHealth, PLAYER_MAX_HEALTH, &entries),
            160.0
        ));
        assert!(close(resolve(Stat::ReloadTime, 2.0, &entries), 1.0));
        assert!(close(resolve(Stat::FireInterval, 1.0, &entries), 0.75));
        assert!(close(resolve(Stat::Damage, 20.0, &entries), 20.0));
        assert!(close(resolve(Stat::MoveSpeed, 5.0, &entries), 5.0));
    }

    #[test]
    fn perks_are_their_own_sources_so_they_multiply_with_others() {
        // Rapid Fire ×0.8 with Double Tap ×0.75 gives ×0.6.
        let mut entries = held(&[Perk::DoubleTap]);
        entries.push(Modifier {
            stat: Stat::FireInterval,
            source: Source("Rapid Fire"),
            effect: Effect::Mult(0.8),
        });
        assert!(close(resolve(Stat::FireInterval, 1.0, &entries), 0.6));
    }

    #[test]
    fn perks_cost_their_bo1_prices() {
        assert_eq!(Perk::Juggernog.price(), 2500);
        assert_eq!(Perk::SpeedCola.price(), 3000);
        assert_eq!(Perk::DoubleTap.price(), 2000);
    }

    #[test]
    fn before_power_the_machines_refuse() {
        for perk in Perk::ALL {
            assert_eq!(
                machine_prompt(perk, false, false, false),
                MachinePrompt::NeedsPower
            );
        }
    }

    #[test]
    fn with_power_an_unheld_perk_is_buyable() {
        for perk in Perk::ALL {
            assert_eq!(machine_prompt(perk, true, false, false), MachinePrompt::Buy);
        }
    }

    #[test]
    fn a_held_perk_shows_no_prompt() {
        for power_on in [false, true] {
            assert_eq!(
                machine_prompt(Perk::Juggernog, power_on, true, false),
                MachinePrompt::NoPrompt
            );
        }
    }

    #[test]
    fn no_machine_sells_while_drinking() {
        assert_eq!(
            machine_prompt(Perk::DoubleTap, true, false, true),
            MachinePrompt::NoPrompt
        );
    }

    #[test]
    fn the_drink_takes_its_full_time() {
        let mut drinking = Drinking::new(Perk::Juggernog);
        assert!(!drinking.advance(PERK_DRINK_SECS - 0.1));
        assert!(drinking.advance(0.1));
    }
}
