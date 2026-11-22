//! Contested Power: on a Map with Generators, Power comes from them, Section by Section, in place
//! of a Power Switch. A Player Claims a Generator by buying it and then Charging it, standing up on
//! its plate, and once it's Held its Section is Powered. Sections and Generators are `Layout` data,
//! so any Map can have them; only the Dam does.

use serde::{Deserialize, Serialize};

use bevy::prelude::*;

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::{Authority, HostSimulation};
use crate::economy::buy::{Buyable, Purchased};
use crate::enemies::target::{half_height_above_feet, is_up};
use crate::maps::map::{Layout, MapId};
use crate::players::last_stand::LastStand;
use crate::players::player::{Locomotion, Player};
use crate::players::respawn::{AwaitingRespawn, Disconnected};
use crate::state::{Choosing, GameState, InRun, Paused, world_runs};
use crate::tuning::*;
use crate::maps::zone::ZoneId;

/// A Section: a group of Zones Powered by one Generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SectionId {
    IntakeTower,
    SpillwayGallery,
    TurbineHall,
}

impl SectionId {
    pub fn name(self) -> &'static str {
        match self {
            SectionId::IntakeTower => "Intake Tower",
            SectionId::SpillwayGallery => "Spillway Gallery",
            SectionId::TurbineHall => "Turbine Hall",
        }
    }
}

/// A Generator, as `Layout` data: the Section it Powers, the Zone it stands in, the middle of its
/// plate on the floor, the way its console faces across the plate, and the price of a Claim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Generator {
    pub section: SectionId,
    pub zone: ZoneId,
    pub at: Vec3,
    pub facing: Vec3,
    pub price: u64,
}

impl Generator {
    /// Whether feet at `feet` stand on its plate.
    pub fn on_plate(&self, feet: Vec3) -> bool {
        let across = Vec2::new(feet.x - self.at.x, feet.z - self.at.z).length();
        across <= GENERATOR_PLATE_RADIUS && (feet.y - self.at.y).abs() <= GENERATOR_PLATE_SLACK
    }

    /// The middle of its console's foot, at the plate's back edge, and the console's size.
    pub fn console(&self) -> (Vec3, Vec3) {
        let depth = GENERATOR_CONSOLE.z;
        let size = if self.facing.x.abs() > 0.5 { GENERATOR_CONSOLE.zyx() } else { GENERATOR_CONSOLE };
        (self.at - self.facing * (GENERATOR_PLATE_RADIUS + depth / 2.0), size)
    }
}

impl Layout {
    /// The Section `zone` is in, if any.
    pub fn section_of(&self, zone: ZoneId) -> Option<SectionId> {
        self.sections.iter().find(|(_, zones)| zones.contains(&zone)).map(|&(section, _)| section)
    }

    /// Whether the Map has a Power Switch: one with Generators has none, and never turns its
    /// `Power` on.
    pub fn has_power_switch(&self) -> bool {
        self.generators.is_empty()
    }

    /// The Zones in `section`: none if it isn't on this Map.
    pub fn zones_of(&self, section: SectionId) -> &'static [ZoneId] {
        self.sections.iter().find(|(id, _)| *id == section).map_or(&[], |(_, zones)| zones)
    }
}

/// Where a Generator is in being Claimed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GeneratorState {
    /// No Player has Claimed it yet: it can be bought.
    Unclaimed,
    /// Bought and being Charged: `charged` of `needed` seconds done. `live` while it advances,
    /// with a Player who is up on its plate; paused otherwise, without losing any.
    Charging { charged: f32, needed: f32, live: bool },
    /// Claimed: its Section is Powered.
    Held,
    /// Taken by a Sapper: its Section has no Power until a Player Reclaims it, with a free Charge.
    Lost,
}

impl GeneratorState {
    /// How far the Charge has come, 0 to 1: none before it's bought, all once Held.
    pub fn fraction(self) -> f32 {
        match self {
            GeneratorState::Unclaimed | GeneratorState::Lost => 0.0,
            GeneratorState::Charging { charged, needed, .. } => (charged / needed).clamp(0.0, 1.0),
            GeneratorState::Held => 1.0,
        }
    }

    /// Whether it's being Charged right now, a Player up on its plate.
    pub fn live(self) -> bool {
        matches!(self, GeneratorState::Charging { live: true, .. })
    }
}

