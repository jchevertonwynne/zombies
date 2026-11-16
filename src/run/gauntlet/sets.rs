//! Set Bonuses: a Family's tiers, reached at 2, 4 and 6 distinct held Boons of it. Stacks of one
//! Boon count once, and the tiers add up and are never lost. Tier 2 is a stat, tier 4 changes a
//! rule and tier 6 is a capstone you can see (spec #182, Appendix A4).

use bevy::prelude::*;

use super::boon::{BoonDef, Family, HeldBoons};
use super::procs::{FROST_COLOUR, Frosted, feet, frost};
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::economy::points::{BonusPoints, PointsAwarded, kill_points};
use crate::economy::power_up::{ActivePowerUps, DropPowerUp};
use crate::enemies::zombie::{
    Attack, DamageTakenFactor, DamageZombie, Dying, HitZone, KillDamage, Pay, Rising, Zombie, ZombieKilled,
    ZombieMoveFactor,
};
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::players::player::{DownGuard, HealPlayer, LocalPlayer};
use crate::run::round::RoundStarted;
use crate::net::fx::Fx;
use crate::state::InRun;
use crate::tuning::*;
use crate::ui::hud::text;
use crate::weapons::ammo_mod::{AmmoMods, CastEffect, CastTarget, Element, TurnedAlly};
use crate::weapons::burn::Burning;
use crate::weapons::weapon::{KnifeSwung, LentAmmoMods, Loadout, ResetKnifeCooldown};

/// The tiers a Player's Families have reached this Run: for each Family, how many of
/// `SET_BONUS_TIERS` (0 to 3). Never lowered.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq)]
pub struct SetBonuses([u8; 6]);

impl SetBonuses {
    /// Whether `family` has reached tier `tier` (2, 4 or 6).
    pub fn has(&self, family: Family, tier: u32) -> bool {
        let reached = self.0[family as usize] as usize;
        SET_BONUS_TIERS[..reached].contains(&tier)
    }

    /// The Families at tier 6, one bit each in `Family::ALL` order.
    pub fn capstones(&self) -> u8 {
        Family::ALL.iter().enumerate().filter(|&(_, &family)| self.has(family, 6)).fold(0, |bits, (i, _)| bits | 1 << i)
    }

    /// What a teammate who holds the Capstones in `families` shows: every tier of those Families.
    pub fn from_capstones(families: u8) -> Self {
        let mut tiers = [0; 6];
        for (i, tier) in tiers.iter_mut().enumerate() {
            if families & (1 << i) != 0 {
                *tier = SET_BONUS_TIERS.len() as u8;
            }
        }
        Self(tiers)
    }

    /// Raises each Family to the tiers its distinct held Boons reach, returning every tier newly
    /// reached as (Family, tier), lowest first.
    pub fn reach(&mut self, held: &HeldBoons) -> Vec<(Family, u32)> {
        let mut reached = Vec::new();
        for family in Family::ALL {
            let was = self.0[family as usize];
            let now = tiers_for(held.in_family(family));
            if now > was {
                reached.extend(SET_BONUS_TIERS[was as usize..now as usize].iter().map(|&tier| (family, tier)));
                self.0[family as usize] = now;
            }
        }
        reached
    }
}

/// Each tier's own source, so it never adds into a Boon's.
const SOURCES: [[Source; 3]; 6] = [
    [Source("Lead 2"), Source("Lead 4"), Source("Lead 6")],
    [Source("Blade 2"), Source("Blade 4"), Source("Blade 6")],
    [Source("Spark 2"), Source("Spark 4"), Source("Spark 6")],
    [Source("Ember 2"), Source("Ember 4"), Source("Ember 6")],
    [Source("Gold 2"), Source("Gold 4"), Source("Gold 6")],
    [Source("Grit 2"), Source("Grit 4"), Source("Grit 6")],
];

/// The source a Family's tier (2, 4 or 6) puts its entries under.
fn source(family: Family, tier: u32) -> Source {
    let index = SET_BONUS_TIERS.iter().position(|&held| held == tier).unwrap_or(0);
    SOURCES[family as usize][index]
}

/// Whether a stat-modifier entry comes from a Set Bonus.
pub fn is_set_source(source: Source) -> bool {
    SOURCES.iter().flatten().any(|&held| held == source)
}

/// Whether a Magazine holding `magazine` of `size` is in its last third, for Lead 4.
pub fn magazine_low(magazine: u32, size: u32) -> bool {
    f64::from(magazine) <= f64::from(size) * SET_LEAD_4_MAGAZINE_FRACTION
}

/// The stat-modifier entries the tiers reached put on the Player: every tier 2's stat, and Lead 4's
/// damage while `magazine_low`.
pub fn set_entries(sets: &SetBonuses, magazine_low: bool) -> Vec<Modifier> {
    let mut entries = Vec::new();
    let mut add = |family, tier, stat, effect| {
        if sets.has(family, tier) {
            entries.push(Modifier { stat, source: source(family, tier), effect });
        }
    };
    add(Family::Lead, 2, Stat::HeadshotMultiplier, Effect::Flat(SET_LEAD_2_HEADSHOT_MULTIPLIER));
    add(Family::Blade, 2, Stat::KnifeDamage, Effect::Mult(1.0 + SET_BLADE_2_KNIFE_DAMAGE_INCREASE));
    add(Family::Spark, 2, Stat::AmmoModCooldown, Effect::Mult(SET_SPARK_2_COOLDOWN_FACTOR));
    add(Family::Spark, 2, Stat::TrapCooldown, Effect::Mult(SET_SPARK_2_COOLDOWN_FACTOR));
    add(Family::Ember, 2, Stat::EquipmentDamage, Effect::Mult(1.0 + SET_EMBER_2_DAMAGE_INCREASE));
    add(Family::Ember, 2, Stat::BurnSpeed, Effect::Mult(SET_EMBER_2_BURN_SPEED_FACTOR));
    add(Family::Gold, 2, Stat::PointsRate, Effect::Flat(SET_GOLD_2_POINTS_RATE));
    add(Family::Grit, 2, Stat::RoundRegen, Effect::Mult(SET_GRIT_2_REGEN_FACTOR));
    if magazine_low {
        add(Family::Lead, 4, Stat::Damage, Effect::Mult(1.0 + SET_LEAD_4_DAMAGE_INCREASE));
    }
    entries
}

