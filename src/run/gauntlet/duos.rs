//! Duo Boons: one for each pair of Families, each eligible once its two named recipe Boons are
//! held. A Duo is Legendary, held once with no Stacks, has no Unlock price (it's in the pool once
//! both recipe Boons are Unlocked), stays if a recipe Boon is lost, and counts toward neither
//! Family (spec #182, Appendix A5).

use bevy::prelude::*;

use super::boon::{BUTCHERS_BILL, BoonDef, Family, HeldBoons, Rarity, RunFacts, WAR_BONDS, find};
use super::procs::{Frosted, ProcFlashes, feet};
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::economy::points::BonusPoints;
use crate::economy::power_up::PowerUpTaken;
use crate::enemies::zombie::{Attack, DamageZombie, Dying, HitZone, Pay, TurnedAllyHit, Zombie, ZombieKilled};
use crate::modifier::{Effect, Modifiers, Stat};
use crate::players::last_stand::Down;
use crate::players::player::{LocalPlayer, Player};
use crate::ui::hud::text;
use crate::run::round::Round;
use crate::state::InRun;
use crate::tuning::*;
use crate::weapons::ammo_mod::{AmmoMod, AmmoMods, CastEffect, CastTarget, Element, TurnedAlly, TurnedAllyExpired};
use crate::weapons::burn::{Burning, Ignite};
use crate::weapons::equipment::{GiveLethals, equipment_splash};
use crate::weapons::explosive::{Explosion, Explosive, makes_crawlers};
use crate::weapons::weapon::{RefillMagazine, ResetKnifeCooldown};