/// Every Generator on the Map, in `Layout.generators` order, and whether any has ever been Held.
/// Empty on a Map without them. The Host changes it; every other game shows what it says.
#[derive(Resource, Debug, Clone, Default)]
pub struct Generators {
    map: Option<MapId>,
    states: Vec<GeneratorState>,
    ever_held: bool,
}

impl Generators {
    /// `map`'s Generators, every one Unclaimed.
    pub fn for_map(map: MapId) -> Self {
        Self {
            map: Some(map),
            states: vec![GeneratorState::Unclaimed; map.layout().generators.len()],
            ever_held: false,
        }
    }

    /// Whether the Map has Generators. If it has, its Power comes from them and not from `Power`.
    pub fn on_map(&self) -> bool {
        !self.states.is_empty()
    }

    /// The Generators as `Layout` data, in the same order as `states`.
    pub fn layout(&self) -> &'static [Generator] {
        self.map.map_or(&[], |map| map.layout().generators)
    }

    /// Each Generator's state, in `Layout.generators` order.
    pub fn states(&self) -> &[GeneratorState] {
        &self.states
    }

    /// The Section `zone` is in, if any.
    pub fn section_of(&self, zone: ZoneId) -> Option<SectionId> {
        self.map.and_then(|map| map.layout().section_of(zone))
    }

    /// Whether `section`'s Generator is Held.
    pub fn section_powered(&self, section: SectionId) -> bool {
        self.layout()
            .iter()
            .zip(&self.states)
            .any(|(generator, &state)| generator.section == section && state == GeneratorState::Held)
    }

    /// Whether something in `zone` (`None` for nowhere in particular) that waits for Power works,
    /// as far as the Generators go: anything outside every Section always does, and anything in
    /// one only while that Section is Powered.
    pub fn powers(&self, zone: Option<ZoneId>) -> bool {
        zone.and_then(|zone| self.section_of(zone)).is_none_or(|section| self.section_powered(section))
    }

    /// Whether every Generator is Held, as Pack-a-Punch needs.
    pub fn all_held(&self) -> bool {
        self.states.iter().all(|&state| state == GeneratorState::Held)
    }

    /// Whether any Generator has ever been Held this Run, even if it isn't now.
    pub fn ever_held(&self) -> bool {
        self.ever_held
    }

    /// Buys Generator `index`, starting its Charge. False if it isn't Unclaimed.
    pub fn claim(&mut self, index: usize) -> bool {
        let Some(state) = self.states.get_mut(index) else { return false };
        if *state != GeneratorState::Unclaimed {
            return false;
        }
        *state = GeneratorState::Charging { charged: 0.0, needed: GENERATOR_CLAIM_CHARGE_SECS, live: false };
        true
    }

    /// A Sapper has taken Generator `index`: it's Lost. False if it isn't Held.
    pub fn lose(&mut self, index: usize) -> bool {
        let Some(state) = self.states.get_mut(index) else { return false };
        if *state != GeneratorState::Held {
            return false;
        }
        *state = GeneratorState::Lost;
        true
    }

    /// The Held Generator nearest `at`, if any is.
    pub fn nearest_held(&self, at: Vec3) -> Option<usize> {
        self.layout()
            .iter()
            .zip(&self.states)
            .enumerate()
            .filter(|(_, (_, state))| **state == GeneratorState::Held)
            .min_by(|(_, (a, _)), (_, (b, _))| a.at.distance(at).total_cmp(&b.at.distance(at)))
            .map(|(index, _)| index)
    }

    /// Starts the free Charge that Reclaims Lost Generator `index`. False if it isn't Lost.
    pub fn reclaim(&mut self, index: usize) -> bool {
        let Some(state) = self.states.get_mut(index) else { return false };
        if *state != GeneratorState::Lost {
            return false;
        }
        *state = GeneratorState::Charging { charged: 0.0, needed: GENERATOR_RECLAIM_CHARGE_SECS, live: false };
        true
    }

    /// Advances Generator `index`'s Charge by `dt` while `on_plate` (a Player who is up and can
    /// shoot stands on it), or pauses it. True on the frame it becomes Held.
    pub fn charge(&mut self, index: usize, dt: f32, on_plate: bool) -> bool {
        let Some(state) = self.states.get_mut(index) else { return false };
        let GeneratorState::Charging { charged, needed, live } = state else { return false };
        *live = on_plate;
        if !on_plate {
            return false;
        }
        *charged += dt;
        if *charged < *needed {
            return false;
        }
        self.hold(index);
        true
    }

    /// Generator `index` is Held, as the Host says or a debug key makes it.
    pub fn hold(&mut self, index: usize) {
        self.show(index, GeneratorState::Held);
    }

    /// Generator `index` is in `state`, as the Host says.
    pub fn show(&mut self, index: usize, state: GeneratorState) {
        if let Some(shown) = self.states.get_mut(index) {
            *shown = state;
            self.ever_held |= state == GeneratorState::Held;
        }
    }
}

