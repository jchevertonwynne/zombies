//! Perks: bought from their Perk Machines, drunk, then held until the Player's next Down. Each
//! Perk is its own source of stat-modifier entries on the Player.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::maps::map::{MapId, QUARRY_X, STOREY};

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::economy::buy::{Buyable, Purchased, Refused};
use crate::layers::GameLayer;
use crate::modifier::{Effect, Modifier, Modifiers, Source, Stat, resolve_count, resolve_points};
use crate::players::player::{LocalPlayer, Player};
use crate::maps::power::Power;
use crate::enemies::sapper::Sabotaged;
use crate::state::{GameState, InRun, RunType, world_runs};
use crate::tuning::*;
use crate::weapons::weapon::Loadout;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Perk {
    Juggernog,
    SpeedCola,
    DoubleTap,
    /// Solo, one Self-Revive: the next Down starts a Last Stand instead of ending the Run. In
    /// Co-op, it only makes the Player's Revives of others faster.
    QuickRevive,
}

impl Perk {
    pub const ALL: [Perk; 4] = [Perk::Juggernog, Perk::SpeedCola, Perk::DoubleTap, Perk::QuickRevive];

    pub fn name(self) -> &'static str {
        match self {
            Perk::Juggernog => "Juggernog",
            Perk::SpeedCola => "Speed Cola",
            Perk::DoubleTap => "Double Tap",
            Perk::QuickRevive => "Quick Revive",
        }
    }

    pub fn price(self, run: RunType) -> u64 {
        match self {
            Perk::QuickRevive if run.is_coop() => PERK_PRICE_QUICK_REVIVE_COOP,
            Perk::Juggernog => PERK_PRICE_JUGGERNOG,
            Perk::SpeedCola => PERK_PRICE_SPEED_COLA,
            Perk::DoubleTap => PERK_PRICE_DOUBLE_TAP,
            Perk::QuickRevive => PERK_PRICE_QUICK_REVIVE,
        }
    }

    /// Whether its machine waits for the Power. Quick Revive does in Co-op.
    pub fn needs_power(self, run: RunType) -> bool {
        match self {
            Perk::Juggernog | Perk::SpeedCola | Perk::DoubleTap => true,
            Perk::QuickRevive => run.is_coop(),
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
            // Its Self-Revive is the Last Stand's business, not a stat.
            Perk::QuickRevive => &[],
        }
    }

    /// Whether its machine leaves after `buys` buys. Only solo Quick Revive runs out, after
    /// `solo_buys` (see `quick_revive_buys`).
    pub fn machine_leaves(self, buys: u32, solo_buys: u32) -> bool {
        self == Perk::QuickRevive && buys >= solo_buys
    }

    /// Where its machine stands on `map`, against a wall of its Zone, and the way it faces.
    pub(crate) fn machine_spot(self, map: MapId) -> (Vec3, Vec3) {
        const Q: f32 = QUARRY_X;
        match (map, self) {
            // West Hall's outer wall.
            (MapId::Greybox, Perk::Juggernog) => (Vec3::new(-25.2, 0.0, 10.0), Vec3::X),
            // The Dining Room's south wall, west of the Passage Door.
            (MapId::Greybox, Perk::SpeedCola) => (Vec3::new(-23.0, 0.0, -25.2), Vec3::Z),
            // The Power Room's south wall, east of the Tunnel Door.
            (MapId::Greybox, Perk::DoubleTap) => (Vec3::new(24.0, 0.0, -25.2), Vec3::Z),
            // The Start Room's north wall, away from its Spawn Points.
            (MapId::Greybox, Perk::QuickRevive) => (Vec3::new(11.5, 0.0, 25.2), Vec3::NEG_Z),
            // The Crusher Shed's back wall, against the terrace above.
            (MapId::Quarry, Perk::Juggernog) => (Vec3::new(Q - 15.0, STOREY, 15.2), Vec3::NEG_Z),
            // The Winding House's south wall, beside the Door up from the Rail Yard.
            (MapId::Quarry, Perk::SpeedCola) => (Vec3::new(Q + 8.0, 2.0 * STOREY, 16.8), Vec3::Z),
            // The Offices' north wall.
            (MapId::Quarry, Perk::DoubleTap) => (Vec3::new(Q - 5.0, 2.0 * STOREY, 27.2), Vec3::NEG_Z),
            // The Pump House's east wall.
            (MapId::Quarry, Perk::QuickRevive) => (Vec3::new(Q + 7.2, 0.0, -25.0), Vec3::NEG_X),
        }
    }

    pub fn colour(self) -> Color {
        match self {
            Perk::Juggernog => Color::srgb(0.75, 0.1, 0.1),
            Perk::SpeedCola => Color::srgb(0.1, 0.6, 0.2),
            Perk::DoubleTap => Color::srgb(0.85, 0.55, 0.1),
            Perk::QuickRevive => Color::srgb(0.2, 0.55, 0.9),
        }
    }
}