/// A Duo's recipe: the two Boons it needs, by stable ID, and their Families in the same order.
#[derive(Debug, PartialEq)]
pub struct DuoDef {
    pub recipe: [&'static str; 2],
    pub families: [Family; 2],
}

pub const GUN_AND_BLADE: &str = "gun_and_blade";
pub const BALL_LIGHTNING: &str = "ball_lightning";
pub const INCENDIARY_VOLLEY: &str = "incendiary_volley";
pub const GOLD_STANDARD: &str = "gold_standard";
pub const CONSCRIPTS: &str = "conscripts";
pub const STORM_SHELL: &str = "storm_shell";
pub const THERMAL_SHOCK: &str = "thermal_shock";
pub const CUT_OF_THE_TAKE: &str = "cut_of_the_take";
pub const REPEL_BOARDERS: &str = "repel_boarders";
pub const LIVE_FUSE: &str = "live_fuse";
pub const HOUSE_EDGE: &str = "house_edge";
pub const GALVANISED_DEAD: &str = "galvanised_dead";
pub const SUPPLY_DROP: &str = "supply_drop";
pub const MARTYRS: &str = "martyrs";
pub const LIFE_INSURANCE: &str = "life_insurance";

/// A Duo Boon: Legendary, with no Requirement beyond its recipe. Its own Family field is its first
/// recipe Boon's, but a Duo counts toward neither.
const fn duo(
    id: &'static str,
    name: &'static str,
    def: &'static DuoDef,
    per_stack: &'static [(Stat, Effect)],
    effect: fn(u32) -> String,
) -> BoonDef {
    BoonDef {
        id,
        name,
        family: def.families[0],
        rarity: Rarity::Legendary,
        starts_unlocked: false,
        requirements: &[],
        per_stack,
        effect,
        proc: None,
        duo: Some(def),
    }
}

const fn recipe(a: &'static str, family_a: Family, b: &'static str, family_b: Family) -> DuoDef {
    DuoDef { recipe: [a, b], families: [family_a, family_b] }
}

/// Every Duo Boon.
pub const DUOS: &[BoonDef] = &[
    duo(GUN_AND_BLADE, "Gun and Blade", &recipe("marksmans_eye", Family::Lead, "trench_raider", Family::Blade), &[], |_| {
        format!("Headshot kills ready the Knife; Knife kills put {DUO_GUN_AND_BLADE_BULLETS} bullets back")
    }),
    duo(BALL_LIGHTNING, "Ball Lightning", &recipe("parting_shot", Family::Lead, "arc_light", Family::Spark), &[], |_| {
        format!("each Zombie a Parting Shot Blast kills chains Shock to {DUO_BALL_LIGHTNING_CHAIN_ZOMBIES} more")
    }),
    duo(
        INCENDIARY_VOLLEY,
        "Incendiary Volley",
        &recipe("firing_squad", Family::Lead, "hot_brass", Family::Ember),
        &[],
        |_| "your phantom guns' shots set Zombies alight (Paid)".into(),
    ),
    duo(GOLD_STANDARD, "Gold Standard", &recipe("hollow_points", Family::Lead, "blood_money", Family::Gold), &[], |_| {
        format!("Headshot kills pay Blood Money x{DUO_GOLD_STANDARD_FACTOR}")
    }),
    duo(CONSCRIPTS, "Conscripts", &recipe("full_belt", Family::Lead, "turncoat", Family::Grit), &[], |_| {
        format!("each Turned ally fires a phantom gun of {DUO_CONSCRIPTS_SHOTS} shots every {DUO_CONSCRIPTS_INTERVAL_SECS:.0} s")
    }),
    duo(STORM_SHELL, "Storm Shell", &recipe("static_discharge", Family::Blade, "live_wire", Family::Spark), &[], |_| {
        format!(
            "Static Discharge also fires with {DUO_STORM_SHELL_CROWD} Zombies within {DUO_STORM_SHELL_RADIUS_M:.0} m; its cooldown x{DUO_STORM_SHELL_COOLDOWN_FACTOR:.1}"
        )
    }),
    duo(THERMAL_SHOCK, "Thermal Shock", &recipe("frostbite", Family::Blade, "wildfire", Family::Ember), &[], |_| {
        format!("a Frosted Zombie set alight shatters in a {DUO_THERMAL_SHOCK_RADIUS_M:.1} m Blast")
    }),
    duo(
        CUT_OF_THE_TAKE,
        "Cut of the Take",
        &recipe("butchers_bill", Family::Blade, "lucky_strike", Family::Gold),
        &[],
        |_| format!("each Butcher's Bill drop is {DUO_CUT_OF_THE_TAKE_DROPS} Power-ups"),
    ),
    duo(REPEL_BOARDERS, "Repel Boarders", &recipe("elbow_room", Family::Blade, "barbed_wire", Family::Grit), &[], |_| {
        format!(
            "Elbow Room throws Zombies back outside, -{:.0}% health",
            DUO_REPEL_BOARDERS_HEALTH_FRACTION * 100.0
        )
    }),
    duo(LIVE_FUSE, "Live Fuse", &recipe("short_circuit", Family::Spark, "firebrand", Family::Ember), &[], |_| {
        format!("each Firebrand pool Shocks a Zombie in it every {DUO_LIVE_FUSE_TICK_SECS:.0} s")
    }),
    duo(
        HOUSE_EDGE,
        "House Edge",
        &recipe("black_market", Family::Spark, "loaded_dice", Family::Gold),
        &[
            (Stat::BoxUpgradeChance, Effect::Flat(1.0)),
            (Stat::BoxAmmoModRolls, Effect::Flat(DUO_HOUSE_EDGE_AMMO_MODS)),
        ],
        |_| format!("Box Weapons come out Upgraded, with {DUO_HOUSE_EDGE_AMMO_MODS:.0} Ammo Mod"),
    ),
    duo(
        GALVANISED_DEAD,
        "Galvanised Dead",
        &recipe("live_wire", Family::Spark, "raise_the_dead", Family::Grit),
        &[],
        |_| format!("a Zombie that hits your Turned ally is Shocked, chaining to {DUO_GALVANISED_DEAD_CHAIN_ZOMBIES} more"),
    ),
    duo(SUPPLY_DROP, "Supply Drop", &recipe("spoils_of_war", Family::Gold, "bandolier", Family::Ember), &[], |_| {
        format!("a Power-up gives {DUO_SUPPLY_DROP_LETHALS} Lethal and drops a free frag on the thickest crowd")
    }),
    duo(MARTYRS, "Martyrs", &recipe("turncoat", Family::Grit, "shrapnel", Family::Ember), &[], |_| {
        format!("a Turned ally that falls Blasts {DUO_MARTYRS_RADIUS_M:.1} m around it")
    }),
    duo(LIFE_INSURANCE, "Life Insurance", &recipe("war_bonds", Family::Gold, "last_rites", Family::Grit), &[], |_| {
        "going Down pays War Bonds' Round-end Points at once".into()
    }),
];

/// Whether a Boon is Unlocked for this Run's Offers.
fn unlocked(id: &str, run: &RunFacts) -> bool {
    run.unlock_all
        || find(id).is_some_and(|boon| boon.starts_unlocked)
        || run.unlocked.iter().any(|held| held == id)
}

/// Whether a Duo can be on an Offer: both recipe Boons held and Unlocked, and the Duo not held.
pub fn duo_eligible(duo: &BoonDef, held: &HeldBoons, run: &RunFacts) -> bool {
    let Some(def) = duo.duo else {
        return false;
    };
    held.stacks(duo.id) == 0 && def.recipe.iter().all(|&id| held.stacks(id) > 0 && unlocked(id, run))
}

/// Every Duo in `table` the Player is eligible for: what an Offer may draw, and what the Boss
/// Offer's guarantee picks from.
pub fn duo_pool(table: &'static [BoonDef], held: &HeldBoons, run: &RunFacts) -> Vec<&'static BoonDef> {
    table.iter().filter(|duo| duo_eligible(duo, held, run)).collect()
}

/// The Duos a pick of `boon` would make eligible, for its card's "Duo ready" tag: those whose
/// other recipe Boon is held, while the Duo isn't.
pub fn duo_ready(boon: &BoonDef, held: &HeldBoons) -> Vec<&'static BoonDef> {
    if held.stacks(boon.id) > 0 {
        return Vec::new();
    }
    DUOS.iter()
        .filter(|duo| held.stacks(duo.id) == 0)
        .filter(|duo| partner(duo, boon.id).is_some_and(|other| held.stacks(other) > 0))
        .collect()
}

/// The other recipe Boon of `duo`, if `id` is one of its two.
pub fn partner(duo: &BoonDef, id: &str) -> Option<&'static str> {
    let [a, b] = duo.duo?.recipe;
    if a == id {
        Some(b)
    } else if b == id {
        Some(a)
    } else {
        None
    }
}