/// How many of `SET_BONUS_TIERS` `distinct` held Boons of one Family reach.
pub fn tiers_for(distinct: u32) -> u8 {
    SET_BONUS_TIERS.iter().filter(|&&tier| distinct >= tier).count() as u8
}

/// What a Family's tier gives, as its banner says it.
fn tier_effect(family: Family, tier: u32) -> String {
    let percent = |x: f32| format!("{:.0}%", x * 100.0);
    match (family, tier) {
        (Family::Lead, 2) => format!("+{SET_LEAD_2_HEADSHOT_MULTIPLIER:.1}x Headshot multiplier"),
        (Family::Lead, 4) => format!("last third of the Magazine +{} damage", percent(SET_LEAD_4_DAMAGE_INCREASE)),
        (Family::Lead, _) => "Ricochet".into(),
        (Family::Blade, 2) => format!("+{} Knife damage", percent(SET_BLADE_2_KNIFE_DAMAGE_INCREASE)),
        (Family::Blade, 4) => format!("a Knife kill heals {SET_BLADE_4_HEAL:.0} and readies the Knife"),
        (Family::Blade, _) => "Cleave".into(),
        (Family::Spark, 2) => format!("Ammo Mod and Trap cooldowns x{SET_SPARK_2_COOLDOWN_FACTOR:.1}"),
        (Family::Spark, 4) => "the Upgraded Weapon lends its Ammo Mods".into(),
        (Family::Spark, _) => "Storm Aura".into(),
        (Family::Ember, 2) => format!(
            "+{} Equipment damage, burns x{SET_EMBER_2_BURN_SPEED_FACTOR:.1} faster",
            percent(SET_EMBER_2_DAMAGE_INCREASE)
        ),
        (Family::Ember, 4) => format!("burning Zombies take x{SET_EMBER_4_DAMAGE_TAKEN_FACTOR:.2} damage"),
        (Family::Ember, _) => "Scorched".into(),
        (Family::Gold, 2) => format!("+{SET_GOLD_2_POINTS_RATE:.1} Points rate"),
        (Family::Gold, 4) => "kills pay twice during timed Power-ups".into(),
        (Family::Gold, _) => "Jackpot".into(),
        (Family::Grit, 2) => format!("Health regenerates x{SET_GRIT_2_REGEN_FACTOR:.1} in a Round"),
        (Family::Grit, 4) => "a Down guard every Round".into(),
        (Family::Grit, _) => "Bodyguard".into(),
    }
}

/// A Set Bonus tier's banner, e.g. "LEAD 4: last third of the Magazine +50% damage".
pub fn banner_line(family: Family, tier: u32) -> String {
    format!("{} {tier}: {}", family.name().to_uppercase(), tier_effect(family, tier))
}

/// The tier a pick of `boon` would reach, as its card's tag ("→ Lead 4"), if it reaches one.
pub fn tier_tag(held: &HeldBoons, boon: &BoonDef) -> Option<String> {
    if held.stacks(boon.id) > 0 || boon.duo.is_some() {
        return None;
    }
    let count = held.in_family(boon.family) + 1;
    SET_BONUS_TIERS
        .contains(&count)
        .then(|| format!("→ {} {count}", boon.family.name()))
}

/// Spark 4: the Ammo Mods lent to every hit, given each slot's (Upgraded, Ammo Mods) and the slot
/// in hand. The first Upgraded slot lends its own while another is held.
pub fn lent_ammo_mods(slots: &[(bool, AmmoMods)], held: usize) -> AmmoMods {
    match slots.iter().position(|&(upgraded, _)| upgraded) {
        Some(lender) if lender != held => slots[lender].1,
        _ => AmmoMods::NONE,
    }
}

/// Cleave: whether a Zombie at `at` is within a swing's reach from `origin` along `forward`, and
/// inside its arc of `SET_BLADE_6_ARC_DEGREES`, on the flat.
pub fn in_cleave(origin: Vec3, forward: Vec3, at: Vec3, range: f32) -> bool {
    let offset = (at - origin).with_y(0.0);
    let ahead = forward.with_y(0.0).normalize_or_zero();
    if offset.length() > range + ZOMBIE_RADIUS {
        return false;
    }
    if offset.length() < 1e-3 {
        return true;
    }
    ahead.angle_between(offset) <= (SET_BLADE_6_ARC_DEGREES / 2.0).to_radians()
}