/// Solo Quick Revive buys before its machine leaves: `QUICK_REVIVE_SOLO_BUYS`, or as many as the
/// Player's stat entries say.
pub fn quick_revive_buys(player: &[Modifier]) -> u32 {
    resolve_count(Stat::QuickReviveBuys, QUICK_REVIVE_SOLO_BUYS, player)
}

/// Perks a Down leaves the Player, by their stat entries: none in Classic.
pub fn kept_on_down(player: &[Modifier]) -> usize {
    resolve_count(Stat::PerksKeptOnDown, 0, player) as usize
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
/// Holding Quick Revive is holding an unused life (solo) or a faster Revive (Co-op), so it can't
/// be bought again until a Down.
pub fn machine_prompt(perk: Perk, power_on: bool, owned: bool, drinking: bool, run: RunType) -> MachinePrompt {
    if owned || drinking {
        MachinePrompt::NoPrompt
    } else if perk.needs_power(run) && !power_on {
        MachinePrompt::NeedsPower
    } else {
        MachinePrompt::Buy
    }
}

/// The Perks the Player holds, in the order they were bought. A Down takes them all.
#[derive(Component, Default)]
pub struct Perks(pub Vec<Perk>);

impl Perks {
    pub fn has(&self, perk: Perk) -> bool {
        self.0.contains(&perk)
    }

    /// Shows the Perks the Host says are held: those it names are held with their entries, and
    /// any other goes, with its entries.
    pub fn show(&mut self, held: &[Perk], modifiers: &mut Modifiers) {
        for &perk in Perk::ALL.iter() {
            let has = self.has(perk);
            if held.contains(&perk) && !has {
                modifiers.0.extend_from_slice(perk.modifiers());
            } else if !held.contains(&perk) && has {
                modifiers.0.retain(|entry| entry.source != Source(perk.name()));
            }
        }
        self.0 = held.to_vec();
    }

    /// A Down takes every Perk but the `keep` earliest bought (see `kept_on_down`), and with them
    /// every entry they put on the Player. Quick Revive always goes: the Down used its life.
    pub fn lose_on_down(&mut self, keep: usize, modifiers: &mut Modifiers) {
        let mut kept = 0;
        let mut lost = Vec::new();
        self.0.retain(|&perk| {
            let stays = perk != Perk::QuickRevive && kept < keep;
            if stays {
                kept += 1;
            } else {
                lost.push(perk);
            }
            stays
        });
        for perk in lost {
            modifiers.0.retain(|entry| entry.source != Source(perk.name()));
        }
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

    /// How far through the drink it is, 0 to 1.
    pub fn progress(&self) -> f32 {
        (self.elapsed / PERK_DRINK_SECS).min(1.0)
    }
}

/// Run condition: the Player's hands are free for their Weapon and Knife.
pub fn hands_free(drinking: Query<(), (With<Player>, With<Drinking>)>) -> bool {
    drinking.is_empty()
}

/// A machine that sells its Perk to each Player once: a Player who holds it, or is drinking it,
/// is refused. The purchase flow checks it before charging.
#[derive(Component)]
pub struct OncePerPlayer(pub Perk);

impl OncePerPlayer {
    /// Whether the Player has this Perk already, or is drinking it.
    pub fn refuses(&self, perks: Option<&Perks>, drinking: Option<&Drinking>) -> bool {
        perks.is_some_and(|perks| perks.has(self.0)) || drinking.is_some_and(|drinking| drinking.perk == self.0)
    }
}

/// The machine that sells one Perk. A Sabotaged machine shows no Prompt until it's restored.
#[derive(Component)]
pub struct PerkMachine {
    pub perk: Perk,
    use_point: Vec3,
    /// How many times it has sold its Perk this Run.
    buys: u32,
}

impl PerkMachine {
    /// Where it's bought from, in front of it.
    pub fn use_point(&self) -> Vec3 {
        self.use_point
    }
}

/// A machine that has sold its last Perk: it waits, floats away, and is gone.
#[derive(Component)]
struct Leaving {
    elapsed: f32,
    from: Vec3,
}

/// How far a leaving machine has risen after `elapsed` seconds, or `None` once it's gone.
pub fn leaving_rise(elapsed: f32) -> Option<f32> {
    let floating = elapsed - QUICK_REVIVE_LEAVE_DELAY_SECS;
    if floating >= QUICK_REVIVE_FLOAT_SECS {
        None
    } else {
        Some(floating.max(0.0) / QUICK_REVIVE_FLOAT_SECS * QUICK_REVIVE_FLOAT_HEIGHT)
    }
}

/// The sign on a machine, lit once the Power is on (or from the start, if its Perk needs none).
#[derive(Component)]
struct MachineLight(Perk);

pub struct PerkPlugin;

impl Plugin for PerkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), spawn_machines)
            .add_systems(Update, give_perks)
            .add_systems(
                Update,
                (
                    (buy_perks, drink).chain().in_set(HostSimulation),
                    prompt_machines,
                    light_machines,
                    float_away,
                )
                    .chain()
                    .run_if(world_runs),
            );
    }
}