/// While `boon` is held and a Duo it's in isn't yet eligible, the partner it waits for and the
/// Duo, as (partner's name, Duo's name).
pub fn partners_wanted(boon: &BoonDef, held: &HeldBoons) -> Vec<(&'static str, &'static str)> {
    if held.stacks(boon.id) == 0 {
        return Vec::new();
    }
    DUOS.iter()
        .filter(|duo| held.stacks(duo.id) == 0)
        .filter_map(|duo| {
            let other = partner(duo, boon.id)?;
            (held.stacks(other) == 0).then(|| (find(other).map_or(other, |boon| boon.name), duo.name))
        })
        .collect()
}

/// Gold Standard: Blood Money's Points for a kill, paid again on a Headshot by a holder.
pub fn gold_standard_points(blood_money: u64, headshot: bool, holds: bool) -> u64 {
    if headshot && holds { blood_money.saturating_mul(DUO_GOLD_STANDARD_FACTOR) } else { blood_money }
}

/// Storm Shell: whether `near` Zombies within `DUO_STORM_SHELL_RADIUS_M` are a crowd enough to
/// fire Static Discharge.
pub fn storm_shell_crowded(near: usize) -> bool {
    near >= DUO_STORM_SHELL_CROWD
}

/// Storm Shell: Static Discharge's cooldown for a holder of it.
pub fn storm_shell_cooldown(cooldown: f32, holds: bool) -> f32 {
    if holds { cooldown * DUO_STORM_SHELL_COOLDOWN_FACTOR } else { cooldown }
}

/// Galvanised Dead: whether an ally last answered for at `last` (seconds) may answer again `now`.
pub fn galvanised_ready(last: Option<f32>, now: f32) -> bool {
    last.is_none_or(|last| now - last >= DUO_GALVANISED_DEAD_COOLDOWN_SECS)
}

/// Repel Boarders: the health a Zombie of `max_health` loses as it's flung outside.
pub fn repel_boarders_damage(max_health: f32) -> f32 {
    max_health * DUO_REPEL_BOARDERS_HEALTH_FRACTION
}

/// Supply Drop: where the free frag lands, among `zombies` within `DUO_SUPPLY_DROP_RANGE_M` of
/// `from`: the one with the most others within the frag's splash radius, nearest first on a tie.
pub fn densest_point(from: Vec3, zombies: &[Vec3]) -> Option<Vec3> {
    let radius = FRAG_SPLASH.radius;
    zombies
        .iter()
        .filter(|at| at.distance(from) <= DUO_SUPPLY_DROP_RANGE_M)
        .map(|&at| {
            let crowd = zombies.iter().filter(|other| other.distance(at) <= radius).count();
            (at, crowd)
        })
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.distance(from).total_cmp(&a.0.distance(from))))
        .map(|(at, _)| at)
}

/// Life Insurance: War Bonds' Round-end Points, paid at once on a Down.
pub fn life_insurance_points(round: u32, war_bonds_stacks: u32) -> u64 {
    super::boon::war_bonds_points(round, war_bonds_stacks)
}

/// Whether `player` holds the Boon or Duo `id`.
fn holds(players: &Query<&HeldBoons, With<Player>>, player: Option<Entity>, id: &str) -> bool {
    player.and_then(|player| players.get(player).ok()).is_some_and(|held| held.stacks(id) > 0)
}

/// A Duo's cast: Paid, shown, with its first Family's accent ring.
fn duo_cast(player: Entity, element: Element, target: CastTarget, size: f32, duo: &str) -> CastEffect {
    CastEffect {
        player,
        element,
        target,
        size,
        reach: None,
        lasts: 0.0,
        pay: Pay::Paid,
        accent: find(duo).map(|duo| duo.family.colour()),
        show: true,
        unbounded: false,
        tint: None,
        ignites: false,
    }
}

/// A Duo's Blast: kills every Zombie within `radius` of `at` (Paid, no Crawlers), with the Proc
/// Blast's look and cue.
fn blast(
    at: Vec3,
    radius: f32,
    player: Option<Entity>,
    zombies: &Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    damage: &mut MessageWriter<DamageZombie>,
    flashes: &mut ProcFlashes,
    cues: &mut MessageWriter<PlayCue>,
) {
    for (zombie, transform) in zombies {
        if transform.translation().distance(at) <= radius {
            damage.write(DamageZombie {
                player,
                zombie,
                damage: f32::INFINITY,
                zone: HitZone::Torso,
                attack: Attack::AmmoMod(Pay::Paid),
            });
        }
    }
    flashes.blast(at, radius);
    cues.write(PlayCue::new(Cue::ProcBlast, Some(at)));
}