/// Ricochet: the nearest of `others` within `SET_LEAD_6_RICOCHET_RANGE_M` of the kill at `at`.
pub fn ricochet_target(at: Vec3, others: &[Vec3]) -> Option<usize> {
    others
        .iter()
        .enumerate()
        .map(|(i, other)| (i, other.distance(at)))
        .filter(|&(_, distance)| distance <= SET_LEAD_6_RICOCHET_RANGE_M)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Gold 4: the Points a kill pays again, while a timed Power-up is on. An Unpaid kill pays none.
pub fn gold_4_points(kill: &ZombieKilled, timed_on: bool) -> u64 {
    if timed_on { kill_points(kill) } else { 0 }
}

/// Jackpot's count of Points toward the next drop, from when Gold 6 is reached.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq)]
pub struct Jackpot(u64);

impl Jackpot {
    /// Counts `amount` Points earned, returning how many drops they complete.
    pub fn earn(&mut self, amount: u64) -> u32 {
        let total = self.0.saturating_add(amount);
        self.0 = total % SET_GOLD_6_POINTS_PER_DROP;
        (total / SET_GOLD_6_POINTS_PER_DROP) as u32
    }

    #[cfg(test)]
    pub fn progress(&self) -> u64 {
        self.0
    }

    /// How far toward the next drop, from 0 to 1.
    pub fn filled(&self) -> f32 {
        self.0 as f32 / SET_GOLD_6_POINTS_PER_DROP as f32
    }
}

/// The capstones' own state on a Player: Storm Aura's clock and the Bodyguard.
#[derive(Component, Debug, Default)]
pub struct Capstones {
    /// Seconds since Storm Aura's last zap.
    aura: f32,
    /// The Bodyguard, once turned.
    guard: Option<Entity>,
    /// A Bodyguard is due: the next eligible Zombie to spawn becomes it.
    owed: bool,
    /// The Zombie picked to become it, waiting to be out of the ground.
    candidate: Option<Entity>,
}

/// A Player's Family reached a Set Bonus tier.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetTierReached {
    pub player: Entity,
    pub family: Family,
    pub tier: u32,
}

/// The cue that marks a tier, rising with it.
fn tier_cue(tier: u32) -> Cue {
    match tier {
        2 => Cue::SetBonusTier2,
        4 => Cue::SetBonusTier4,
        _ => Cue::SetBonusTier6,
    }
}

/// Each Player's Families reach the tiers their held Boons now make, and the `LocalPlayer` hears
/// each one.
pub fn reach_set_bonuses(
    mut players: Query<(Entity, &HeldBoons, &mut SetBonuses, Has<LocalPlayer>), Changed<HeldBoons>>,
    mut reached: MessageWriter<SetTierReached>,
    mut cues: MessageWriter<PlayCue>,
) {
    for (player, held, mut sets, local) in &mut players {
        for (family, tier) in sets.reach(held) {
            reached.write(SetTierReached { player, family, tier });
            if local {
                cues.write(PlayCue::new(tier_cue(tier), None));
            }
        }
    }
}

/// The Set Bonus banner, top-centre, and the seconds it has left.
#[derive(Component)]
pub struct SetBanner(f32);