fn spawn_machines(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    map: Res<MapId>,
) {
    let body = Cuboid::new(1.2, 2.2, 0.9);
    let sign = Cuboid::new(1.0, 0.4, 0.05);
    for perk in Perk::ALL {
        let (at, facing) = perk.machine_spot(*map);
        let centre = at + Vec3::Y * body.half_size.y;
        let lit = materials.add(StandardMaterial {
            base_color: perk.colour(),
            emissive: LinearRgba::from(perk.colour()) * 6.0,
            ..default()
        });
        let machine = commands
            .spawn((
                InRun,
                Name::new(perk.name()),
                PerkMachine {
                    perk,
                    use_point: centre + facing * (body.half_size.z + 0.1),
                    buys: 0,
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
                MachineLight(perk),
                Mesh3d(meshes.add(sign)),
                MeshMaterial3d(lit),
                Transform::from_xyz(0.0, 0.75, body.half_size.z + 0.03),
                // Quick Revive is lit from the start, solo; `light_machines` fixes it for Co-op.
                if perk == Perk::QuickRevive { Visibility::Inherited } else { Visibility::Hidden },
            ))
            .id();
        // Quick Revive's rebuys are its own rule (see `quick_revive_buys`).
        if perk != Perk::QuickRevive {
            commands.entity(machine).insert(OncePerPlayer(perk));
        }
    }
}

fn give_perks(mut commands: Commands, players: Query<Entity, Added<Player>>) {
    for player in &players {
        commands.entity(player).insert(Perks::default());
    }
}

/// A bought Perk starts the drink, putting the Weapon away mid-Reload if need be. A machine that
/// has sold its last Perk starts to leave.
fn buy_perks(
    mut commands: Commands,
    mut purchases: MessageReader<Purchased>,
    mut machines: Query<(&mut PerkMachine, &Transform)>,
    mut players: Query<(&mut Loadout, &Modifiers), With<Player>>,
    run: Res<RunType>,
) {
    for &Purchased { player: entity, buyable: bought } in purchases.read() {
        let Ok((mut loadout, modifiers)) = players.get_mut(entity) else { continue };
        let solo_buys = quick_revive_buys(&modifiers.0);
        if let Ok((mut machine, transform)) = machines.get_mut(bought) {
            loadout.reload = None;
            commands.entity(entity).insert(Drinking::new(machine.perk));
            machine.buys = machine.buys.saturating_add(1);
            // In Co-op the machine never flies away.
            if !run.is_coop() && machine.perk.machine_leaves(machine.buys, solo_buys) {
                commands.entity(bought).remove::<(Buyable, Refused)>().insert(Leaving {
                    elapsed: 0.0,
                    from: transform.translation,
                });
            }
        }
    }
}

fn float_away(
    mut commands: Commands,
    mut machines: Query<(Entity, &mut Leaving, &mut Transform)>,
    mut cues: MessageWriter<PlayCue>,
    time: Res<Time>,
) {
    for (entity, mut leaving, mut transform) in &mut machines {
        let waited = leaving.elapsed;
        leaving.elapsed += time.delta_secs();
        if waited < QUICK_REVIVE_LEAVE_DELAY_SECS && leaving.elapsed >= QUICK_REVIVE_LEAVE_DELAY_SECS {
            cues.write(PlayCue {
                cue: Cue::PerkMachineLeave,
                at: Some(leaving.from),
            });
        }
        match leaving_rise(leaving.elapsed) {
            Some(rise) => transform.translation = leaving.from + Vec3::Y * rise,
            None => commands.entity(entity).despawn(),
        }
    }
}

/// The Perk applies when the drink ends.
pub fn drink(
    mut commands: Commands,
    mut players: Query<(Entity, &mut Drinking, &mut Perks, &mut Modifiers), With<Player>>,
    time: Res<Time>,
) {
    for (entity, mut drinking, mut perks, mut modifiers) in &mut players {
        if !drinking.advance(time.delta_secs()) {
            continue;
        }
        let perk = drinking.perk;
        if !perks.has(perk) {
            perks.0.push(perk);
            modifiers.0.extend_from_slice(perk.modifiers());
        }
        commands.entity(entity).remove::<Drinking>();
    }
}

/// Machines own their Buyables: a buy, the Power refusal, or no Prompt at all (always, while
/// Sabotaged).
fn prompt_machines(
    mut commands: Commands,
    machines: Query<(Entity, &PerkMachine, Option<&Buyable>, Has<Refused>, Has<Sabotaged>), Without<Leaving>>,
    players: Query<(&Perks, &Modifiers, Has<Drinking>, Has<LocalPlayer>), With<Player>>,
    power: Res<Power>,
    run: Res<RunType>,
) {
    // The price is the one the `LocalPlayer` sees.
    let Some(modifiers) = players.iter().find(|player| player.3).map(|player| player.1) else { return };
    for (entity, machine, buyable, refused, sabotaged) in &machines {
        let perk = machine.perk;
        let price = resolve_points(Stat::PerkPrice, perk.price(*run), &modifiers.0);
        let want = if sabotaged {
            MachinePrompt::NoPrompt
        } else {
            // Offered while any Player can still buy it; each is refused for their own Perk.
            let prompts = players.iter().map(|(perks, _, drinking, _)| {
                machine_prompt(perk, power.is_on(), perks.has(perk), drinking, *run)
            });
            prompts.max_by_key(|&prompt| match prompt {
                MachinePrompt::Buy => 2,
                MachinePrompt::NeedsPower => 1,
                MachinePrompt::NoPrompt => 0,
            })
            .unwrap_or(MachinePrompt::NoPrompt)
        };
        let current = match (buyable, refused) {
            (None, _) => MachinePrompt::NoPrompt,
            (Some(_), true) => MachinePrompt::NeedsPower,
            (Some(_), false) => MachinePrompt::Buy,
        };
        // A changed price (from the modifier seam) re-offers the buy.
        let repriced = want == MachinePrompt::Buy && buyable.is_some_and(|buyable| buyable.price != Some(price));
        if want == current && !repriced {
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
                    Some(price),
                ));
            }
        }
    }
}