/// Gun and Blade: a Headshot kill readies the Knife, and a Knife kill puts bullets back in the
/// held Weapon's Magazine.
pub fn gun_and_blade(
    mut kills: MessageReader<ZombieKilled>,
    players: Query<&HeldBoons, With<Player>>,
    mut resets: MessageWriter<ResetKnifeCooldown>,
    mut refills: MessageWriter<RefillMagazine>,
) {
    for kill in kills.read() {
        let Some(player) = kill.player.filter(|_| holds(&players, kill.player, GUN_AND_BLADE)) else {
            continue;
        };
        if kill.headshot {
            resets.write(ResetKnifeCooldown { player });
        }
        if kill.attack == Attack::Knife {
            refills.write(RefillMagazine { player, bullets: DUO_GUN_AND_BLADE_BULLETS });
        }
    }
}

/// Cut of the Take: how many Power-ups each Butcher's Bill drop makes for this holder.
pub fn butchers_bill_drops_for(held: &HeldBoons) -> u32 {
    if held.stacks(CUT_OF_THE_TAKE) > 0 && held.stacks(BUTCHERS_BILL) > 0 { DUO_CUT_OF_THE_TAKE_DROPS } else { 1 }
}

/// Life Insurance: going Down pays War Bonds' Round-end Points at once.
pub fn life_insurance(
    mut downs: MessageReader<Down>,
    players: Query<&HeldBoons, With<Player>>,
    round: Res<Round>,
    mut bonus: MessageWriter<BonusPoints>,
) {
    for down in downs.read() {
        let Ok(held) = players.get(down.player) else {
            continue;
        };
        if held.stacks(LIFE_INSURANCE) == 0 {
            continue;
        }
        let amount = life_insurance_points(round.number, held.stacks(WAR_BONDS));
        if amount > 0 {
            bonus.write(BonusPoints { player: down.player, amount });
        }
    }
}

/// Martyrs: a holder's Turned ally Blasts around it as it falls.
pub fn martyrs(
    mut expired: MessageReader<TurnedAllyExpired>,
    players: Query<&HeldBoons, With<Player>>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut damage: MessageWriter<DamageZombie>,
    mut flashes: ResMut<ProcFlashes>,
    mut cues: MessageWriter<PlayCue>,
) {
    for fallen in expired.read() {
        if holds(&players, fallen.player, MARTYRS) {
            blast(fallen.at, DUO_MARTYRS_RADIUS_M, fallen.player, &zombies, &mut damage, &mut flashes, &mut cues);
        }
    }
}

/// Thermal Shock: a Frosted Zombie a holder sets alight shatters in a Blast.
pub fn thermal_shock(
    mut ignites: MessageReader<Ignite>,
    players: Query<&HeldBoons, With<Player>>,
    frosted: Query<&GlobalTransform, (With<Frosted>, Without<Burning>, Without<Dying>)>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut damage: MessageWriter<DamageZombie>,
    mut flashes: ResMut<ProcFlashes>,
    mut cues: MessageWriter<PlayCue>,
) {
    let mut shattered: Vec<Entity> = Vec::new();
    for ignite in ignites.read() {
        if shattered.contains(&ignite.zombie) || !holds(&players, Some(ignite.player), THERMAL_SHOCK) {
            continue;
        }
        if let Ok(at) = frosted.get(ignite.zombie) {
            shattered.push(ignite.zombie);
            let at = at.translation();
            blast(at, DUO_THERMAL_SHOCK_RADIUS_M, Some(ignite.player), &zombies, &mut damage, &mut flashes, &mut cues);
        }
    }
}

/// When Galvanised Dead last answered for a Turned ally, in seconds of game time.
#[derive(Component, Debug)]
pub struct Galvanised(f32);

/// Galvanised Dead: a Zombie that hits a holder's Turned ally takes a Shock that chains on, at
/// most once per ally every `DUO_GALVANISED_DEAD_COOLDOWN_SECS`.
pub fn galvanised_dead(
    mut commands: Commands,
    mut hits: MessageReader<TurnedAllyHit>,
    mut allies: Query<(&TurnedAlly, Option<&mut Galvanised>)>,
    players: Query<&HeldBoons, With<Player>>,
    mut casts: MessageWriter<CastEffect>,
    mut cues: MessageWriter<PlayCue>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    for hit in hits.read() {
        let Ok((ally, last)) = allies.get_mut(hit.ally) else {
            continue;
        };
        let Some(player) = ally.player().filter(|&player| holds(&players, Some(player), GALVANISED_DEAD)) else {
            continue;
        };
        if !galvanised_ready(last.as_ref().map(|last| last.0), now) {
            continue;
        }
        match last {
            Some(mut last) => last.0 = now,
            None => {
                commands.entity(hit.ally).insert(Galvanised(now));
            }
        }
        let chain = DUO_GALVANISED_DEAD_CHAIN_ZOMBIES as f32;
        casts.write(duo_cast(player, Element::Shock, CastTarget::Zombie(hit.by), chain, GALVANISED_DEAD));
        cues.write(PlayCue {
            cue: AmmoMod::DeadWire.cue(),
            at: None,
            pitch: GAUNTLET_PROC_CUE_PITCH,
            gain: GAUNTLET_PROC_CUE_GAIN,
        });
    }
}

/// Seconds to a Turned ally's next Conscripts volley.
#[derive(Component, Debug)]
pub struct Conscript(f32);