pub fn spawn_set_banner(mut commands: Commands) {
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(15.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child((SetBanner(0.0), text("", 34.0, Color::WHITE), Visibility::Hidden));
}

/// A tier the `LocalPlayer` reaches shows its banner in its Family's colour for
/// `GAUNTLET_SET_BANNER_SECS`.
pub fn show_set_banner(
    mut reached: MessageReader<SetTierReached>,
    local: Query<(), With<LocalPlayer>>,
    banner: Single<(&mut SetBanner, &mut Text, &mut TextColor, &mut Visibility)>,
    time: Res<Time<Real>>,
) {
    let (mut banner, mut text, mut colour, mut visibility) = banner.into_inner();
    for tier in reached.read().filter(|tier| local.contains(tier.player)) {
        text.0 = banner_line(tier.family, tier.tier);
        colour.0 = tier.family.colour();
        banner.0 = GAUNTLET_SET_BANNER_SECS;
    }
    banner.0 -= time.delta_secs();
    visibility.set_if_neq(if banner.0 > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
}

/// Blade 4: a Knife kill readies the Knife at once and heals its Player.
pub fn blade_4(
    mut kills: MessageReader<ZombieKilled>,
    players: Query<&SetBonuses>,
    mut resets: MessageWriter<ResetKnifeCooldown>,
    mut heals: MessageWriter<HealPlayer>,
) {
    for kill in kills.read().filter(|kill| kill.attack == Attack::Knife) {
        let Some(player) = kill.player else {
            continue;
        };
        if players.get(player).is_ok_and(|sets| sets.has(Family::Blade, 4)) {
            resets.write(ResetKnifeCooldown { player });
            heals.write(HealPlayer { player, amount: SET_BLADE_4_HEAL });
        }
    }
}

/// Spark 4: a Player's Upgraded Weapon lends its Ammo Mods to the other while it's held.
pub fn lend_ammo_mods(
    mut commands: Commands,
    mut players: Query<(Entity, &SetBonuses, &Loadout, Option<&mut LentAmmoMods>)>,
) {
    for (player, sets, loadout, lent) in &mut players {
        let wanted = if sets.has(Family::Spark, 4) {
            let slots: Vec<_> = loadout
                .slots()
                .iter()
                .map(|slot| slot.as_ref().map_or((false, AmmoMods::NONE), |copy| (copy.upgraded, copy.ammo_mods)))
                .collect();
            lent_ammo_mods(&slots, loadout.held_slot())
        } else {
            AmmoMods::NONE
        };
        match lent {
            Some(mut lent) => {
                if lent.0 != wanted {
                    lent.0 = wanted;
                }
            }
            None if !wanted.is_empty() => {
                commands.entity(player).insert(LentAmmoMods(wanted));
            }
            None => {}
        }
    }
}

/// Marks the `DamageTakenFactor` Ember 4 put on a burning Zombie.
#[derive(Component)]
pub struct Seared;

/// Ember 4: while anyone holds it, a burning Zombie takes more damage, body and head.
pub fn ember_4(
    mut commands: Commands,
    players: Query<&SetBonuses>,
    zombies: Query<(Entity, Has<Burning>, Has<Seared>), (With<Zombie>, Without<Dying>)>,
) {
    let on = players.iter().any(|sets| sets.has(Family::Ember, 4));
    for (zombie, burning, seared) in &zombies {
        if on && burning && !seared {
            let factor = SET_EMBER_4_DAMAGE_TAKEN_FACTOR;
            commands.entity(zombie).insert((DamageTakenFactor { body: factor, head: factor }, Seared));
        } else if seared && !(on && burning) {
            commands.entity(zombie).remove::<(DamageTakenFactor, Seared)>();
        }
    }
}

/// Gold 4: while a timed Power-up is on, a Paid kill pays its kill Points again.
pub fn gold_4(
    mut kills: MessageReader<ZombieKilled>,
    players: Query<&SetBonuses>,
    power_ups: Res<ActivePowerUps>,
    mut bonus: MessageWriter<BonusPoints>,
) {
    let timed_on = ActivePowerUps::TIMED.iter().any(|&power_up| power_ups.active(power_up));
    for kill in kills.read() {
        let Some(player) = kill.player else {
            continue;
        };
        if !players.get(player).is_ok_and(|sets| sets.has(Family::Gold, 4)) {
            continue;
        }
        let amount = gold_4_points(kill, timed_on);
        if amount > 0 {
            bonus.write(BonusPoints { player, amount });
        }
    }
}

/// Grit 4: a Down guard as the tier is reached, refilled at each Round's start.
pub fn grit_4(
    mut commands: Commands,
    mut started: MessageReader<RoundStarted>,
    mut reached: MessageReader<SetTierReached>,
    players: Query<(Entity, &SetBonuses)>,
) {
    let mut guarded: Vec<Entity> = reached
        .read()
        .filter(|tier| tier.family == Family::Grit && tier.tier == 4)
        .map(|tier| tier.player)
        .collect();
    if started.read().count() > 0 {
        guarded.extend(players.iter().filter(|(_, sets)| sets.has(Family::Grit, 4)).map(|(player, _)| player));
    }
    for player in guarded {
        commands.entity(player).insert(DownGuard { charges: SET_GRIT_4_GUARDS_PER_ROUND });
    }
}

/// Short-lived capstone visuals, drawn by Gauntlet.
#[derive(Resource, Default)]
pub struct SetFlashes {
    flashes: Vec<SetFlash>,
    /// What the Host drew, for it to tell the Players (`crate::net::fx`).
    pub sent: Vec<Fx>,
}

impl SetFlashes {
    /// Shows `flash` and tells the Players of it.
    fn add(&mut self, flash: SetFlash) {
        let SetFlash { shape, colour, left: secs } = flash;
        self.sent.push(match shape {
            FlashShape::Line(from, to) => Fx::Tracer { from, to, colour, secs },
            FlashShape::Arc(feet, forward, radius) => Fx::Cleave { feet, forward, radius, colour, secs },
            FlashShape::Burst(at, radius) => Fx::Jackpot { at, radius, colour, secs },
            FlashShape::Disc(at, radius) => Fx::Scorched { at, radius, colour, secs },
        });
        self.flashes.push(SetFlash { shape, colour, left: secs });
    }

    /// Tells the Players of a Frost patch, which each draws from its own `FrostPatch`.
    fn tell_patch(&mut self, at: Vec3, secs: f32) {
        self.sent.push(Fx::Ring { at, secs });
    }

    /// Tells the Players of a Supply Drop frag in flight.
    pub fn tell_shell(&mut self, from: Vec3, to: Vec3) {
        self.sent.push(Fx::Arc { from, to });
    }

    /// Shows an effect the Host told of, which is not told on.
    pub fn hear(&mut self, fx: Fx) {
        let (shape, colour, left) = match fx {
            Fx::Tracer { from, to, colour, secs } => (FlashShape::Line(from, to), colour, secs),
            Fx::Cleave { feet, forward, radius, colour, secs } => (FlashShape::Arc(feet, forward, radius), colour, secs),
            Fx::Jackpot { at, radius, colour, secs } => (FlashShape::Burst(at, radius), colour, secs),
            Fx::Scorched { at, radius, colour, secs } => (FlashShape::Disc(at, radius), colour, secs),
            _ => return,
        };
        self.flashes.push(SetFlash { shape, colour, left });
    }

    /// How many are showing.
    #[cfg(test)]
    pub fn showing(&self) -> usize {
        self.flashes.len()
    }
}

pub struct SetFlash {
    shape: FlashShape,
    colour: Color,
    left: f32,
}

enum FlashShape {
    /// Ricochet's tracer.
    Line(Vec3, Vec3),
    /// Cleave's sweep: (feet, forward, radius).
    Arc(Vec3, Vec3, f32),
    /// Jackpot's burst.
    Burst(Vec3, f32),
    /// Scorched's flickering patch.
    Disc(Vec3, f32),
}

/// Lead 6, Ricochet: a Headshot kill's bullet goes on to the head of the nearest other Zombie,
/// at the killing bullet's damage, as Paid Ammo Mod damage. One bounce.
pub fn ricochet(
    mut kills: MessageReader<KillDamage>,
    players: Query<&SetBonuses>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut damage: MessageWriter<DamageZombie>,
    mut flashes: ResMut<SetFlashes>,
) {
    for kill in kills.read().filter(|kill| kill.headshot && kill.attack == Attack::Bullet) {
        let Some(player) = kill.player else {
            continue;
        };
        if !players.get(player).is_ok_and(|sets| sets.has(Family::Lead, 6)) {
            continue;
        }
        let (others, positions): (Vec<Entity>, Vec<Vec3>) = zombies
            .iter()
            .filter(|(zombie, _)| *zombie != kill.zombie)
            .map(|(zombie, at)| (zombie, at.translation()))
            .unzip();
        let head = kill.at + Vec3::Y * RICOCHET_HEAD_HEIGHT;
        let Some(i) = ricochet_target(head, &positions) else {
            continue;
        };
        damage.write(DamageZombie {
            player: Some(player),
            zombie: others[i],
            damage: kill.damage,
            zone: HitZone::Head,
            attack: Attack::AmmoMod(Pay::Paid),
        });
        flashes.add(SetFlash {
            shape: FlashShape::Line(head, positions[i] + Vec3::Y * 0.7),
            colour: Family::Lead.colour(),
            left: CAPSTONE_FLASH_SECS,
        });
    }
}

/// How high above a dead Zombie's feet its head was, for Ricochet's tracer.
const RICOCHET_HEAD_HEIGHT: f32 = 1.6;
/// Ricochet's tracer, Cleave's sweep and Jackpot's burst last this long.
const CAPSTONE_FLASH_SECS: f32 = 0.3;

/// A Cleave's Frost trail patch: Frosts every Zombie in it until it fades.
#[derive(Component)]
pub struct FrostPatch {
    at: Vec3,
    left: f32,
}

impl FrostPatch {
    /// A patch a client shows for `secs`, which Frosts no one.
    pub fn fading(at: Vec3, secs: f32) -> Self {
        Self { at, left: secs }
    }
}

/// On a client, the Host's Frost patches fade and go (the Host's own `chill_patches` does it there).
pub fn fade_patches(mut commands: Commands, mut patches: Query<(Entity, &mut FrostPatch)>, time: Res<Time>) {
    for (entity, mut patch) in &mut patches {
        patch.left -= time.delta_secs();
        if patch.left <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// Blade 6, Cleave: a swing also strikes every other Zombie in Knife range within its arc, for
/// the Knife's damage as Paid Ammo Mod damage, and leaves a Frost patch where it went.
pub fn cleave(
    mut commands: Commands,
    mut swings: MessageReader<KnifeSwung>,
    players: Query<(&SetBonuses, &GlobalTransform)>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut damage: MessageWriter<DamageZombie>,
    mut flashes: ResMut<SetFlashes>,
) {
    for swing in swings.read() {
        let Ok((sets, transform)) = players.get(swing.player) else {
            continue;
        };
        if !sets.has(Family::Blade, 6) {
            continue;
        }
        for (zombie, at) in &zombies {
            if Some(zombie) != swing.struck && in_cleave(swing.origin, swing.forward, at.translation(), swing.range) {
                damage.write(DamageZombie {
                    player: Some(swing.player),
                    zombie,
                    damage: swing.damage,
                    zone: HitZone::Torso,
                    attack: Attack::AmmoMod(Pay::Paid),
                });
            }
        }
        let feet = feet(transform);
        let ahead = swing.forward.with_y(0.0).normalize_or_zero();
        commands.spawn((
            InRun,
            Name::new("Cleave trail"),
            FrostPatch { at: feet + ahead * swing.range / 2.0, left: SET_BLADE_6_TRAIL_SECS },
        ));
        flashes.tell_patch(feet + ahead * swing.range / 2.0, SET_BLADE_6_TRAIL_SECS);
        flashes.add(SetFlash {
            shape: FlashShape::Arc(feet, ahead, swing.range),
            colour: Family::Blade.colour(),
            left: CAPSTONE_FLASH_SECS,
        });
    }
}

/// Cleave's Frost patches Frost every Zombie standing in them, then fade.
pub fn chill_patches(
    mut commands: Commands,
    mut patches: Query<(Entity, &mut FrostPatch)>,
    mut zombies: Query<
        (Entity, &GlobalTransform, Option<&mut ZombieMoveFactor>, Option<&mut Frosted>),
        (With<Zombie>, Without<Dying>),
    >,
    time: Res<Time>,
) {
    for (entity, mut patch) in &mut patches {
        patch.left -= time.delta_secs();
        if patch.left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        for (zombie, at, factor, frosted) in &mut zombies {
            if at.translation().with_y(0.0).distance(patch.at.with_y(0.0)) <= SET_BLADE_6_TRAIL_RADIUS_M {
                frost(&mut commands, zombie, factor, frosted, SET_BLADE_6_TRAIL_FROST_SECS);
            }
        }
    }
}

/// Spark 6, Storm Aura: every `SET_SPARK_6_AURA_TICK_SECS` a Shock strikes one Zombie within
/// `SET_SPARK_6_AURA_RADIUS_M` of the Player, Paid. Never thinned.
pub fn storm_aura(
    mut players: Query<(Entity, &SetBonuses, &GlobalTransform, &mut Capstones)>,
    mut casts: MessageWriter<CastEffect>,
    time: Res<Time>,
) {
    for (player, sets, transform, mut capstones) in &mut players {
        if !sets.has(Family::Spark, 6) {
            continue;
        }
        capstones.aura += time.delta_secs();
        if capstones.aura < SET_SPARK_6_AURA_TICK_SECS {
            continue;
        }
        capstones.aura -= SET_SPARK_6_AURA_TICK_SECS;
        casts.write(CastEffect {
            player,
            element: Element::Shock,
            target: CastTarget::Point(transform.translation()),
            size: 1.0,
            reach: Some(SET_SPARK_6_AURA_RADIUS_M),
            lasts: 0.0,
            pay: Pay::Paid,
            accent: None,
            show: true,
            unbounded: false,
            tint: None,
            ignites: false,
        });
    }
}

/// Ember 6, Scorched: every Player kill leaves a short fire patch where it fell, Paid. Never
/// thinned; Gauntlet draws it as a flickering disc.
pub fn scorched(
    mut kills: MessageReader<ZombieKilled>,
    players: Query<&SetBonuses>,
    mut casts: MessageWriter<CastEffect>,
    mut flashes: ResMut<SetFlashes>,
) {
    for kill in kills.read().filter(|kill| matches!(kill.attack, Attack::Bullet | Attack::Knife)) {
        let Some(player) = kill.player else {
            continue;
        };
        let Ok(sets) = players.get(player) else {
            continue;
        };
        if !sets.has(Family::Ember, 6) {
            continue;
        }
        casts.write(CastEffect {
            player,
            element: Element::Fire,
            target: CastTarget::Point(kill.at),
            size: SET_EMBER_6_PATCH_RADIUS_M,
            reach: None,
            lasts: SET_EMBER_6_PATCH_SECS,
            pay: Pay::Paid,
            accent: None,
            show: false,
            unbounded: false,
            tint: None,
            ignites: false,
        });
        flashes.add(SetFlash {
            shape: FlashShape::Disc(kill.at, SET_EMBER_6_PATCH_RADIUS_M),
            colour: Family::Ember.colour(),
            left: SET_EMBER_6_PATCH_SECS,
        });
    }
}

/// Gold 6, Jackpot: every `SET_GOLD_6_POINTS_PER_DROP` Points a holder earns drops a Power-up at
/// their feet, beyond the per-Round max.
pub fn jackpot(
    mut awarded: MessageReader<PointsAwarded>,
    mut players: Query<(&SetBonuses, &mut Jackpot, &GlobalTransform)>,
    mut drops: MessageWriter<DropPowerUp>,
    mut flashes: ResMut<SetFlashes>,
) {
    for award in awarded.read() {
        let Ok((sets, mut jackpot, transform)) = players.get_mut(award.player) else {
            continue;
        };
        if !sets.has(Family::Gold, 6) {
            continue;
        }
        let at = feet(transform);
        for _ in 0..jackpot.earn(award.amount) {
            drops.write(DropPowerUp { at, power_up: None });
            flashes.add(SetFlash {
                shape: FlashShape::Burst(at + Vec3::Y * 0.5, 1.2),
                colour: Family::Gold.colour(),
                left: CAPSTONE_FLASH_SECS * 2.0,
            });
        }
    }
}

/// Grit 6, Bodyguard: once the tier is reached, the first eligible Zombie to spawn is Turned
/// for good, outside the Turned ally cap and glowing Grit red; again after each Round's start if it
/// was lost.
#[allow(clippy::too_many_arguments)]
pub fn bodyguard(
    mut reached: MessageReader<SetTierReached>,
    mut started: MessageReader<RoundStarted>,
    mut players: Query<(Entity, &SetBonuses, &mut Capstones)>,
    spawned: Query<(Entity, &Zombie), Added<Zombie>>,
    zombies: Query<(Has<Rising>, Has<Dying>), With<Zombie>>,
    allies: Query<(), With<TurnedAlly>>,
    mut casts: MessageWriter<CastEffect>,
) {
    for tier in reached.read().filter(|tier| tier.family == Family::Grit && tier.tier == 6) {
        if let Ok((.., mut capstones)) = players.get_mut(tier.player) {
            capstones.owed = true;
        }
    }
    let new_round = started.read().count() > 0;
    // The first eligible Zombie to spawn this frame: an ordinary one, not a Hellhound, a Nova
    // Crawler or a special.
    let fresh = spawned
        .iter()
        .find(|(_, zombie)| zombie.special_kind().is_none() && !zombie.is_hellhound() && !zombie.is_nova_crawler())
        .map(|(entity, _)| entity);
    for (player, sets, mut capstones) in &mut players {
        if !sets.has(Family::Grit, 6) {
            continue;
        }
        if new_round && capstones.guard.is_none_or(|guard| !allies.contains(guard)) {
            capstones.guard = None;
            capstones.owed = true;
        }
        if !capstones.owed {
            continue;
        }
        if capstones.candidate.is_none() {
            capstones.candidate = fresh;
        }
        let Some(candidate) = capstones.candidate else {
            continue;
        };
        match zombies.get(candidate) {
            // Still climbing out: it's turned once it stands.
            Ok((true, false)) => {}
            Ok((false, false)) => {
                casts.write(CastEffect {
                    player,
                    element: Element::Turned,
                    target: CastTarget::Zombie(candidate),
                    size: 0.0,
                    reach: None,
                    lasts: 0.0,
                    pay: Pay::Unpaid,
                    accent: None,
                    show: true,
                    unbounded: true,
                    tint: Some(Family::Grit.colour()),
                    ignites: false,
                });
                capstones.guard = Some(candidate);
                capstones.owed = false;
                capstones.candidate = None;
            }
            // Killed before it stood: wait for the next to spawn.
            _ => capstones.candidate = None,
        }
    }
}

/// Draws the capstones' looks: Storm Aura's ring around each holder, and the flashes.
pub fn draw_set_flashes(
    players: Query<(&SetBonuses, &GlobalTransform)>,
    mut flashes: ResMut<SetFlashes>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let aura = Family::Spark.colour().with_alpha(0.35);
    for (sets, transform) in &players {
        if sets.has(Family::Spark, 6) {
            gizmos.circle(Isometry3d::new(feet(transform) + Vec3::Y * 0.05, flat), SET_SPARK_6_AURA_RADIUS_M, aura);
        }
    }
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();
    flashes.flashes.retain_mut(|flash| {
        flash.left -= dt;
        match flash.shape {
            FlashShape::Line(from, to) => {
                gizmos.line(from, to, flash.colour);
            }
            FlashShape::Arc(feet, forward, radius) => {
                let half = (SET_BLADE_6_ARC_DEGREES / 2.0).to_radians();
                let at = |angle: f32| feet + Vec3::Y * 1.0 + Quat::from_rotation_y(angle) * forward * radius;
                const STEPS: usize = 12;
                for step in 0..STEPS {
                    let a = -half + 2.0 * half * step as f32 / STEPS as f32;
                    let b = -half + 2.0 * half * (step + 1) as f32 / STEPS as f32;
                    gizmos.line(at(a), at(b), flash.colour);
                }
            }
            FlashShape::Burst(at, radius) => {
                gizmos.sphere(Isometry3d::from_translation(at), radius, flash.colour);
            }
            FlashShape::Disc(at, radius) => {
                // Flickering: a few rings whose size wavers.
                for ring in 1..=3 {
                    let wobble = 0.85 + 0.15 * (elapsed * 23.0 + ring as f32 * 1.7).sin();
                    let r = radius * ring as f32 / 3.0 * wobble;
                    gizmos.circle(Isometry3d::new(at + Vec3::Y * 0.06, flat), r, flash.colour);
                }
            }
        }
        flash.left > 0.0
    });
}

/// Cleave's Frost patches show as pale rings while they last.
pub fn draw_frost_patches(patches: Query<&FrostPatch>, mut gizmos: Gizmos) {
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for patch in &patches {
        let fade = (patch.left / SET_BLADE_6_TRAIL_SECS).clamp(0.0, 1.0);
        gizmos.circle(
            Isometry3d::new(patch.at + Vec3::Y * 0.05, flat),
            SET_BLADE_6_TRAIL_RADIUS_M,
            FROST_COLOUR.with_alpha(fade),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::gauntlet::boon::BOONS;
    use crate::modifier::resolve;
    use crate::weapons::ammo_mod::AmmoMod;

    fn named(id: &str) -> &'static super::super::boon::BoonDef {
        BOONS.iter().find(|boon| boon.id == id).unwrap()
    }

    fn holding(ids: &[&str]) -> HeldBoons {
        let mut held = HeldBoons::default();
        for id in ids {
            held.add(named(id));
        }
        held
    }

    const LEAD: [&str; 6] = ["hollow_points", "marksmans_eye", "full_belt", "stripper_clips", "parting_shot", "third_hand"];

    #[test]
    fn a_familys_tiers_come_at_two_four_and_six_distinct_boons() {
        assert_eq!(
            (0..=7).map(tiers_for).collect::<Vec<_>>(),
            [0, 0, 1, 1, 2, 2, 3, 3]
        );
    }

    #[test]
    fn stacks_of_one_boon_count_once_toward_a_tier() {
        let mut sets = SetBonuses::default();
        let held = holding(&["hollow_points", "hollow_points", "hollow_points"]);
        assert!(sets.reach(&held).is_empty());
        assert!(!sets.has(Family::Lead, 2));
    }

    #[test]
    fn each_tier_is_reported_once_as_it_is_reached() {
        let mut sets = SetBonuses::default();
        assert_eq!(sets.reach(&holding(&LEAD[..2])), [(Family::Lead, 2)]);
        assert!(sets.reach(&holding(&LEAD[..3])).is_empty());
        assert_eq!(sets.reach(&holding(&LEAD[..4])), [(Family::Lead, 4)]);
        assert!(sets.has(Family::Lead, 2) && sets.has(Family::Lead, 4));
        assert!(!sets.has(Family::Lead, 6));
        assert!(!sets.has(Family::Blade, 2), "other Families are their own");
    }

    #[test]
    fn several_tiers_reached_at_once_are_each_reported() {
        let mut sets = SetBonuses::default();
        assert_eq!(sets.reach(&holding(&LEAD)), [(Family::Lead, 2), (Family::Lead, 4), (Family::Lead, 6)]);
    }

    #[test]
    fn a_tier_reached_is_never_lost() {
        let mut sets = SetBonuses::default();
        sets.reach(&holding(&LEAD));
        assert!(sets.reach(&HeldBoons::default()).is_empty());
        assert!(sets.has(Family::Lead, 6));
    }

    fn reaching(family: Family, tiers: u8) -> SetBonuses {
        let mut sets = SetBonuses::default();
        sets.0[family as usize] = tiers;
        sets
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn at(sets: &SetBonuses, stat: Stat, base: f32) -> f32 {
        resolve(stat, base, &set_entries(sets, false))
    }

    #[test]
    fn no_tier_reached_puts_no_entries() {
        assert!(set_entries(&SetBonuses::default(), true).is_empty());
    }

    #[test]
    fn lead_2_adds_half_to_the_headshot_multiplier() {
        assert!(close(at(&reaching(Family::Lead, 1), Stat::HeadshotMultiplier, 3.5), 4.0));
    }

    #[test]
    fn blade_2_adds_a_quarter_to_knife_damage() {
        assert!(close(at(&reaching(Family::Blade, 1), Stat::KnifeDamage, 100.0), 125.0));
    }

    #[test]
    fn spark_2_shortens_ammo_mod_and_trap_cooldowns_by_a_tenth() {
        let sets = reaching(Family::Spark, 1);
        assert!(close(at(&sets, Stat::AmmoModCooldown, 10.0), 9.0));
        assert!(close(at(&sets, Stat::TrapCooldown, 30.0), 27.0));
    }

    #[test]
    fn ember_2_adds_a_fifth_to_equipment_damage_and_burns_tick_faster() {
        let sets = reaching(Family::Ember, 1);
        assert!(close(at(&sets, Stat::EquipmentDamage, 1.0), 1.2));
        assert!(close(at(&sets, Stat::BurnSpeed, 1.0), 1.2));
    }

    #[test]
    fn gold_2_adds_a_tenth_to_the_points_rate() {
        assert!(close(at(&reaching(Family::Gold, 1), Stat::PointsRate, 0.0), 0.1));
    }

    #[test]
    fn grit_2_regenerates_half_again_as_fast_in_a_round() {
        assert!(close(at(&reaching(Family::Grit, 1), Stat::RoundRegen, 2.0), 3.0));
    }

    #[test]
    fn higher_tiers_keep_the_tier_2_stat() {
        assert!(close(at(&reaching(Family::Lead, 3), Stat::HeadshotMultiplier, 3.5), 4.0));
    }

    #[test]
    fn lead_4_adds_half_damage_only_in_the_last_third_of_the_magazine() {
        assert!(magazine_low(10, 30));
        assert!(magazine_low(0, 30));
        assert!(!magazine_low(11, 30));
        assert!(magazine_low(1, 4), "1 of 4 is under a third");
        assert!(!magazine_low(2, 4));
        let damage = |tiers, low| resolve(Stat::Damage, 100.0, &set_entries(&reaching(Family::Lead, tiers), low));
        assert!(close(damage(2, true), 150.0));
        assert!(close(damage(2, false), 100.0));
        assert!(close(damage(1, true), 100.0), "not before Lead 4");
    }

    #[test]
    fn each_tier_is_its_own_source() {
        let mut sets = reaching(Family::Lead, 2);
        sets.0[Family::Ember as usize] = 1;
        for entry in set_entries(&sets, true) {
            assert!(is_set_source(entry.source));
        }
        assert!(!is_set_source(Source("Hollow Points")));
    }

    #[test]
    fn spark_4_lends_the_upgraded_weapons_ammo_mods_while_the_other_is_held() {
        let none = AmmoMods::NONE;
        let wire = none.with(AmmoMod::DeadWire);
        let furnace = none.with(AmmoMod::BlastFurnace);
        assert_eq!(lent_ammo_mods(&[(true, wire), (false, none)], 1), wire);
        assert_eq!(lent_ammo_mods(&[(true, wire), (false, none)], 0), none, "it's in hand already");
        assert_eq!(lent_ammo_mods(&[(false, none), (true, wire)], 0), wire);
        // Both Upgraded: the first Upgraded slot lends, while the other is held.
        assert_eq!(lent_ammo_mods(&[(true, wire), (true, furnace)], 1), wire);
        assert_eq!(lent_ammo_mods(&[(true, wire), (true, furnace)], 0), none);
        assert_eq!(lent_ammo_mods(&[(false, none), (false, none)], 0), none);
    }

    #[test]
    fn cleave_strikes_within_knife_range_and_a_120_degree_arc() {
        let origin = Vec3::new(0.0, 1.6, 0.0);
        let forward = Vec3::NEG_Z;
        let hits = |x: f32, z: f32| in_cleave(origin, forward, Vec3::new(x, 1.0, z), 1.6);
        assert!(hits(0.0, -1.5), "straight ahead");
        assert!(hits(1.0, -1.0), "45° off");
        assert!(!hits(1.5, -0.5), "past 60° off");
        assert!(!hits(0.0, -2.5), "out of reach");
        assert!(!hits(0.0, 1.0), "behind");
        // A longer reach (Long Arm) reaches further.
        assert!(in_cleave(origin, forward, Vec3::new(0.0, 1.0, -2.5), 2.35));
    }

    #[test]
    fn ricochet_goes_to_the_nearest_other_zombie_within_eight_metres() {
        let at = Vec3::ZERO;
        let others = [Vec3::new(9.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 5.0), Vec3::new(-3.0, 0.0, 0.0)];
        assert_eq!(ricochet_target(at, &others), Some(2));
        assert_eq!(ricochet_target(at, &others[..1]), None);
        assert_eq!(ricochet_target(at, &[]), None);
    }

    #[test]
    fn jackpot_drops_a_power_up_every_four_thousand_points_earned() {
        let mut jackpot = Jackpot::default();
        assert_eq!(jackpot.earn(3000), 0);
        assert_eq!(jackpot.earn(1500), 1);
        assert_eq!(jackpot.progress(), 500);
        assert_eq!(jackpot.earn(8000), 2);
        assert_eq!(jackpot.progress(), 500);
        assert!((jackpot.filled() - 0.125).abs() < 1e-6);
    }

    #[test]
    fn gold_4_pays_a_paid_kill_again_only_while_a_timed_power_up_is_on() {
        use crate::economy::points::kill_points;
        let kill = |pay| ZombieKilled { player: None, pay, headshot: false, zone: HitZone::Torso, attack: Attack::Bullet, at: Vec3::ZERO, surge: false, zombie: Entity::PLACEHOLDER };
        let paid = kill(Pay::Paid);
        assert_eq!(gold_4_points(&paid, true), kill_points(&paid));
        assert!(gold_4_points(&paid, true) > 0);
        assert_eq!(gold_4_points(&paid, false), 0);
        assert_eq!(gold_4_points(&kill(Pay::Unpaid), true), 0);
    }

    #[test]
    fn a_banner_names_the_family_and_tier_and_what_it_gives() {
        assert_eq!(banner_line(Family::Lead, 4), "LEAD 4: last third of the Magazine +50% damage");
        assert_eq!(banner_line(Family::Grit, 6), "GRIT 6: Bodyguard");
    }

    #[test]
    fn a_pick_that_reaches_a_tier_is_tagged_with_it() {
        let held = holding(&LEAD[..3]);
        assert_eq!(tier_tag(&held, named("third_hand")), Some("→ Lead 4".to_string()));
        assert_eq!(tier_tag(&held, named("hollow_points")), None, "a held Boon adds no distinct one");
        assert_eq!(tier_tag(&held, named("long_arm")), None, "Blade has none held");
        assert_eq!(tier_tag(&holding(&["trench_raider"]), named("long_arm")), Some("→ Blade 2".to_string()));
        let duo = crate::run::gauntlet::boon::find("gold_standard").unwrap();
        assert_eq!(tier_tag(&holding(&["hollow_points", "blood_money"]), duo), None, "a Duo counts toward no Family");
    }

    #[test]
    fn families_count_apart() {
        let mut sets = SetBonuses::default();
        let reached = sets.reach(&holding(&["hollow_points", "trench_raider", "light_feet", "long_arm"]));
        assert_eq!(reached, [(Family::Blade, 2)]);
    }
}