fn light_machines(power: Res<Power>, run: Res<RunType>, mut lights: Query<(&MachineLight, &mut Visibility)>) {
    for (&MachineLight(perk), mut light) in &mut lights {
        let lit = power.is_on() || !perk.needs_power(*run);
        let want = if lit { Visibility::Inherited } else { Visibility::Hidden };
        if *light != want {
            *light = want;
        }
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
        assert_eq!(Perk::Juggernog.price(RunType::Solo), 2500);
        assert_eq!(Perk::SpeedCola.price(RunType::Solo), 3000);
        assert_eq!(Perk::DoubleTap.price(RunType::Solo), 2000);
        assert_eq!(Perk::QuickRevive.price(RunType::Solo), 500);
    }

    #[test]
    fn before_power_the_machines_refuse() {
        for perk in [Perk::Juggernog, Perk::SpeedCola, Perk::DoubleTap] {
            assert_eq!(
                machine_prompt(perk, false, false, false, RunType::Solo),
                MachinePrompt::NeedsPower
            );
        }
    }

    #[test]
    fn with_power_an_unheld_perk_is_buyable() {
        for perk in Perk::ALL {
            assert_eq!(machine_prompt(perk, true, false, false, RunType::Solo), MachinePrompt::Buy);
        }
    }

    #[test]
    fn a_held_perk_shows_no_prompt() {
        for power_on in [false, true] {
            assert_eq!(
                machine_prompt(Perk::Juggernog, power_on, true, false, RunType::Solo),
                MachinePrompt::NoPrompt
            );
        }
    }

    #[test]
    fn no_machine_sells_while_drinking() {
        assert_eq!(
            machine_prompt(Perk::DoubleTap, true, false, true, RunType::Solo),
            MachinePrompt::NoPrompt
        );
    }

    #[test]
    fn the_drink_takes_its_full_time() {
        let mut drinking = Drinking::new(Perk::Juggernog);
        assert!(!drinking.advance(PERK_DRINK_SECS - 0.1));
        assert!(drinking.advance(0.1));
    }

    #[test]
    fn quick_revive_sells_without_power() {
        assert_eq!(
            machine_prompt(Perk::QuickRevive, false, false, false, RunType::Solo),
            MachinePrompt::Buy
        );
    }

    #[test]
    fn quick_revive_cannot_be_bought_while_its_life_is_unused() {
        for power_on in [false, true] {
            assert_eq!(
                machine_prompt(Perk::QuickRevive, power_on, true, false, RunType::Solo),
                MachinePrompt::NoPrompt
            );
        }
    }

    #[test]
    fn co_op_quick_revive_costs_1500_and_waits_for_the_power() {
        assert_eq!(Perk::QuickRevive.price(RunType::Coop), 1500);
        assert_eq!(
            machine_prompt(Perk::QuickRevive, false, false, false, RunType::Coop),
            MachinePrompt::NeedsPower
        );
        assert_eq!(machine_prompt(Perk::QuickRevive, true, false, false, RunType::Coop), MachinePrompt::Buy);
        // Held, it can't be bought again, until a Down takes it.
        assert_eq!(machine_prompt(Perk::QuickRevive, true, true, false, RunType::Coop), MachinePrompt::NoPrompt);
        // The other Perks are the same in both.
        for perk in [Perk::Juggernog, Perk::SpeedCola, Perk::DoubleTap] {
            assert_eq!(perk.price(RunType::Coop), perk.price(RunType::Solo));
            assert!(perk.needs_power(RunType::Coop));
        }
    }

    #[test]
    fn quick_revive_puts_no_entries_on_the_player() {
        assert!(Perk::QuickRevive.modifiers().is_empty());
    }

    #[test]
    fn only_quick_revive_leaves_and_only_after_the_third_buy() {
        let solo = quick_revive_buys(&[]);
        assert_eq!(solo, 3);
        assert!(!Perk::QuickRevive.machine_leaves(1, solo));
        assert!(!Perk::QuickRevive.machine_leaves(2, solo));
        assert!(Perk::QuickRevive.machine_leaves(3, solo));
        for perk in [Perk::Juggernog, Perk::SpeedCola, Perk::DoubleTap] {
            assert!(!perk.machine_leaves(100, solo));
        }
    }

    #[test]
    fn an_extra_buy_entry_keeps_quick_revive_one_buy_longer() {
        let one = [Modifier {
            stat: Stat::QuickReviveBuys,
            source: Source("test"),
            effect: Effect::Flat(1.0),
        }];
        let solo = quick_revive_buys(&one);
        assert_eq!(solo, 4);
        assert!(!Perk::QuickRevive.machine_leaves(3, solo));
        assert!(Perk::QuickRevive.machine_leaves(4, solo));
    }

    #[test]
    fn a_down_can_leave_the_earliest_perks_bought_but_never_quick_revive() {
        let keep_one = [Modifier {
            stat: Stat::PerksKeptOnDown,
            source: Source("test"),
            effect: Effect::Flat(1.0),
        }];
        assert_eq!(kept_on_down(&[]), 0);
        assert_eq!(kept_on_down(&keep_one), 1);

        let bought = [Perk::QuickRevive, Perk::SpeedCola, Perk::Juggernog];
        let mut perks = Perks(bought.to_vec());
        let mut modifiers = Modifiers(held(&bought));
        perks.lose_on_down(kept_on_down(&keep_one), &mut modifiers);
        assert_eq!(perks.0, [Perk::SpeedCola]);
        assert_eq!(modifiers.0, Perk::SpeedCola.modifiers());

        let mut perks = Perks(bought.to_vec());
        let mut modifiers = Modifiers(held(&bought));
        perks.lose_on_down(5, &mut modifiers);
        assert_eq!(perks.0, [Perk::SpeedCola, Perk::Juggernog]);
        assert_eq!(modifiers.0.len(), 2);
    }

    #[test]
    fn a_leaving_machine_waits_2_s_then_floats_for_3_s_and_is_gone() {
        assert_eq!(leaving_rise(0.0), Some(0.0));
        assert_eq!(leaving_rise(1.9), Some(0.0));
        assert!(close(leaving_rise(3.5).unwrap(), QUICK_REVIVE_FLOAT_HEIGHT / 2.0));
        assert!(leaving_rise(4.99).unwrap() < QUICK_REVIVE_FLOAT_HEIGHT);
        assert_eq!(leaving_rise(5.0), None);
    }

    #[test]
    fn losing_every_perk_takes_only_their_entries() {
        let mut perks = Perks(Perk::ALL.to_vec());
        let boon = Modifier {
            stat: Stat::Damage,
            source: Source("Rapid Fire"),
            effect: Effect::Mult(1.1),
        };
        let mut entries = held(&Perk::ALL);
        entries.push(boon);
        let mut modifiers = Modifiers(entries);
        perks.lose_on_down(0, &mut modifiers);
        assert!(perks.0.is_empty());
        assert_eq!(modifiers.0, [boon]);
        assert!(close(
            resolve(Stat::MaxHealth, PLAYER_MAX_HEALTH, &modifiers.0),
            PLAYER_MAX_HEALTH
        ));
    }
}