/// A Generator in the world, by its place in `Layout.generators`: the Buyable that Claims it,
/// on its console, while it's Unclaimed.
#[derive(Component, Debug, Clone, Copy)]
pub struct GeneratorPlate(pub usize);

/// The drawn disc of Generator `index`'s plate, with its own material, and whether it glows now.
#[derive(Component, Debug, Clone)]
pub struct PlateDisc {
    pub index: usize,
    pub material: Handle<StandardMaterial>,
    pub lit: bool,
}

/// The emissive colour of a plate's disc, lit or not.
pub fn plate_emissive(lit: bool) -> LinearRgba {
    if lit { LinearRgba::from(GENERATOR_PLATE_GLOW) * GENERATOR_PLATE_GLOW_STRENGTH } else { LinearRgba::BLACK }
}

/// Which of `generators` have `feet` on their plate: all false for no feet.
pub fn on_plates(generators: &[Generator], feet: &[Vec3]) -> Vec<bool> {
    generators.iter().map(|generator| feet.iter().any(|&feet| generator.on_plate(feet))).collect()
}

/// Every plate glows while a Player who is up stands on it, and goes dark when they leave.
fn glow_plates(
    players: Query<(&Transform, Option<&Locomotion>, Has<LastStand>, Has<AwaitingRespawn>, Has<Disconnected>), With<Player>>,
    generators: Res<Generators>,
    mut discs: Query<&mut PlateDisc>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let feet: Vec<Vec3> = players
        .iter()
        .filter(|&(_, _, last_stand, awaiting, disconnected)| is_up(last_stand, awaiting, disconnected))
        .map(|(transform, locomotion, ..)| transform.translation - Vec3::Y * half_height_above_feet(locomotion))
        .collect();
    let on = on_plates(generators.layout(), &feet);
    for mut disc in &mut discs {
        let lit = on.get(disc.index).copied().unwrap_or(false);
        if disc.lit == lit {
            continue;
        }
        disc.lit = lit;
        if let Some(mut material) = materials.get_mut(&disc.material) {
            material.emissive = plate_emissive(lit);
        }
    }
}

/// Request to lose the Held Generator nearest `at`, as a Sapper would take it. The Host judges it.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct LoseNearestGenerator {
    pub at: Vec3,
}

/// A `LoseNearestGenerator` loses the Held Generator nearest it.
fn lose_nearest(mut asked: MessageReader<LoseNearestGenerator>, mut generators: ResMut<Generators>) {
    for request in asked.read() {
        if let Some(index) = generators.nearest_held(request.at) {
            generators.lose(index);
        }
    }
}

pub struct GeneratorPlugin;

impl Plugin for GeneratorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Generators>()
            .add_message::<LoseNearestGenerator>()
            .add_systems(OnEnter(GameState::Playing), (reset, spawn_plates))
            .add_systems(
                Update,
                (
                    (claim, lose_nearest, charge).chain().in_set(HostSimulation),
                    run_shown_charge.run_if(in_state(Authority::Client)),
                    (prompt_plates, announce, glow_plates),
                )
                    .chain()
                    .run_if(world_runs),
            );
    }
}

fn reset(mut commands: Commands, map: Res<MapId>) {
    commands.insert_resource(Generators::for_map(*map));
}

/// The Claim on an Unclaimed Generator's console.
fn claim_buyable(generator: &Generator) -> Buyable {
    let (foot, _) = generator.console();
    let face = foot + generator.facing * (GENERATOR_CONSOLE.z / 2.0 + GENERATOR_PROMPT_OUT) + Vec3::Y * GENERATOR_DIAL_HEIGHT;
    Buyable {
        use_point: face,
        prompt: format!("Press F to Claim the {} Generator", generator.section.name()),
        price: Some(generator.price),
        range: USE_RANGE,
    }
}