/// Conscripts: each of a holder's Turned allies raises a phantom gun every
/// `DUO_CONSCRIPTS_INTERVAL_SECS` (Unpaid), igniting under Incendiary Volley.
pub fn conscripts(
    mut commands: Commands,
    mut allies: Query<(Entity, &TurnedAlly, &GlobalTransform, Option<&mut Conscript>)>,
    players: Query<&HeldBoons, With<Player>>,
    mut casts: MessageWriter<CastEffect>,
    time: Res<Time>,
) {
    for (entity, ally, at, clock) in &mut allies {
        let Some(player) = ally.player().filter(|&player| holds(&players, Some(player), CONSCRIPTS)) else {
            continue;
        };
        let Some(mut clock) = clock else {
            commands.entity(entity).insert(Conscript(DUO_CONSCRIPTS_INTERVAL_SECS));
            continue;
        };
        clock.0 -= time.delta_secs();
        if clock.0 > 0.0 {
            continue;
        }
        clock.0 = DUO_CONSCRIPTS_INTERVAL_SECS;
        let feet = at.translation() - Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0);
        let shots = DUO_CONSCRIPTS_SHOTS as f32;
        casts.write(CastEffect {
            pay: Pay::Unpaid,
            ignites: holds(&players, Some(player), INCENDIARY_VOLLEY),
            ..duo_cast(player, Element::PhantomGun, CastTarget::Point(feet), shots, CONSCRIPTS)
        });
    }
}

/// A Firebrand pool under Live Fuse: whose, where, how wide, how long it has left, and the time to
/// its next Shock.
#[derive(Component, Debug)]
pub struct LiveFuse {
    pub player: Entity,
    pub at: Vec3,
    pub radius: f32,
    pub left: f32,
    pub next: f32,
}

/// Live Fuse: each pool Shocks one Zombie standing in it every `DUO_LIVE_FUSE_TICK_SECS` (Paid).
pub fn live_fuse(
    mut commands: Commands,
    mut fuses: Query<(Entity, &mut LiveFuse)>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut casts: MessageWriter<CastEffect>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let mut struck: Vec<Entity> = Vec::new();
    for (entity, mut fuse) in &mut fuses {
        fuse.left -= dt;
        if fuse.left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        fuse.next -= dt;
        if fuse.next > 0.0 {
            continue;
        }
        fuse.next = DUO_LIVE_FUSE_TICK_SECS;
        let target = zombies
            .iter()
            .filter(|(zombie, _)| !struck.contains(zombie))
            .find(|(_, at)| at.translation().with_y(0.0).distance(fuse.at.with_y(0.0)) <= fuse.radius);
        if let Some((zombie, _)) = target {
            struck.push(zombie);
            casts.write(duo_cast(fuse.player, Element::Shock, CastTarget::Zombie(zombie), 0.0, LIVE_FUSE));
        }
    }
}

/// A free frag in flight: whose, from where to where, and how long it has flown.
#[derive(Component, Debug)]
pub struct SupplyShell {
    player: Entity,
    from: Vec3,
    to: Vec3,
    elapsed: f32,
}

/// Where a free frag is `t` (0 to 1) of the way along its drawn arc.
pub fn shell_arc(from: Vec3, to: Vec3, t: f32) -> Vec3 {
    from.lerp(to, t) + Vec3::Y * DUO_SUPPLY_DROP_ARC_HEIGHT_M * 4.0 * t * (1.0 - t)
}

/// Supply Drop: a holder's Power-up pickup gives Lethals, and throws a free frag on the thickest
/// crowd in range.
pub fn supply_drop(
    mut commands: Commands,
    mut taken: MessageReader<PowerUpTaken>,
    players: Query<&HeldBoons, With<Player>>,
    transforms: Query<&GlobalTransform, With<Player>>,
    zombies: Query<&GlobalTransform, (With<Zombie>, Without<Dying>)>,
    mut lethals: MessageWriter<GiveLethals>,
) {
    for &PowerUpTaken(_, taker) in taken.read() {
        let Some(player) = taker.filter(|_| holds(&players, taker, SUPPLY_DROP)) else {
            continue;
        };
        lethals.write(GiveLethals { player, count: DUO_SUPPLY_DROP_LETHALS });
        let Ok(transform) = transforms.get(player) else {
            continue;
        };
        let crowd: Vec<Vec3> = zombies.iter().map(GlobalTransform::translation).collect();
        if let Some(to) = densest_point(feet(transform), &crowd) {
            let from = transform.translation();
            commands.spawn((InRun, Name::new("Supply Drop frag"), SupplyShell { player, from, to, elapsed: 0.0 }));
        }
    }
}