/// Perk Machines with two Players: each Perk once per Player, from the one machine.
#[cfg(test)]
mod two_player_tests {
    use super::*;
    use crate::economy::buy::UseRequest;
    use crate::players::player::two_players::{balance, two_players_with};

    #[test]
    fn each_player_buys_each_perk_once() {
        let (mut sim, local, teammate) = two_players_with(|app| {
            app.insert_resource(RunType::Coop).add_systems(Update, (buy_perks, drink).chain());
        });
        for player in [local, teammate] {
            sim.world_mut().entity_mut(player).insert(Perks::default());
            sim.world_mut().get_mut::<crate::economy::points::Points>(player).unwrap().balance = 10_000;
        }
        let perk = Perk::Juggernog;
        let machine = sim
            .world_mut()
            .spawn((
                PerkMachine { perk, use_point: Vec3::ZERO, buys: 0 },
                OncePerPlayer(perk),
                Buyable { use_point: Vec3::ZERO, prompt: String::new(), price: Some(perk.price(RunType::Coop)), range: USE_RANGE },
                Transform::default(),
            ))
            .id();
        let (local_before, teammate_before) = (balance(&mut sim, local), balance(&mut sim, teammate));
        sim.send(UseRequest { player: local, buyable: machine, hold: false });
        sim.run(sim.frames(PERK_DRINK_SECS + 0.5));
        assert!(sim.world_mut().get::<Perks>(local).unwrap().has(perk));
        assert!(!sim.world_mut().get::<Perks>(teammate).unwrap().has(perk), "the teammate got the local Player's Perk");
        assert_eq!(balance(&mut sim, local), local_before - perk.price(RunType::Coop));

        // A second buy by the same Player is refused and charges nothing.
        sim.send(UseRequest { player: local, buyable: machine, hold: false });
        sim.run(2);
        assert_eq!(balance(&mut sim, local), local_before - perk.price(RunType::Coop));
        assert_eq!(sim.world_mut().get::<Perks>(local).unwrap().0, [perk]);

        // The other Player still can, from the same machine.
        sim.send(UseRequest { player: teammate, buyable: machine, hold: false });
        sim.run(sim.frames(PERK_DRINK_SECS + 0.5));
        assert!(sim.world_mut().get::<Perks>(teammate).unwrap().has(perk));
        assert_eq!(balance(&mut sim, teammate), teammate_before - perk.price(RunType::Coop));
    }
}