/// The free Reclaim on a Lost Generator's console.
fn reclaim_buyable(generator: &Generator) -> Buyable {
    Buyable {
        prompt: format!("Press F to Reclaim the {} Generator", generator.section.name()),
        price: None,
        ..claim_buyable(generator)
    }
}

/// What a Generator's console offers in `state`: a Claim while it's Unclaimed, a Reclaim while
/// it's Lost, and nothing otherwise.
fn offer(generator: &Generator, state: GeneratorState) -> Option<Buyable> {
    match state {
        GeneratorState::Unclaimed => Some(claim_buyable(generator)),
        GeneratorState::Lost => Some(reclaim_buyable(generator)),
        GeneratorState::Charging { .. } | GeneratorState::Held => None,
    }
}

fn spawn_plates(mut commands: Commands, map: Res<MapId>) {
    for (index, generator) in map.layout().generators.iter().enumerate() {
        let (foot, size) = generator.console();
        commands.spawn((
            InRun,
            Name::new(format!("{} Generator", generator.section.name())),
            GeneratorPlate(index),
            claim_buyable(generator),
            Transform::from_translation(foot + Vec3::Y * size.y / 2.0),
        ));
    }
}

/// A bought Generator starts its Charge, and so does a Reclaimed one. The purchase flow has
/// charged the price, Gold Rush and all; a Reclaim is free.
fn claim(mut purchases: MessageReader<Purchased>, plates: Query<&GeneratorPlate>, mut generators: ResMut<Generators>) {
    for purchase in purchases.read() {
        if let Ok(&GeneratorPlate(index)) = plates.get(purchase.buyable)
            && !generators.claim(index)
        {
            generators.reclaim(index);
        }
    }
}

/// Each bought Generator Charges while a Player who is up, and can shoot, stands on its plate.
pub(crate) fn charge(
    players: Query<
        (&Transform, Option<&Locomotion>, Has<LastStand>, Has<AwaitingRespawn>, Has<Disconnected>, Has<Choosing>, Has<Paused>),
        With<Player>,
    >,
    mut generators: ResMut<Generators>,
    time: Res<Time>,
) {
    let feet: Vec<Vec3> = players
        .iter()
        .filter(|&(_, _, last_stand, awaiting, disconnected, choosing, paused)| {
            is_up(last_stand, awaiting, disconnected) && !choosing && !paused
        })
        .map(|(transform, locomotion, ..)| transform.translation - Vec3::Y * half_height_above_feet(locomotion))
        .collect();
    for index in 0..generators.states().len() {
        if !matches!(generators.states()[index], GeneratorState::Charging { .. }) {
            continue;
        }
        let generator = generators.layout()[index];
        let on_plate = feet.iter().any(|&feet| generator.on_plate(feet));
        generators.charge(index, time.delta_secs(), on_plate);
    }
}

/// A client runs a live Charge on between the Host's words, so its meter fills smoothly. Only the
/// Host's word makes it Held.
fn run_shown_charge(mut generators: ResMut<Generators>, time: Res<Time>) {
    for state in &mut generators.states {
        if let GeneratorState::Charging { charged, needed, live: true } = state {
            *charged = (*charged + time.delta_secs()).min(*needed * GENERATOR_SHOWN_CHARGE_MAX);
        }
    }
}

/// An Unclaimed Generator offers its Claim, and a Lost one its Reclaim; one being Charged or Held
/// offers nothing.
fn prompt_plates(mut commands: Commands, plates: Query<(Entity, &GeneratorPlate, Option<&Buyable>)>, generators: Res<Generators>) {
    for (entity, &GeneratorPlate(index), offered) in &plates {
        let Some(&state) = generators.states().get(index) else { continue };
        let wanted = offer(&generators.layout()[index], state);
        match (wanted, offered) {
            (Some(wanted), Some(offered)) if wanted.prompt == offered.prompt => {}
            (Some(wanted), _) => {
                commands.entity(entity).insert(wanted);
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<Buyable>();
            }
            (None, None) => {}
        }
    }
}