/// A free frag flies its drawn arc and goes off where it lands, never hurting a Player.
pub fn fly_supply_shells(
    mut commands: Commands,
    mut shells: Query<(Entity, &mut SupplyShell)>,
    players: Query<&Modifiers, With<Player>>,
    mut explosions: MessageWriter<Explosion>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    let colour = Family::Ember.colour();
    for (entity, mut shell) in &mut shells {
        shell.elapsed += time.delta_secs();
        let t = (shell.elapsed / DUO_SUPPLY_DROP_FLIGHT_SECS).min(1.0);
        gizmos.linestrip((0..=16).map(|i| shell_arc(shell.from, shell.to, t * i as f32 / 16.0)), colour);
        gizmos.sphere(Isometry3d::from_translation(shell_arc(shell.from, shell.to, t)), 0.12, colour);
        if t < 1.0 {
            continue;
        }
        let entries = players.get(shell.player).map_or(&[][..], |modifiers| &modifiers.0[..]);
        explosions.write(Explosion {
            at: shell.to,
            thrower: Some(shell.player),
            splash: equipment_splash(FRAG_SPLASH, entries),
            hurts_player: false,
            hurts_zombies: true,
            ammo_mods: AmmoMods::NONE,
            crawlers: makes_crawlers(Explosive::Frag),
        });
        commands.entity(entity).despawn();
    }
}

/// Ball Lightning: where each Zombie a holder's Parting Shot Blast killed this frame fell, with
/// whose Blast it was.
#[derive(Resource, Default)]
pub struct BallLightning(pub Vec<(Entity, Vec3)>);

/// Ball Lightning: once the Blast's kills have landed, a Shock chains on from where each fell, so
/// no chain picks a Zombie the Blast already killed.
pub fn ball_lightning(mut chains: ResMut<BallLightning>, mut casts: MessageWriter<CastEffect>) {
    let size = DUO_BALL_LIGHTNING_CHAIN_ZOMBIES as f32;
    for (player, at) in chains.0.drain(..) {
        casts.write(duo_cast(player, Element::Shock, CastTarget::Point(at), size, BALL_LIGHTNING));
    }
}

/// A Player took a Duo.
#[derive(Message, Clone, Copy)]
pub struct DuoTaken {
    pub player: Entity,
    pub duo: &'static BoonDef,
}

/// The Duos a Player has already been told they took.
#[derive(Component, Debug, Default)]
pub struct DuosTold(Vec<&'static str>);

/// The Duos newly held since `told`, noting them as told.
pub fn newly_taken(held: &HeldBoons, told: &mut DuosTold) -> Vec<&'static BoonDef> {
    let new: Vec<_> = held
        .0
        .iter()
        .map(|&(boon, _)| boon)
        .filter(|boon| boon.duo.is_some() && !told.0.contains(&boon.id))
        .collect();
    told.0.extend(new.iter().map(|duo| duo.id));
    new
}

/// Each Duo a Player takes is told once, and the `LocalPlayer` hears its stinger. Reading the held
/// Boons rather than the pick means a Co-op client's Player, whose Boons the Host tells, is told
/// too.
pub fn announce_duos(
    mut commands: Commands,
    mut players: Query<(Entity, &HeldBoons, Option<&mut DuosTold>, Has<LocalPlayer>), Changed<HeldBoons>>,
    mut taken: MessageWriter<DuoTaken>,
    mut cues: MessageWriter<PlayCue>,
) {
    for (player, held, told, local) in &mut players {
        let Some(mut told) = told else {
            let mut told = DuosTold::default();
            newly_taken(held, &mut told);
            commands.entity(player).insert(told);
            continue;
        };
        for duo in newly_taken(held, &mut told) {
            taken.write(DuoTaken { player, duo });
            if local {
                cues.write(PlayCue::new(Cue::DuoTaken, None));
            }
        }
    }
}

/// The Duo-taken banner: its two Family halves and its name, and the seconds it has left.
#[derive(Component)]
pub struct DuoBanner(f32);

#[derive(Component)]
pub struct DuoBannerHalf(usize);

#[derive(Component)]
pub struct DuoBannerText;

pub fn spawn_duo_banner(mut commands: Commands) {
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(22.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                DuoBanner(0.0),
                Node {
                    padding: UiRect::axes(px(28.0), px(8.0)),
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|banner| {
                for half in 0..2 {
                    banner.spawn((
                        DuoBannerHalf(half),
                        Node {
                            position_type: PositionType::Absolute,
                            left: percent(50.0 * half as f32),
                            width: percent(50.0),
                            height: percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ));
                }
                banner.spawn((DuoBannerText, text("", 34.0, Color::WHITE)));
            });
        });
}