/// Everyone hears it, wherever they are, when a Generator becomes Held, and when one is Lost.
fn announce(generators: Res<Generators>, mut was: Local<Vec<GeneratorState>>, mut cues: MessageWriter<PlayCue>) {
    if !generators.is_changed() {
        return;
    }
    if was.len() == generators.states().len() {
        for (&before, &now) in was.iter().zip(generators.states()) {
            let cue = match now {
                GeneratorState::Held => Cue::GeneratorHeld,
                GeneratorState::Lost => Cue::GeneratorLost,
                _ => continue,
            };
            if before != now {
                cues.write(PlayCue::new(cue, None));
            }
        }
    }
    *was = generators.states().to_vec();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::STOREY;

    fn dam() -> Generators {
        Generators::for_map(MapId::Dam)
    }

    /// The Dam's Generator for `section`.
    fn of(section: SectionId) -> usize {
        MapId::Dam.layout().generators.iter().position(|generator| generator.section == section).unwrap()
    }

    #[test]
    fn a_player_charges_from_anywhere_on_the_plate_but_not_off_it() {
        let generator = MapId::Dam.layout().generators[0];
        let at = generator.at;
        assert!(generator.on_plate(at));
        assert!(generator.on_plate(at + Vec3::X * (GENERATOR_PLATE_RADIUS - 0.1) + Vec3::Y * 0.2));
        assert!(!generator.on_plate(at + Vec3::Z * (GENERATOR_PLATE_RADIUS + 0.1)), "off its edge");
        assert!(!generator.on_plate(at + Vec3::Y * STOREY), "a storey up");
        assert!(!generator.on_plate(at - Vec3::Y * STOREY), "a storey down");
    }

    #[test]
    fn a_plate_is_on_only_for_feet_inside_it_and_glows_only_then() {
        let generators = MapId::Dam.layout().generators;
        let at = generators[0].at;
        assert_eq!(on_plates(generators, &[]), [false, false, false]);
        assert_eq!(on_plates(generators, &[at + Vec3::X * (GENERATOR_PLATE_RADIUS - 0.1)]), [true, false, false]);
        assert_eq!(on_plates(generators, &[at + Vec3::X * (GENERATOR_PLATE_RADIUS + 0.1)]), [false, false, false]);
        assert_eq!(plate_emissive(false), LinearRgba::BLACK);
        assert!(plate_emissive(true).red > 1.0, "lit, it's brighter than white's worth of red");
    }

    #[test]
    fn a_map_without_generators_has_none_and_powers_everything_in_its_zones() {
        let generators = Generators::for_map(MapId::Greybox);
        assert!(!generators.on_map());
        assert!(generators.powers(Some(ZoneId::PowerRoom)));
    }

    #[test]
    fn every_dam_generator_starts_unclaimed_and_powers_nothing() {
        let generators = dam();
        assert!(generators.on_map());
        assert_eq!(generators.states().len(), 3);
        assert!(generators.states().iter().all(|&state| state == GeneratorState::Unclaimed));
        assert!(!generators.powers(Some(ZoneId::IntakeGallery)));
        assert!(!generators.all_held());
        assert!(!generators.ever_held());
    }

    /// Anything outside every Section works from the start; one in a Section waits for its own
    /// Generator to be Held, and no other.
    #[test]
    fn a_held_generator_powers_its_own_section_and_nothing_waits_outside_every_section() {
        let mut generators = dam();
        assert!(generators.powers(Some(ZoneId::CrestRoad)));
        assert!(generators.powers(Some(ZoneId::ControlRoom)));
        assert!(generators.powers(None));
        generators.hold(of(SectionId::TurbineHall));
        assert!(generators.section_powered(SectionId::TurbineHall));
        assert!(generators.powers(Some(ZoneId::SwitchgearRoom)));
        assert!(generators.powers(Some(ZoneId::DrainageGallery)));
        assert!(!generators.powers(Some(ZoneId::TowerHead)));
        assert!(!generators.powers(Some(ZoneId::SpillwayGatehouse)));
    }

    #[test]
    fn pack_a_punch_waits_for_every_generator() {
        let mut generators = dam();
        generators.hold(of(SectionId::IntakeTower));
        generators.hold(of(SectionId::SpillwayGallery));
        assert!(!generators.all_held());
        generators.hold(of(SectionId::TurbineHall));
        assert!(generators.all_held());
    }

    #[test]
    fn a_claim_charges_for_twenty_seconds_on_the_plate_then_is_held() {
        let mut generators = dam();
        let intake = of(SectionId::IntakeTower);
        assert!(generators.claim(intake));
        assert!(!generators.claim(intake), "a Generator is bought once");
        assert_eq!(GENERATOR_CLAIM_CHARGE_SECS, 20.0);
        assert!(!generators.charge(intake, 19.0, true));
        assert!(generators.states()[intake].live());
        assert!(!generators.section_powered(SectionId::IntakeTower), "not Powered while Charging");
        assert!(generators.charge(intake, 1.0, true), "Held on the frame the Charge finishes");
        assert_eq!(generators.states()[intake], GeneratorState::Held);
        assert!(generators.section_powered(SectionId::IntakeTower));
        // Only once.
        assert!(!generators.charge(intake, 1.0, true));
    }

    #[test]
    fn an_empty_plate_pauses_the_charge_without_losing_any() {
        let mut generators = dam();
        let spillway = of(SectionId::SpillwayGallery);
        generators.claim(spillway);
        generators.charge(spillway, 12.0, true);
        generators.charge(spillway, 60.0, false);
        let paused = generators.states()[spillway];
        assert!(!paused.live());
        assert!((paused.fraction() - 0.6).abs() < 1e-4, "{paused:?}");
        assert!(generators.charge(spillway, 8.0, true), "it picks up where it left off");
    }

    #[test]
    fn an_unbought_generator_doesnt_charge() {
        let mut generators = dam();
        let turbine = of(SectionId::TurbineHall);
        assert!(!generators.charge(turbine, 60.0, true));
        assert_eq!(generators.states()[turbine], GeneratorState::Unclaimed);
    }

    #[test]
    fn the_power_counts_as_on_for_good_once_any_generator_is_held() {
        let mut generators = dam();
        let intake = of(SectionId::IntakeTower);
        generators.claim(intake);
        generators.charge(intake, 10.0, true);
        assert!(!generators.ever_held(), "bought isn't Claimed");
        generators.charge(intake, 10.0, true);
        assert!(generators.ever_held());
    }

    /// A Lost Generator unpowers its Section at once, but the Power still counts as having been
    /// on, and only a Held one can be Lost.
    #[test]
    fn a_lost_generator_unpowers_its_section_but_the_power_has_still_been_on() {
        let mut generators = dam();
        let turbine = of(SectionId::TurbineHall);
        assert!(!generators.lose(turbine), "an Unclaimed Generator can't be Lost");
        generators.hold(turbine);
        generators.hold(of(SectionId::IntakeTower));
        generators.hold(of(SectionId::SpillwayGallery));
        assert!(generators.lose(turbine));
        assert_eq!(generators.states()[turbine], GeneratorState::Lost);
        assert!(!generators.section_powered(SectionId::TurbineHall));
        assert!(!generators.powers(Some(ZoneId::SwitchgearRoom)));
        assert!(generators.powers(Some(ZoneId::TowerHead)), "the other Sections keep theirs");
        assert!(!generators.all_held(), "Pack-a-Punch goes off");
        assert!(generators.ever_held());
        assert!(!generators.lose(turbine), "Lost once");
        assert!(!generators.claim(turbine), "a Lost Generator isn't bought again");
    }

    #[test]
    fn the_nearest_held_generator_is_the_nearest_of_those_held() {
        let mut generators = dam();
        let [intake, spillway, turbine] = [SectionId::IntakeTower, SectionId::SpillwayGallery, SectionId::TurbineHall].map(of);
        let by_intake = MapId::Dam.layout().generators[intake].at;
        assert_eq!(generators.nearest_held(by_intake), None);
        generators.hold(spillway);
        generators.hold(turbine);
        let nearest = generators.nearest_held(by_intake).unwrap();
        assert!(nearest == spillway || nearest == turbine);
        generators.hold(intake);
        assert_eq!(generators.nearest_held(by_intake), Some(intake));
    }

    /// A Reclaim is a free Charge of ten seconds, by the same rules as a Claim's.
    #[test]
    fn a_reclaim_charges_for_ten_seconds_on_the_plate_then_is_held_again() {
        let mut generators = dam();
        let intake = of(SectionId::IntakeTower);
        assert!(!generators.reclaim(intake), "only a Lost Generator is Reclaimed");
        generators.hold(intake);
        assert!(!generators.reclaim(intake));
        generators.lose(intake);
        assert!(generators.reclaim(intake));
        assert!(!generators.reclaim(intake), "once");
        assert_eq!(GENERATOR_RECLAIM_CHARGE_SECS, 10.0);
        generators.charge(intake, 6.0, true);
        generators.charge(intake, 30.0, false);
        assert!((generators.states()[intake].fraction() - 0.6).abs() < 1e-4, "an empty plate pauses it");
        assert!(!generators.section_powered(SectionId::IntakeTower));
        assert!(generators.charge(intake, 4.0, true));
        assert_eq!(generators.states()[intake], GeneratorState::Held);
        assert!(generators.section_powered(SectionId::IntakeTower));
    }

    #[test]
    fn the_dams_zones_are_in_its_three_sections_but_the_crest_road_and_the_control_room() {
        use SectionId::{IntakeTower, SpillwayGallery};
        use ZoneId::*;
        let dam = MapId::Dam.layout();
        for (zone, section) in [
            (TowerHead, Some(IntakeTower)),
            (GateShaft, Some(IntakeTower)),
            (IntakeGallery, Some(IntakeTower)),
            (SpillwayGatehouse, Some(SpillwayGallery)),
            (ZoneId::SpillwayGallery, Some(SectionId::SpillwayGallery)),
            (SpillwayStair, Some(SectionId::SpillwayGallery)),
            (PenstockTunnel, Some(SectionId::TurbineHall)),
            (ZoneId::TurbineHall, Some(SectionId::TurbineHall)),
            (SwitchgearRoom, Some(SectionId::TurbineHall)),
            (DrainageGallery, Some(SectionId::TurbineHall)),
            (CrestRoad, None),
            (ControlRoom, None),
        ] {
            assert_eq!(dam.section_of(zone), section, "the {}", zone.name());
        }
    }

    /// One Generator per Section, on its plate in a Zone of that Section, at the Spec's prices.
    #[test]
    fn the_dam_has_a_generator_in_each_section_at_its_price() {
        let dam = MapId::Dam.layout();
        let mut found: Vec<(SectionId, ZoneId, u64)> = Vec::new();
        for generator in dam.generators {
            assert_eq!(crate::enemies::nav::zone_at(MapId::Dam, generator.at + Vec3::Y * 0.1), Some(generator.zone), "{generator:?}");
            assert_eq!(dam.section_of(generator.zone), Some(generator.section));
            found.push((generator.section, generator.zone, generator.price));
        }
        assert_eq!(
            found,
            [
                (SectionId::IntakeTower, ZoneId::IntakeGallery, 1500),
                (SectionId::SpillwayGallery, ZoneId::SpillwayGallery, 1500),
                (SectionId::TurbineHall, ZoneId::TurbineHall, 2000),
            ]
        );
    }

    #[test]
    fn the_other_maps_have_no_sections_or_generators() {
        for map in [MapId::Greybox, MapId::Quarry, MapId::Village] {
            let layout = map.layout();
            assert!(layout.sections.is_empty() && layout.generators.is_empty(), "the {}", map.name());
            assert!(layout.zones.iter().all(|&zone| layout.section_of(zone).is_none()));
        }
    }
}

/// Claiming a Generator on the built Dam: bought through the purchase flow, then Charged by a
/// Player standing on its plate.
#[cfg(test)]
mod claim_tests {
    use super::*;
    use crate::economy::buy::{BuyRefused, CantAfford, HoldPurchased, RefusedByPlayer, UseRequest, resolve_use};
    use crate::economy::points::{Points, PointsPlugin};
    use crate::enemies::zombie::headless::Sim;
    use crate::modifier::RunModifiers;

    /// The Dam, with its Generators, the purchase flow, and the stand-in Player holding 5000 Points.
    fn dam() -> (Sim, Entity) {
        let mut sim = Sim::on(MapId::Dam, 30.0, |app| {
            app.add_plugins((PointsPlugin, GeneratorPlugin))
                .init_resource::<RunModifiers>()
                .add_message::<UseRequest>()
                .add_message::<Purchased>()
                .add_message::<HoldPurchased>()
                .add_message::<CantAfford>()
                .add_message::<BuyRefused>()
                .add_message::<RefusedByPlayer>()
                .add_message::<PlayCue>()
                .add_systems(Update, resolve_use.before(claim));
        });
        let world = sim.world_mut();
        let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
        world.entity_mut(player).insert(Points { balance: 5000, ..default() });
        (sim, player)
    }