/// A Duo the `LocalPlayer` takes shows its banner, split between its Families' colours, for
/// `GAUNTLET_SET_BANNER_SECS`.
pub fn show_duo_banner(
    mut taken: MessageReader<DuoTaken>,
    local: Query<(), With<LocalPlayer>>,
    banner: Single<(&mut DuoBanner, &mut Visibility)>,
    mut halves: Query<(&DuoBannerHalf, &mut BackgroundColor)>,
    mut words: Single<&mut Text, With<DuoBannerText>>,
    time: Res<Time<Real>>,
) {
    let (mut banner, mut visibility) = banner.into_inner();
    for taken in taken.read().filter(|taken| local.contains(taken.player)) {
        let Some(def) = taken.duo.duo else {
            continue;
        };
        words.0 = format!("DUO: {}", taken.duo.name.to_uppercase());
        for (half, mut colour) in &mut halves {
            colour.0 = def.families[half.0].colour().with_alpha(0.85);
        }
        banner.0 = GAUNTLET_SET_BANNER_SECS;
    }
    banner.0 -= time.delta_secs();
    visibility.set_if_neq(if banner.0 > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::gauntlet::boon::{BOONS, draw_offer};

    fn boon(id: &str) -> &'static BoonDef {
        find(id).unwrap_or_else(|| panic!("no Boon {id}"))
    }

    fn holding(ids: &[&str]) -> HeldBoons {
        let mut held = HeldBoons::default();
        for id in ids {
            held.add(boon(id));
        }
        held
    }

    fn everything() -> RunFacts {
        RunFacts { unlock_all: true, ..default() }
    }

    #[test]
    fn every_duo_from_appendix_a5_is_in_the_table_with_its_recipe() {
        let expected = [
            (GUN_AND_BLADE, ["marksmans_eye", "trench_raider"]),
            (BALL_LIGHTNING, ["parting_shot", "arc_light"]),
            (INCENDIARY_VOLLEY, ["firing_squad", "hot_brass"]),
            (GOLD_STANDARD, ["hollow_points", "blood_money"]),
            (CONSCRIPTS, ["full_belt", "turncoat"]),
            (STORM_SHELL, ["static_discharge", "live_wire"]),
            (THERMAL_SHOCK, ["frostbite", "wildfire"]),
            (CUT_OF_THE_TAKE, ["butchers_bill", "lucky_strike"]),
            (REPEL_BOARDERS, ["elbow_room", "barbed_wire"]),
            (LIVE_FUSE, ["short_circuit", "firebrand"]),
            (HOUSE_EDGE, ["black_market", "loaded_dice"]),
            (GALVANISED_DEAD, ["live_wire", "raise_the_dead"]),
            (SUPPLY_DROP, ["spoils_of_war", "bandolier"]),
            (MARTYRS, ["turncoat", "shrapnel"]),
            (LIFE_INSURANCE, ["war_bonds", "last_rites"]),
        ];
        assert_eq!(DUOS.len(), expected.len());
        for (id, recipe) in expected {
            let duo = boon(id);
            let def = duo.duo.expect("a Duo");
            assert_eq!(def.recipe, recipe, "{id}");
            assert_eq!(duo.rarity, Rarity::Legendary, "{id}");
            for (i, part) in recipe.iter().enumerate() {
                let part = boon(part);
                assert!(part.duo.is_none(), "{id}'s recipe is plain Boons");
                assert_eq!(def.families[i], part.family, "{id}: {} is {:?}", part.id, part.family);
            }
        }
    }

    #[test]
    fn there_is_one_duo_for_each_pair_of_families() {
        let mut pairs: Vec<_> = DUOS
            .iter()
            .map(|duo| {
                let [a, b] = duo.duo.unwrap().families;
                assert_ne!(a, b, "{}", duo.id);
                ((a as usize).min(b as usize), (a as usize).max(b as usize))
            })
            .collect();
        pairs.sort();
        pairs.dedup();
        assert_eq!(pairs.len(), 15, "every pair of the six Families, once");
    }

    #[test]
    fn no_boon_is_in_more_than_two_recipes() {
        for plain in BOONS {
            let count = DUOS.iter().filter(|duo| partner(duo, plain.id).is_some()).count();
            assert!(count <= 2, "{} is in {count} recipes", plain.id);
        }
    }

    #[test]
    fn duo_ids_and_names_never_clash_with_a_boons() {
        for duo in DUOS {
            assert!(BOONS.iter().all(|plain| plain.id != duo.id && plain.name != duo.name), "{}", duo.id);
        }
    }

    #[test]
    fn a_duo_is_eligible_once_both_its_recipe_boons_are_held() {
        let duo = boon(GOLD_STANDARD);
        let run = everything();
        assert!(!duo_eligible(duo, &holding(&[]), &run));
        assert!(!duo_eligible(duo, &holding(&["hollow_points"]), &run));
        assert!(duo_eligible(duo, &holding(&["hollow_points", "blood_money"]), &run));
        assert!(!duo_eligible(duo, &holding(&["hollow_points", "blood_money", GOLD_STANDARD]), &run), "held once");
    }

    #[test]
    fn a_duo_needs_both_recipe_boons_unlocked_and_has_no_price_of_its_own() {
        let duo = boon(BALL_LIGHTNING);
        let held = holding(&["parting_shot", "arc_light"]);
        let one = RunFacts { unlocked: vec!["parting_shot".into()], ..default() };
        assert!(!duo_eligible(duo, &held, &one));
        let both = RunFacts { unlocked: vec!["parting_shot".into(), "arc_light".into()], ..default() };
        assert!(duo_eligible(duo, &held, &both), "the Duo's own id never needs buying");
        // A recipe of starting Boons is in the pool from the first launch.
        let starters = holding(&["hollow_points", "blood_money"]);
        assert!(duo_eligible(boon(GOLD_STANDARD), &starters, &RunFacts::default()));
    }

    #[test]
    fn the_duo_pool_holds_every_eligible_duo_and_nothing_else() {
        let held = holding(&["turncoat", "full_belt", "shrapnel", "hollow_points"]);
        let ids: Vec<_> = duo_pool(DUOS, &held, &everything()).iter().map(|duo| duo.id).collect();
        assert_eq!(ids, [CONSCRIPTS, MARTYRS]);
    }

    #[test]
    fn an_eligible_duo_turns_up_on_offers_as_a_legendary() {
        let held = holding(&["hollow_points", "blood_money"]);
        let run = everything();
        let mut seen = 0;
        for i in 0..200 {
            let mut state = i as f64 / 200.0;
            let offer = draw_offer(BOONS, &held, &run, |total| {
                state = (state * 7.31 + 0.137).fract();
                state * total
            });
            seen += offer.iter().filter(|card| card.id == GOLD_STANDARD).count();
        }
        assert!(seen > 0, "Gold Standard never came up");
        let never = holding(&["hollow_points"]);
        for _ in 0..50 {
            let offer = draw_offer(BOONS, &never, &run, |total| rand::random_range(0.0..total));
            assert!(offer.iter().all(|card| card.duo.is_none()), "a Duo came up without its recipe");
        }
    }

    #[test]
    fn duos_count_toward_neither_family() {
        let held = holding(&["hollow_points", "blood_money", GOLD_STANDARD]);
        assert_eq!(held.in_family(Family::Lead), 1);
        assert_eq!(held.in_family(Family::Gold), 1);
    }

    #[test]
    fn a_pick_that_completes_a_recipe_is_duo_ready() {
        let held = holding(&["turncoat"]);
        let ready: Vec<_> = duo_ready(boon("shrapnel"), &held).iter().map(|duo| duo.id).collect();
        assert_eq!(ready, [MARTYRS]);
        assert!(duo_ready(boon("turncoat"), &held).is_empty(), "already held");
        assert!(duo_ready(boon("hollow_points"), &held).is_empty(), "in no recipe with Turncoat");
        let done = holding(&["turncoat", MARTYRS]);
        assert!(duo_ready(boon("shrapnel"), &done).is_empty(), "the Duo is held already");
    }

    #[test]
    fn a_held_recipe_boon_names_the_partner_it_waits_for() {
        let held = holding(&["turncoat"]);
        assert_eq!(partners_wanted(boon("turncoat"), &held), [("Full Belt", "Conscripts"), ("Shrapnel", "Martyrs")]);
        let paired = holding(&["turncoat", "shrapnel"]);
        assert_eq!(partners_wanted(boon("turncoat"), &paired), [("Full Belt", "Conscripts")]);
        assert!(partners_wanted(boon("shrapnel"), &held).is_empty(), "not held");
    }

    #[test]
    fn gold_standard_pays_blood_money_twice_on_a_headshot() {
        assert_eq!(gold_standard_points(20, true, true), 40);
        assert_eq!(gold_standard_points(20, false, true), 20);
        assert_eq!(gold_standard_points(20, true, false), 20);
        assert_eq!(gold_standard_points(0, true, true), 0, "an Unpaid kill stays unpaid");
    }

    #[test]
    fn storm_shell_fires_on_a_crowd_of_five_and_halves_the_cooldown() {
        assert!(!storm_shell_crowded(4));
        assert!(storm_shell_crowded(5));
        assert_eq!(storm_shell_cooldown(30.0, true), 15.0);
        assert_eq!(storm_shell_cooldown(30.0, false), 30.0);
    }

    #[test]
    fn galvanised_dead_answers_for_one_ally_at_most_every_two_seconds() {
        assert!(galvanised_ready(None, 0.0));
        assert!(!galvanised_ready(Some(1.0), 2.9));
        assert!(galvanised_ready(Some(1.0), 3.0));
    }

    #[test]
    fn repel_boarders_takes_a_tenth_of_max_health() {
        assert!((repel_boarders_damage(500.0) - 50.0).abs() < 1e-4);
    }

    #[test]
    fn supply_drop_lands_on_the_thickest_crowd_in_range() {
        let from = Vec3::ZERO;
        let crowd = Vec3::new(6.0, 0.0, 0.0);
        let zombies = [Vec3::new(-6.0, 0.0, 0.0), crowd, crowd + Vec3::X * 0.5, crowd - Vec3::Z * 0.5, Vec3::new(30.0, 0.0, 0.0)];
        let landed = densest_point(from, &zombies).unwrap();
        assert!(landed.distance(crowd) < 1.0, "{landed}");
        assert_eq!(densest_point(from, &[Vec3::new(30.0, 0.0, 0.0)]), None, "nothing in range");
    }

    #[test]
    fn a_duo_taken_is_told_once() {
        let mut told = DuosTold::default();
        let mut held = holding(&["hollow_points", "blood_money"]);
        assert!(newly_taken(&held, &mut told).is_empty(), "no Duo yet");
        held.add(boon(GOLD_STANDARD));
        let new: Vec<_> = newly_taken(&held, &mut told).iter().map(|duo| duo.id).collect();
        assert_eq!(new, [GOLD_STANDARD]);
        held.add(boon("hollow_points"));
        assert!(newly_taken(&held, &mut told).is_empty(), "a later pick never tells it again");
    }

    #[test]
    fn life_insurance_pays_war_bonds_round_end_points() {
        assert_eq!(life_insurance_points(7, 2), BOON_WAR_BONDS_POINTS_PER_ROUND * 7 * 2);
        assert_eq!(life_insurance_points(7, 0), 0);
    }
}