    fn console(sim: &mut Sim, index: usize) -> Entity {
        let world = sim.world_mut();
        world.query::<(Entity, &GeneratorPlate)>().iter(world).find(|(_, plate)| plate.0 == index).unwrap().0
    }

    fn stand(sim: &mut Sim, player: Entity, feet: Vec3) {
        sim.world_mut().get_mut::<Transform>(player).unwrap().translation = feet + Vec3::Y * half_height_above_feet(None);
    }

    fn state(sim: &mut Sim, index: usize) -> GeneratorState {
        sim.world_mut().resource::<Generators>().states()[index]
    }

    #[test]
    fn a_bought_generator_is_held_after_twenty_seconds_on_its_plate() {
        let (mut sim, player) = dam();
        let turbine = 2;
        let generator = MapId::Dam.layout().generators[turbine];
        let console = console(&mut sim, turbine);
        assert!(sim.world_mut().get::<Buyable>(console).is_some(), "an Unclaimed Generator offers its Claim");
        sim.send(UseRequest { player, buyable: console, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().get::<Points>(player).unwrap().balance, 5000 - generator.price);
        assert!(sim.world_mut().get::<Buyable>(console).is_none(), "a bought Generator offers nothing");
        // Off the plate, nothing.
        sim.run(sim.frames(5.0));
        assert_eq!(state(&mut sim, turbine).fraction(), 0.0);
        stand(&mut sim, player, generator.at);
        sim.run(sim.frames(GENERATOR_CLAIM_CHARGE_SECS - 1.0));
        assert!(state(&mut sim, turbine).live());
        assert!(!sim.world_mut().resource::<Generators>().section_powered(SectionId::TurbineHall));
        sim.run(sim.frames(1.5));
        assert_eq!(state(&mut sim, turbine), GeneratorState::Held);
        assert!(sim.world_mut().resource::<Generators>().section_powered(SectionId::TurbineHall));
    }

    /// A Lost Generator's console offers a free Reclaim, and ten seconds on the plate Hold it again.
    #[test]
    fn a_lost_generator_is_reclaimed_for_free_with_ten_seconds_on_its_plate() {
        let (mut sim, player) = dam();
        let spillway = 1;
        let generator = MapId::Dam.layout().generators[spillway];
        let console = console(&mut sim, spillway);
        let mut generators = sim.world_mut().resource_mut::<Generators>();
        generators.hold(spillway);
        generators.lose(spillway);
        sim.run(2);
        let offered = sim.world_mut().get::<Buyable>(console).expect("a Lost Generator offers its Reclaim");
        assert_eq!(offered.price, None, "a Reclaim is free");
        assert_eq!(offered.prompt, "Press F to Reclaim the Spillway Gallery Generator");
        sim.send(UseRequest { player, buyable: console, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().get::<Points>(player).unwrap().balance, 5000);
        assert!(sim.world_mut().get::<Buyable>(console).is_none(), "a Reclaim under way offers nothing");
        stand(&mut sim, player, generator.at);
        sim.run(sim.frames(GENERATOR_RECLAIM_CHARGE_SECS - 1.0));
        assert!(state(&mut sim, spillway).live());
        sim.run(sim.frames(1.5));
        assert_eq!(state(&mut sim, spillway), GeneratorState::Held);
    }

    #[test]
    fn going_down_on_the_plate_pauses_the_charge() {
        let (mut sim, player) = dam();
        let intake = 0;
        let generator = MapId::Dam.layout().generators[intake];
        sim.world_mut().resource_mut::<Generators>().claim(intake);
        stand(&mut sim, player, generator.at);
        sim.run(sim.frames(5.0));
        sim.world_mut().entity_mut(player).insert(LastStand::shown(10.0, true));
        let downed = state(&mut sim, intake).fraction();
        sim.run(sim.frames(10.0));
        assert!(!state(&mut sim, intake).live());
        assert_eq!(state(&mut sim, intake).fraction(), downed, "a Down Player doesn't Charge");
        assert!((downed - 5.0 / GENERATOR_CLAIM_CHARGE_SECS).abs() < 0.02, "{downed}");
    }
}
