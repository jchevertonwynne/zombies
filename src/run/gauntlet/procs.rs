//! Proc Boons: a Trigger the Player does, and an Effect that may fire and is seen working. Each
//! Proc Boon's Trigger chance and cooldown are fixed; its Stacks grow only the Effect. The Effects
//! borrow the Ammo Mod elements through Classic's `CastEffect` (ADR 0007), plus Gauntlet's own
//! Frost and Blast. A Proc's damage is `Attack::AmmoMod`, so it never rolls an Ammo Mod, never
//! fires another Proc and never makes a Crawler.

use bevy::prelude::*;

use super::boon::{BoonDef, Family, HeldBoons};
use super::duos::{
    BALL_LIGHTNING, BallLightning, INCENDIARY_VOLLEY, LIVE_FUSE, LiveFuse, REPEL_BOARDERS, STORM_SHELL,
    repel_boarders_damage, storm_shell_cooldown, storm_shell_crowded,
};
use super::pact::{HeldPacts, OVERCLOCKED};
use crate::maps::barrier::FlingOutside;
use crate::modifier::Modifiers;
use crate::state::InRun;
use crate::weapons::ammo_mod::fire_radius;
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::economy::power_up::PowerUpTaken;
use crate::enemies::zombie::{Heavyweight, 
    Attack, DamageZombie, Dying, HitZone, Pay, Zombie, ZombieKilled, ZombieMoveFactor,
};
use crate::modifier::Source;
use crate::players::player::{Player, PlayerHit, Seat};
use crate::tuning::*;
use crate::weapons::ammo_mod::{AmmoMod, CastEffect, CastTarget, Element, OutsideAllyCap, TurnedAlly, TurnedAllyCap};
use crate::weapons::equipment::LethalThrown;
use crate::weapons::weapon::{MagazineEmptied, Reloaded};

/// What the Player does that may fire a Proc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// A Player kill that's a Headshot.
    HeadshotKill,
    /// A Player kill with the Knife.
    KnifeKill,
    /// Any Player kill.
    AnyKill,
    /// The Player takes a hit.
    Hit,
    /// A Reload finishes.
    Reload,
    /// A shot fires the last bullet in the Magazine.
    MagazineEmptied,
    /// A Lethal leaves the hand.
    LethalThrown,
    /// The Player picks up a Power-up.
    PowerUp,
    /// A crowd of Zombies around the Player, under Storm Shell: it fires Static Discharge alone.
    Crowded,
}

/// What a Proc does, sized by its Stacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcEffect {
    /// A Shock chain from the kill to `size` Zombies.
    ShockChain,
    /// Shock kills every Zombie within `size` metres of the Player.
    Discharge,
    /// A fire ring sets alight every Zombie within `size` metres of the Player.
    FireRing,
    /// The kill's corpse bursts, setting alight every Zombie within `size` metres.
    FireBurst,
    /// A fire pool of `size` metres where the Lethal lands.
    FirePool,
    /// Knockback kills and flings the `size` closest Zombies around the Player.
    Knockback,
    /// That shot's hit Blasts, killing every Zombie within `size` metres.
    Blast,
    /// Frost slows every Zombie near the kill for `size` seconds.
    Frost,
    /// A phantom gun at the Player's feet fires `size` shots.
    PhantomGun,
    /// The nearest Zombie to the Player is Turned for `size` seconds.
    TurnNearest,
    /// `size` Zombies nearest the kill are Turned.
    Raise,
}

impl ProcEffect {
    /// Whether its kills pay Points: the phantom gun and Turned don't.
    pub fn pay(self) -> Pay {
        match self {
            ProcEffect::PhantomGun | ProcEffect::TurnNearest | ProcEffect::Raise => Pay::Unpaid,
            _ => Pay::Paid,
        }
    }

    /// Whether it turns Zombies, so the Turned ally cap holds it back.
    pub fn turns(self) -> bool {
        matches!(self, ProcEffect::TurnNearest | ProcEffect::Raise)
    }

    /// The cue it plays, and whether that's an Ammo Mod's borrowed one (played higher and
    /// quieter) or one of its own.
    pub fn cue(self) -> (Cue, bool) {
        match self {
            ProcEffect::ShockChain | ProcEffect::Discharge => (AmmoMod::DeadWire.cue(), true),
            ProcEffect::FireRing | ProcEffect::FireBurst | ProcEffect::FirePool => (AmmoMod::BlastFurnace.cue(), true),
            ProcEffect::Knockback => (AmmoMod::ThunderWall.cue(), true),
            ProcEffect::PhantomGun => (AmmoMod::Fireworks.cue(), true),
            ProcEffect::TurnNearest | ProcEffect::Raise => (AmmoMod::Turned.cue(), true),
            ProcEffect::Blast => (Cue::ProcBlast, false),
            ProcEffect::Frost => (Cue::ProcFrost, false),
        }
    }
}

/// A Proc Boon's Trigger and Effect.
#[derive(Debug, PartialEq)]
pub struct ProcDef {
    pub trigger: Trigger,
    /// Its chance to fire on each Trigger, whatever its Stacks.
    pub chance: f64,
    /// Seconds before it can fire again, before Live Wire and Overclocked.
    pub cooldown_secs: f32,
    pub effect: ProcEffect,
    /// The Effect's size at 1 Stack, and what each extra Stack adds.
    pub size: f32,
    pub size_per_stack: f32,
    pub family: Family,
}

/// Always fires when ready.
const ALWAYS: f64 = 1.0;

/// The Proc Boons a Duo changes, by stable ID.
pub const STATIC_DISCHARGE_ID: &str = "static_discharge";
pub const PARTING_SHOT_ID: &str = "parting_shot";

pub const ARC_LIGHT: ProcDef = ProcDef {
    trigger: Trigger::HeadshotKill,
    chance: ALWAYS,
    cooldown_secs: BOON_ARC_LIGHT_COOLDOWN_SECS,
    effect: ProcEffect::ShockChain,
    size: BOON_ARC_LIGHT_CHAIN_ZOMBIES as f32,
    size_per_stack: BOON_ARC_LIGHT_CHAIN_ZOMBIES_PER_STACK as f32,
    family: Family::Spark,
};
pub const HOT_BRASS: ProcDef = ProcDef {
    trigger: Trigger::Reload,
    chance: ALWAYS,
    cooldown_secs: BOON_HOT_BRASS_COOLDOWN_SECS,
    effect: ProcEffect::FireRing,
    size: BOON_HOT_BRASS_RADIUS_M,
    size_per_stack: BOON_HOT_BRASS_RADIUS_M_PER_STACK,
    family: Family::Ember,
};
pub const ELBOW_ROOM: ProcDef = ProcDef {
    trigger: Trigger::Hit,
    chance: ALWAYS,
    cooldown_secs: BOON_ELBOW_ROOM_COOLDOWN_SECS,
    effect: ProcEffect::Knockback,
    size: BOON_ELBOW_ROOM_ZOMBIES as f32,
    size_per_stack: BOON_ELBOW_ROOM_ZOMBIES_PER_STACK as f32,
    family: Family::Blade,
};
pub const PARTING_SHOT: ProcDef = ProcDef {
    trigger: Trigger::MagazineEmptied,
    chance: ALWAYS,
    cooldown_secs: 0.0,
    effect: ProcEffect::Blast,
    size: BOON_PARTING_SHOT_RADIUS_M,
    size_per_stack: BOON_PARTING_SHOT_RADIUS_M_PER_STACK,
    family: Family::Lead,
};
pub const FROSTBITE: ProcDef = ProcDef {
    trigger: Trigger::KnifeKill,
    chance: ALWAYS,
    cooldown_secs: 0.0,
    effect: ProcEffect::Frost,
    size: BOON_FROSTBITE_SECS,
    size_per_stack: BOON_FROSTBITE_SECS_PER_STACK,
    family: Family::Blade,
};
pub const SPOILS_OF_WAR: ProcDef = ProcDef {
    trigger: Trigger::PowerUp,
    chance: ALWAYS,
    cooldown_secs: 0.0,
    effect: ProcEffect::PhantomGun,
    size: BOON_SPOILS_OF_WAR_SHOTS as f32,
    size_per_stack: BOON_SPOILS_OF_WAR_SHOTS_PER_STACK as f32,
    family: Family::Gold,
};
pub const TURNCOAT: ProcDef = ProcDef {
    trigger: Trigger::AnyKill,
    chance: BOON_TURNCOAT_CHANCE,
    cooldown_secs: BOON_TURNCOAT_COOLDOWN_SECS,
    effect: ProcEffect::TurnNearest,
    size: BOON_TURNCOAT_SECS,
    size_per_stack: BOON_TURNCOAT_SECS_PER_STACK,
    family: Family::Grit,
};
pub const FIREBRAND: ProcDef = ProcDef {
    trigger: Trigger::LethalThrown,
    chance: ALWAYS,
    cooldown_secs: 0.0,
    effect: ProcEffect::FirePool,
    size: BOON_FIREBRAND_RADIUS_M,
    size_per_stack: BOON_FIREBRAND_RADIUS_M_PER_STACK,
    family: Family::Ember,
};
pub const STATIC_DISCHARGE: ProcDef = ProcDef {
    trigger: Trigger::Hit,
    chance: ALWAYS,
    cooldown_secs: BOON_STATIC_DISCHARGE_COOLDOWN_SECS,
    effect: ProcEffect::Discharge,
    size: BOON_STATIC_DISCHARGE_RADIUS_M,
    size_per_stack: BOON_STATIC_DISCHARGE_RADIUS_M_PER_STACK,
    family: Family::Blade,
};
pub const WILDFIRE: ProcDef = ProcDef {
    trigger: Trigger::AnyKill,
    chance: BOON_WILDFIRE_CHANCE,
    cooldown_secs: BOON_WILDFIRE_COOLDOWN_SECS,
    effect: ProcEffect::FireBurst,
    size: BOON_WILDFIRE_RADIUS_M,
    size_per_stack: BOON_WILDFIRE_RADIUS_M_PER_STACK,
    family: Family::Ember,
};
pub const FIRING_SQUAD: ProcDef = ProcDef {
    trigger: Trigger::Reload,
    chance: ALWAYS,
    cooldown_secs: BOON_FIRING_SQUAD_COOLDOWN_SECS,
    effect: ProcEffect::PhantomGun,
    size: BOON_FIRING_SQUAD_SHOTS as f32,
    size_per_stack: BOON_FIRING_SQUAD_SHOTS_PER_STACK as f32,
    family: Family::Lead,
};
pub const RAISE_THE_DEAD: ProcDef = ProcDef {
    trigger: Trigger::HeadshotKill,
    chance: BOON_RAISE_THE_DEAD_CHANCE,
    cooldown_secs: BOON_RAISE_THE_DEAD_COOLDOWN_SECS,
    effect: ProcEffect::Raise,
    size: BOON_RAISE_THE_DEAD_ZOMBIES as f32,
    size_per_stack: BOON_RAISE_THE_DEAD_ZOMBIES_PER_STACK as f32,
    family: Family::Grit,
};

/// A Proc's Effect size at `stacks` Stacks: its size at 1, plus its growth for each one after.
pub fn proc_size(def: &ProcDef, stacks: u32) -> f32 {
    def.size + def.size_per_stack * stacks.saturating_sub(1) as f32
}

/// A Proc's cooldown for its holder: ×`BOON_LIVE_WIRE_COOLDOWN_FACTOR` per Live Wire Stack, and
/// none at all under Overclocked.
pub fn proc_cooldown(base: f32, live_wire_stacks: u32, overclocked: bool) -> f32 {
    if overclocked {
        0.0
    } else {
        base * BOON_LIVE_WIRE_COOLDOWN_FACTOR.powi(live_wire_stacks.min(i32::MAX as u32) as i32)
    }
}

/// Whether a Proc fires on its Trigger: ready, and a `roll` in [0, 1) under its chance.
pub fn proc_fires(def: &ProcDef, ready: bool, roll: f64) -> bool {
    ready && roll < def.chance
}

/// Whether a Proc fires, where a turning one also needs `turn_room` under the Turned ally cap. A
/// Proc that doesn't fire starts no cooldown.
pub fn proc_may_fire(def: &ProcDef, ready: bool, turn_room: bool, roll: f64) -> bool {
    (turn_room || !def.effect.turns()) && proc_fires(def, ready, roll)
}

/// The kill Triggers a kill sets off. Only a Player's own kills count: a Bullet (Weapons and
/// Lethals) or the Knife. Ammo Mod, Proc, burn, Trap and every other kill sets off none.
pub fn kill_triggers(attack: Attack, headshot: bool) -> Vec<Trigger> {
    let mut triggers = Vec::new();
    if !matches!(attack, Attack::Bullet | Attack::Knife) {
        return triggers;
    }
    if headshot {
        triggers.push(Trigger::HeadshotKill);
    }
    if attack == Attack::Knife {
        triggers.push(Trigger::KnifeKill);
    }
    triggers.push(Trigger::AnyKill);
    triggers
}

/// The Proc Boon `id`'s Trigger and Effect, if it's a Proc Boon.
#[cfg(test)]
fn proc_of(id: &str) -> Option<&'static ProcDef> {
    super::boon::BOONS.iter().find(|boon| boon.id == id)?.proc
}

/// Seconds left on each Proc Boon's cooldown for one Player: (Boon ID, left, the cooldown it
/// started at).
#[derive(Component, Debug, Default, Clone, PartialEq)]
pub struct ProcCooldowns(Vec<(&'static str, f32, f32)>);

impl ProcCooldowns {
    pub fn ready(&self, id: &str) -> bool {
        self.0.iter().all(|&(held, left, _)| held != id || left <= 0.0)
    }

    pub fn start(&mut self, id: &'static str, secs: f32) {
        self.0.retain(|&(held, ..)| held != id);
        if secs > 0.0 {
            self.0.push((id, secs, secs));
        }
    }

    pub fn tick(&mut self, dt: f32) {
        for (_, left, _) in &mut self.0 {
            *left = (*left - dt).max(0.0);
        }
        self.0.retain(|&(_, left, _)| left > 0.0);
    }

    /// How far its cooldown has refilled, from 0 just fired to 1 ready.
    pub fn filled(&self, id: &str) -> f32 {
        self.0
            .iter()
            .find(|&&(held, ..)| held == id)
            .map_or(1.0, |&(_, left, total)| 1.0 - left / total)
    }
}

/// Plays of one cue thinned on Gauntlet's side: never two within `GAUNTLET_CUE_MIN_GAP_SECS`, and
/// never more than `GAUNTLET_CUE_MAX_VOICES` playing at once.
#[derive(Debug, Default, Clone)]
pub struct CueGate {
    /// When each play still sounding started.
    voices: Vec<f32>,
}

impl CueGate {
    /// Whether the cue may play at `now`, counting the play if so.
    pub fn allow(&mut self, now: f32) -> bool {
        self.voices.retain(|&start| now - start < GAUNTLET_CUE_VOICE_SECS);
        let gap = self.voices.last().is_none_or(|&last| now - last >= GAUNTLET_CUE_MIN_GAP_SECS);
        if !gap || self.voices.len() >= GAUNTLET_CUE_MAX_VOICES {
            return false;
        }
        self.voices.push(now);
        true
    }
}

/// Proc visuals and accent rings alive, thinned so a crowd of Procs doesn't fill the screen. A
/// visual counts as alive for `GAUNTLET_PROC_ACCENT_SECS`.
#[derive(Debug, Default, Clone)]
pub struct VisualGate {
    shown: Vec<f32>,
    accents: Vec<f32>,
}

impl VisualGate {
    /// Whether a Proc cast at `now` shows, and whether it gets its accent ring, counting both.
    pub fn allow(&mut self, now: f32) -> (bool, bool) {
        let alive = |start: &f32| now - start < GAUNTLET_PROC_ACCENT_SECS;
        self.shown.retain(alive);
        self.accents.retain(alive);
        let show = self.shown.len() < GAUNTLET_PROC_FLASH_CAP;
        if !show {
            return (false, false);
        }
        self.shown.push(now);
        let accent = self.accents.len() < GAUNTLET_ACCENT_RING_CAP;
        if accent {
            self.accents.push(now);
        }
        (true, accent)
    }
}

/// Gauntlet's thinning of its own Proc cues and visuals. Classic's cues are never thinned.
#[derive(Resource, Default)]
pub struct Thinning {
    cues: Vec<(Cue, CueGate)>,
    visuals: VisualGate,
}

impl Thinning {
    fn cue(&mut self, cue: Cue, now: f32) -> bool {
        let at = match self.cues.iter().position(|(held, _)| *held == cue) {
            Some(at) => at,
            None => {
                self.cues.push((cue, CueGate::default()));
                self.cues.len() - 1
            }
        };
        self.cues[at].1.allow(now)
    }
}

/// A Proc that fired: whose, which Boon's, how big, and where.
#[derive(Message, Clone, Copy)]
pub struct ProcFired {
    pub player: Entity,
    pub boon: &'static BoonDef,
    pub size: f32,
    /// Where it happens: the kill, the shot's hit or the Player's feet.
    pub at: Vec3,
    /// The Lethal in flight, for an Effect that waits for it to land.
    pub projectile: Option<Entity>,
}

/// A Zombie slowed by Frost for `left` more seconds.
#[derive(Component, Debug)]
pub struct Frosted {
    left: f32,
}

/// The source Frost's factor goes on a Zombie's `ZombieMoveFactor` under.
const FROST: Source = Source("Frost");
/// Frost's pale blue-white.
pub const FROST_COLOUR: Color = Color::srgb(0.7, 0.9, 1.0);
/// A Blast's orange-white.
const BLAST_COLOUR: Color = Color::srgb(1.0, 0.8, 0.55);

/// Gauntlet-drawn Proc visuals: Blast spheres, Frost bursts and their accent rings, each with
/// (where, radius, colour, seconds left, a sphere rather than a ring).
#[derive(Resource, Default)]
pub struct ProcFlashes(Vec<(Vec3, f32, Color, f32, bool)>);

impl ProcFlashes {
    /// A Blast's sphere of `radius` at `at`.
    pub fn blast(&mut self, at: Vec3, radius: f32) {
        self.0.push((at, radius, BLAST_COLOUR, GAUNTLET_PROC_ACCENT_SECS, true));
    }
}

/// Firebrand pools waiting on their Lethal to land: (frag, where it was last seen, the Proc).
#[derive(Resource, Default)]
pub struct Landing(Vec<(Entity, Vec3, ProcFired)>);

/// Frost on one Zombie for at least `secs`: its move factor under Frost's source, and how long
/// it lasts.
pub fn frost(
    commands: &mut Commands,
    zombie: Entity,
    factor: Option<Mut<ZombieMoveFactor>>,
    frosted: Option<Mut<Frosted>>,
    secs: f32,
) {
    match factor {
        Some(mut factor) => factor.set(FROST, GAUNTLET_FROST_SPEED_FACTOR),
        None => {
            let mut factor = ZombieMoveFactor::default();
            factor.set(FROST, GAUNTLET_FROST_SPEED_FACTOR);
            commands.entity(zombie).insert(factor);
        }
    }
    match frosted {
        Some(mut frosted) => frosted.left = frosted.left.max(secs),
        None => {
            commands.entity(zombie).insert(Frosted { left: secs });
        }
    }
}

/// Where a Player's feet are.
pub fn feet(player: &GlobalTransform) -> Vec3 {
    player.translation() - Vec3::Y * (PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0)
}

/// Every Trigger, read from Classic's messages, fires the held Proc Boons it sets off that are
/// ready and roll under their chance; each starts its cooldown. A turning Proc at the Turned ally
/// cap doesn't fire and starts no cooldown.
#[allow(clippy::too_many_arguments)]
pub fn fire_procs(
    mut kills: MessageReader<ZombieKilled>,
    mut hits: MessageReader<PlayerHit>,
    mut reloads: MessageReader<Reloaded>,
    mut emptied: MessageReader<MagazineEmptied>,
    mut lethals: MessageReader<LethalThrown>,
    mut taken: MessageReader<PowerUpTaken>,
    mut players: Query<(Entity, &Seat, &HeldBoons, &mut ProcCooldowns, &GlobalTransform), With<Player>>,
    zombies: Query<&GlobalTransform, (With<Zombie>, Without<Dying>)>,
    pacts: Res<HeldPacts>,
    (cap, allies): (Res<TurnedAllyCap>, Query<(), (With<TurnedAlly>, Without<OutsideAllyCap>)>),
    mut fired: MessageWriter<ProcFired>,
    time: Res<Time>,
) {
    for (.., mut cooldowns, _) in &mut players {
        cooldowns.tick(time.delta_secs());
    }
    // (Player, Trigger, where, the Lethal in flight). `None` for where means the Player's feet.
    let mut triggered: Vec<(Entity, Trigger, Option<Vec3>, Option<Entity>)> = Vec::new();
    for kill in kills.read() {
        if let Some(player) = kill.player {
            for trigger in kill_triggers(kill.attack, kill.headshot) {
                triggered.push((player, trigger, Some(kill.at), None));
            }
        }
    }
    triggered.extend(hits.read().map(|hit| (hit.player, Trigger::Hit, None, None)));
    triggered.extend(reloads.read().map(|reload| (reload.player, Trigger::Reload, None, None)));
    triggered.extend(
        emptied
            .read()
            .filter_map(|emptied| Some((emptied.player, Trigger::MagazineEmptied, Some(emptied.at?), None))),
    );
    triggered.extend(
        lethals
            .read()
            .map(|lethal| (lethal.player, Trigger::LethalThrown, Some(lethal.at), Some(lethal.projectile))),
    );
    triggered.extend(taken.read().filter_map(|taken| Some((taken.1?, Trigger::PowerUp, None, None))));
    // Storm Shell: a crowd close around a holder of Static Discharge sets it off too.
    for (player, _, held, cooldowns, transform) in &players {
        if held.stacks(STORM_SHELL) == 0 || held.stacks(STATIC_DISCHARGE_ID) == 0 || !cooldowns.ready(STATIC_DISCHARGE_ID) {
            continue;
        }
        let at = transform.translation();
        let near = zombies.iter().filter(|zombie| zombie.translation().distance(at) <= DUO_STORM_SHELL_RADIUS_M).count();
        if storm_shell_crowded(near) {
            triggered.push((player, Trigger::Crowded, None, None));
        }
    }

    let mut allies = allies.iter().count();
    for (player, trigger, at, projectile) in triggered {
        let Ok((_, &seat, held, mut cooldowns, transform)) = players.get_mut(player) else {
            continue;
        };
        let live_wire = held.stacks(super::boon::LIVE_WIRE);
        let overclocked = pacts.holder(OVERCLOCKED.id) == Some(seat);
        let storm_shell = held.stacks(STORM_SHELL) > 0;
        for &(boon, stacks) in &held.0 {
            let crowded = trigger == Trigger::Crowded && boon.id == STATIC_DISCHARGE_ID;
            let Some(def) = boon.proc.filter(|def| def.trigger == trigger || crowded) else {
                continue;
            };
            if !proc_may_fire(def, cooldowns.ready(boon.id), cap.has_room(allies), rand::random()) {
                continue;
            }
            let cooldown = proc_cooldown(def.cooldown_secs, live_wire, overclocked);
            let cooldown = storm_shell_cooldown(cooldown, storm_shell && boon.id == STATIC_DISCHARGE_ID);
            cooldowns.start(boon.id, cooldown);
            if def.effect.turns() {
                allies += 1;
            }
            fired.write(ProcFired {
                player,
                boon,
                size: proc_size(def, stacks),
                at: at.unwrap_or_else(|| feet(transform)),
                projectile,
            });
        }
    }
}

/// Each fired Proc takes effect: through `CastEffect` for an Ammo Mod element, or Gauntlet's own
/// Frost and Blast. Its cue plays higher and quieter for a borrowed one, and thinning may drop
/// the cue, the visual or the accent ring, never the Effect.
#[allow(clippy::too_many_arguments)]
pub fn cast_procs(
    mut commands: Commands,
    mut fired: MessageReader<ProcFired>,
    mut zombies: Query<
        (Entity, &GlobalTransform, &Zombie, Option<&mut ZombieMoveFactor>, Option<&mut Frosted>, Has<Heavyweight>),
        Without<Dying>,
    >,
    players: Query<(&GlobalTransform, &HeldBoons, &Modifiers), With<Player>>,
    mut thinning: ResMut<Thinning>,
    mut flashes: ResMut<ProcFlashes>,
    mut landing: ResMut<Landing>,
    mut chains: ResMut<BallLightning>,
    mut casts: MessageWriter<CastEffect>,
    (mut damage, mut flings): (MessageWriter<DamageZombie>, MessageWriter<FlingOutside>),
    mut cues: MessageWriter<PlayCue>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed_secs();
    for &proc in fired.read() {
        let Some(def) = proc.boon.proc else {
            continue;
        };
        let (holder, entries) = match players.get(proc.player) {
            Ok((_, held, modifiers)) => (Some(held), &modifiers.0[..]),
            Err(_) => (None, &[][..]),
        };
        let holds = |id: &str| holder.is_some_and(|held| held.stacks(id) > 0);
        // A Firebrand pool waits for its Lethal to land.
        if let Some(projectile) = proc.projectile {
            landing.0.push((projectile, proc.at, ProcFired { projectile: None, ..proc }));
            continue;
        }
        let (show, accent) = thinning.visuals.allow(now);
        let accent = accent.then(|| def.family.colour());
        let centre = players.get(proc.player).map_or(proc.at, |(at, ..)| at.translation());
        let cast = |element, target, size, reach, lasts| CastEffect {
            player: proc.player,
            element,
            target,
            size,
            reach,
            lasts,
            pay: def.effect.pay(),
            accent,
            show,
            unbounded: false,
            tint: None,
            ignites: false,
        };
        let point = CastTarget::Point(proc.at);
        match def.effect {
            ProcEffect::ShockChain => {
                casts.write(cast(Element::Shock, point, proc.size, None, 0.0));
            }
            ProcEffect::Discharge => {
                casts.write(cast(Element::Shock, CastTarget::Point(centre), f32::INFINITY, Some(proc.size), 0.0));
            }
            ProcEffect::FireRing | ProcEffect::FireBurst => {
                casts.write(cast(Element::Fire, point, proc.size, None, 0.0));
            }
            ProcEffect::FirePool => {
                casts.write(cast(Element::Fire, point, proc.size, None, BOON_FIREBRAND_POOL_SECS));
                // Live Fuse: the pool Shocks a Zombie in it every so often.
                if holds(LIVE_FUSE) {
                    commands.spawn((
                        InRun,
                        Name::new("Live Fuse"),
                        LiveFuse {
                            player: proc.player,
                            at: proc.at,
                            radius: fire_radius(proc.size, entries),
                            left: BOON_FIREBRAND_POOL_SECS,
                            next: DUO_LIVE_FUSE_TICK_SECS,
                        },
                    ));
                }
            }
            // Repel Boarders: the Zombies Elbow Room would fling are thrown back outside instead,
            // hurt but alive. Never a Heavyweight.
            ProcEffect::Knockback if holds(REPEL_BOARDERS) => {
                let mut near: Vec<_> = zombies
                    .iter()
                    .filter(|(.., heavyweight)| !heavyweight)
                    .map(|(zombie, at, body, ..)| (zombie, at.translation().distance(centre), body.max_health()))
                    .filter(|&(_, distance, _)| distance <= BOON_ELBOW_ROOM_RADIUS_M)
                    .collect();
                near.sort_by(|a, b| a.1.total_cmp(&b.1));
                for &(zombie, _, max_health) in near.iter().take(proc.size.max(0.0) as usize) {
                    damage.write(DamageZombie {
                        player: Some(proc.player),
                        zombie,
                        damage: repel_boarders_damage(max_health),
                        zone: HitZone::Torso,
                        attack: Attack::AmmoMod(Pay::Paid),
                    });
                    flings.write(FlingOutside { zombie });
                }
                if let Some(colour) = accent {
                    flashes.0.push((centre, BOON_ELBOW_ROOM_RADIUS_M, colour, GAUNTLET_PROC_ACCENT_SECS, false));
                }
            }
            ProcEffect::Knockback => {
                casts.write(cast(
                    Element::Knockback,
                    CastTarget::Point(centre),
                    proc.size,
                    Some(BOON_ELBOW_ROOM_RADIUS_M),
                    0.0,
                ));
            }
            ProcEffect::PhantomGun => {
                // Incendiary Volley: its shots set Zombies alight.
                casts.write(CastEffect {
                    ignites: holds(INCENDIARY_VOLLEY),
                    ..cast(Element::PhantomGun, point, proc.size, None, 0.0)
                });
            }
            ProcEffect::TurnNearest => {
                casts.write(cast(Element::Turned, CastTarget::Point(centre), proc.size, None, 0.0));
            }
            ProcEffect::Raise => {
                // Each turn takes the next nearest Zombie to the kill, all within the cap.
                for _ in 0..proc.size.max(0.0) as u32 {
                    casts.write(cast(Element::Turned, point, TURNED_LIFETIME_SECS, None, 0.0));
                }
            }
            ProcEffect::Blast => {
                // Ball Lightning: each Zombie a Parting Shot Blast kills chains Shock on.
                let ball_lightning = proc.boon.id == PARTING_SHOT_ID && holds(BALL_LIGHTNING);
                for (zombie, at, ..) in &zombies {
                    if at.translation().distance(proc.at) <= proc.size {
                        damage.write(DamageZombie {
                            player: Some(proc.player),
                            zombie,
                            damage: f32::INFINITY,
                            zone: HitZone::Torso,
                            attack: Attack::AmmoMod(Pay::Paid),
                        });
                        if ball_lightning {
                            chains.0.push((proc.player, at.translation()));
                        }
                    }
                }
                if show {
                    flashes.0.push((proc.at, proc.size, BLAST_COLOUR, GAUNTLET_PROC_ACCENT_SECS, true));
                }
                if let Some(colour) = accent {
                    flashes.0.push((proc.at, CAST_ACCENT_RADIUS, colour, GAUNTLET_PROC_ACCENT_SECS, false));
                }
            }
            ProcEffect::Frost => {
                for (zombie, at, _, factor, frosted, _) in &mut zombies {
                    if at.translation().with_y(0.0).distance(proc.at.with_y(0.0)) > BOON_FROSTBITE_RADIUS_M {
                        continue;
                    }
                    frost(&mut commands, zombie, factor, frosted, proc.size);
                }
                if show {
                    flashes.0.push((proc.at, BOON_FROSTBITE_RADIUS_M, FROST_COLOUR, GAUNTLET_PROC_ACCENT_SECS * 2.0, false));
                }
                if let Some(colour) = accent {
                    flashes.0.push((proc.at, CAST_ACCENT_RADIUS, colour, GAUNTLET_PROC_ACCENT_SECS, false));
                }
            }
        }
        let (cue, borrowed) = def.effect.cue();
        if thinning.cue(cue, now) {
            let (pitch, gain) = if borrowed { (GAUNTLET_PROC_CUE_PITCH, GAUNTLET_PROC_CUE_GAIN) } else { (1.0, 1.0) };
            cues.write(PlayCue { cue, at: Some(proc.at), pitch, gain });
        }
    }
}

/// A Firebrand pool lands where its Lethal was last seen once the Lethal is gone (it went off).
pub fn land_lethals(
    mut landing: ResMut<Landing>,
    projectiles: Query<&GlobalTransform>,
    mut fired: MessageWriter<ProcFired>,
) {
    landing.0.retain_mut(|(projectile, last, proc)| match projectiles.get(*projectile) {
        Ok(at) => {
            *last = at.translation();
            true
        }
        Err(_) => {
            fired.write(ProcFired { at: *last, ..*proc });
            false
        }
    });
}

/// Frost wears off.
pub fn thaw(
    mut commands: Commands,
    mut frosted: Query<(Entity, &mut Frosted, Option<&mut ZombieMoveFactor>)>,
    time: Res<Time>,
) {
    for (zombie, mut frost, factor) in &mut frosted {
        frost.left -= time.delta_secs();
        if frost.left > 0.0 {
            continue;
        }
        commands.entity(zombie).remove::<Frosted>();
        if let Some(mut factor) = factor {
            factor.clear(FROST);
            if factor.is_empty() {
                commands.entity(zombie).remove::<ZombieMoveFactor>();
            }
        }
    }
}

/// A Frosted Zombie wears a pale ring at its feet and a halo; Blasts and Frost bursts flash.
pub fn draw_procs(
    frosted: Query<&GlobalTransform, With<Frosted>>,
    mut flashes: ResMut<ProcFlashes>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for at in &frosted {
        let at = at.translation();
        gizmos.circle(Isometry3d::new(at - Vec3::Y * 0.85, flat), 0.45, FROST_COLOUR);
        gizmos.sphere(Isometry3d::from_translation(at + Vec3::Y * 0.9), 0.18, FROST_COLOUR);
    }
    let dt = time.delta_secs();
    flashes.0.retain_mut(|(at, radius, colour, left, sphere)| {
        *left -= dt;
        if *sphere {
            gizmos.sphere(Isometry3d::from_translation(*at), *radius, *colour);
        } else {
            gizmos.circle(Isometry3d::new(*at + Vec3::Y * 0.05, flat), *radius, *colour);
        }
        *left > 0.0
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::gauntlet::boon::BOONS;

    fn def(id: &str) -> &'static ProcDef {
        proc_of(id).unwrap_or_else(|| panic!("{id} is a Proc Boon"))
    }

    #[test]
    fn each_proc_boon_has_its_trigger_chance_and_cooldown() {
        let table = [
            ("arc_light", Trigger::HeadshotKill, 1.0, 2.0),
            ("hot_brass", Trigger::Reload, 1.0, 8.0),
            ("elbow_room", Trigger::Hit, 1.0, 15.0),
            ("parting_shot", Trigger::MagazineEmptied, 1.0, 0.0),
            ("frostbite", Trigger::KnifeKill, 1.0, 0.0),
            ("spoils_of_war", Trigger::PowerUp, 1.0, 0.0),
            ("turncoat", Trigger::AnyKill, 0.05, 20.0),
            ("firebrand", Trigger::LethalThrown, 1.0, 0.0),
            ("static_discharge", Trigger::Hit, 1.0, 30.0),
            ("wildfire", Trigger::AnyKill, 0.15, 1.0),
            ("firing_squad", Trigger::Reload, 1.0, 10.0),
            ("raise_the_dead", Trigger::HeadshotKill, 0.10, 5.0),
        ];
        for (id, trigger, chance, cooldown) in table {
            let def = def(id);
            assert_eq!((def.trigger, def.chance, def.cooldown_secs), (trigger, chance, cooldown), "{id}");
        }
        assert_eq!(BOONS.iter().filter(|boon| boon.proc.is_some()).count(), 12);
    }

    #[test]
    fn stacks_grow_each_proc_boons_effect_and_nothing_else() {
        // (id, size at 1 Stack, size at 3 Stacks), from Appendix A3.
        let table = [
            ("arc_light", 3.0, 7.0),
            ("hot_brass", 2.5, 3.5),
            ("elbow_room", 6.0, 12.0),
            ("parting_shot", 2.0, 3.0),
            ("frostbite", 3.0, 5.0),
            ("spoils_of_war", 10.0, 20.0),
            ("turncoat", 20.0, 30.0),
            ("firebrand", 2.0, 3.0),
            ("static_discharge", 5.0, 7.0),
            ("wildfire", 3.0, 5.0),
            ("firing_squad", 10.0, 20.0),
            ("raise_the_dead", 1.0, 3.0),
        ];
        for (id, one, three) in table {
            assert_eq!(proc_size(def(id), 1), one, "{id} at 1 Stack");
            assert_eq!(proc_size(def(id), 3), three, "{id} at 3 Stacks");
        }
    }

    #[test]
    fn a_proc_fires_on_a_roll_under_its_chance_and_only_when_ready() {
        let turncoat = def("turncoat");
        assert!(proc_fires(turncoat, true, 0.049));
        assert!(!proc_fires(turncoat, true, 0.05));
        assert!(!proc_fires(turncoat, false, 0.0), "cooling down");
        let arc_light = def("arc_light");
        assert!(proc_fires(arc_light, true, 0.999), "a sure Trigger always fires");
    }

    #[test]
    fn a_proc_cooldown_blocks_it_until_it_runs_out() {
        let mut cooldowns = ProcCooldowns::default();
        assert!(cooldowns.ready("arc_light"));
        assert_eq!(cooldowns.filled("arc_light"), 1.0);
        cooldowns.start("arc_light", 2.0);
        assert!(!cooldowns.ready("arc_light"));
        assert!(cooldowns.ready("hot_brass"), "each Proc Boon has its own");
        cooldowns.tick(0.5);
        assert!((cooldowns.filled("arc_light") - 0.25).abs() < 1e-6);
        cooldowns.tick(1.5);
        assert!(cooldowns.ready("arc_light"));
        cooldowns.start("hot_brass", 0.0);
        assert!(cooldowns.ready("hot_brass"), "no cooldown, never blocked");
    }

    #[test]
    fn a_turning_proc_at_the_turned_ally_cap_never_fires_so_starts_no_cooldown() {
        for id in ["turncoat", "raise_the_dead"] {
            assert!(!proc_may_fire(def(id), true, false, 0.0), "{id} at the cap");
            assert!(proc_may_fire(def(id), true, true, 0.0), "{id} under it");
        }
        assert!(proc_may_fire(def("arc_light"), true, false, 0.0), "only turning Procs heed the cap");
    }

    mod headless {
        use super::*;
        use crate::enemies::zombie::headless::Sim;
        use crate::weapons::ammo_mod::CastEffect;

        /// A solo Player holding `boons`, with Proc dispatch running and nothing to act on casts.
        fn holding(boons: &[(&str, u32)]) -> (Sim, Entity) {
            let mut sim = Sim::new(60.0, |app| {
                app.add_message::<ZombieKilled>()
                    .add_message::<Reloaded>()
                    .add_message::<MagazineEmptied>()
                    .add_message::<LethalThrown>()
                    .add_message::<PowerUpTaken>()
                    .add_message::<ProcFired>()
                    .add_message::<CastEffect>()
                    .add_message::<PlayCue>()
                    .init_resource::<HeldPacts>()
                    .init_resource::<TurnedAllyCap>()
                    .init_resource::<Thinning>()
                    .init_resource::<ProcFlashes>()
                    .init_resource::<Landing>()
                    .init_resource::<BallLightning>()
                    .add_message::<FlingOutside>()
                    .add_systems(Update, (fire_procs, land_lethals, cast_procs, thaw).chain());
            });
            let world = sim.world_mut();
            let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
            let mut held = HeldBoons::default();
            for &(id, stacks) in boons {
                let boon = crate::run::gauntlet::boon::find(id).unwrap();
                for _ in 0..stacks {
                    held.add(boon);
                }
            }
            world.entity_mut(player).insert((held, ProcCooldowns::default(), Seat(0), GlobalTransform::default()));
            (sim, player)
        }

        fn kill(player: Entity, attack: Attack, headshot: bool) -> ZombieKilled {
            ZombieKilled { player: Some(player), pay: Pay::Paid, headshot, zone: HitZone::Head, attack, at: Vec3::new(4.0, 1.0, 12.0), surge: false }
        }

        fn casts(sim: &mut Sim) -> Vec<CastEffect> {
            sim.world_mut().resource_mut::<Messages<CastEffect>>().drain().collect()
        }

        #[test]
        fn a_headshot_kill_casts_arc_lights_shock_and_starts_its_cooldown() {
            let (mut sim, player) = holding(&[("arc_light", 2)]);
            sim.send(kill(player, Attack::Bullet, true));
            sim.run(2);
            let cast = casts(&mut sim);
            assert_eq!(cast.len(), 1);
            assert_eq!((cast[0].element, cast[0].size, cast[0].pay), (Element::Shock, 5.0, Pay::Paid));
            assert_eq!(cast[0].accent, Some(Family::Spark.colour()), "its Family's accent ring");
            // Cooling down: a second Headshot kill straight after casts nothing.
            sim.send(kill(player, Attack::Bullet, true));
            sim.run(2);
            assert!(casts(&mut sim).is_empty());
            assert!(!sim.world_mut().get::<ProcCooldowns>(player).unwrap().ready("arc_light"));
        }

        /// Where the Player stands, with their `GlobalTransform` put there too (`holding` leaves
        /// it at the origin).
        fn player_at(sim: &mut Sim, player: Entity) -> Vec3 {
            let transform = *sim.world_mut().get::<Transform>(player).unwrap();
            sim.world_mut().entity_mut(player).insert(GlobalTransform::from(transform));
            transform.translation
        }

        /// `count` Zombies in a ring `radius` around `at`, on the floor.
        fn ring(sim: &mut Sim, at: Vec3, count: usize, radius: f32) -> Vec<Entity> {
            (0..count)
                .map(|i| {
                    let angle = i as f32 / count as f32 * std::f32::consts::TAU;
                    let feet = at.with_y(0.0) + Vec3::new(angle.cos(), 0.0, angle.sin()) * radius;
                    sim.spawn(feet, None)
                })
                .collect()
        }

        #[test]
        fn storm_shell_fires_static_discharge_on_a_crowd_and_not_without_one() {
            for (crowd, fires) in [(DUO_STORM_SHELL_CROWD, true), (DUO_STORM_SHELL_CROWD - 1, false)] {
                let (mut sim, player) = holding(&[("static_discharge", 1), ("live_wire", 1), ("storm_shell", 1)]);
                let at = player_at(&mut sim, player);
                ring(&mut sim, at, crowd, DUO_STORM_SHELL_RADIUS_M - 1.0);
                sim.run(2);
                let discharges = casts(&mut sim).iter().filter(|cast| cast.element == Element::Shock).count();
                assert_eq!(discharges > 0, fires, "a crowd of {crowd}");
                let ready = sim.world_mut().get::<ProcCooldowns>(player).unwrap().ready(STATIC_DISCHARGE_ID);
                assert_eq!(ready, !fires, "a discharge starts its cooldown");
            }
            let (mut sim, player) = holding(&[("static_discharge", 1)]);
            let at = player_at(&mut sim, player);
            ring(&mut sim, at, DUO_STORM_SHELL_CROWD + 2, 1.5);
            sim.run(2);
            assert!(casts(&mut sim).is_empty(), "without Storm Shell a crowd sets nothing off");
        }

        #[test]
        fn incendiary_volley_makes_a_proc_phantom_gun_ignite() {
            for (duo, ignites) in [(true, true), (false, false)] {
                let boons: &[(&str, u32)] = if duo {
                    &[("spoils_of_war", 1), ("incendiary_volley", 1)]
                } else {
                    &[("spoils_of_war", 1)]
                };
                let (mut sim, player) = holding(boons);
                sim.send(PowerUpTaken(crate::economy::power_up::PowerUp::MaxAmmo, Some(player)));
                sim.run(2);
                let cast = casts(&mut sim);
                assert_eq!(cast.len(), 1);
                assert_eq!((cast[0].element, cast[0].ignites), (Element::PhantomGun, ignites));
            }
        }

        #[test]
        fn ball_lightning_chains_on_from_each_zombie_a_parting_shot_blast_kills() {
            let (mut sim, player) = holding(&[("parting_shot", 1), ("ball_lightning", 1)]);
            let at = Vec3::new(4.0, 0.0, 12.0);
            sim.spawn(at, None);
            sim.spawn(at + Vec3::X * 0.8, None);
            sim.run(5);
            sim.send(MagazineEmptied { player, weapon: crate::weapons::arsenal::WeaponId::M1911, at: Some(at + Vec3::Y) });
            sim.run(2);
            let chains = &sim.world_mut().resource::<BallLightning>().0;
            assert_eq!(chains.len(), 2, "one chain per Zombie the Blast killed");
            assert!(chains.iter().all(|&(by, _)| by == player));
        }

        #[test]
        fn repel_boarders_flings_elbow_rooms_zombies_outside_a_tenth_down_instead_of_killing_them() {
            let (mut sim, player) = holding(&[("elbow_room", 1), ("repel_boarders", 1)]);
            let at = player_at(&mut sim, player);
            let near = ring(&mut sim, at, 3, 2.0);
            sim.run(3);
            sim.send(PlayerHit { player, damage: 10.0, from: None });
            sim.run(2);
            assert!(casts(&mut sim).iter().all(|cast| cast.element != Element::Knockback), "no Knockback kills");
            let mut flung: Vec<_> = sim.world_mut().resource_mut::<Messages<FlingOutside>>().drain().map(|fling| fling.zombie).collect();
            flung.sort();
            let mut expected = near.clone();
            expected.sort();
            assert_eq!(flung, expected);
        }

        #[test]
        fn proc_and_ammo_mod_kills_never_set_off_a_proc() {
            let (mut sim, player) = holding(&[("arc_light", 1), ("frostbite", 1)]);
            sim.send(kill(player, Attack::AmmoMod(Pay::Paid), true));
            sim.send(kill(player, Attack::Burn, true));
            sim.run(2);
            assert!(casts(&mut sim).is_empty());
            assert!(sim.world_mut().get::<ProcCooldowns>(player).unwrap().ready("arc_light"));
        }

        #[test]
        fn frostbite_slows_zombies_near_a_knife_kill_until_it_wears_off() {
            let (mut sim, player) = holding(&[("frostbite", 1)]);
            let near = sim.spawn(Vec3::new(5.0, 0.0, 12.0), None);
            let far = sim.spawn(Vec3::new(14.0, 0.0, 12.0), None);
            sim.run(5);
            sim.send(kill(player, Attack::Knife, false));
            sim.run(2);
            let factor = |sim: &mut Sim, zombie| sim.world_mut().get::<ZombieMoveFactor>(zombie).map(|f| f.factor());
            assert_eq!(factor(&mut sim, near), Some(GAUNTLET_FROST_SPEED_FACTOR));
            assert_eq!(factor(&mut sim, far), None);
            sim.run(sim.frames(BOON_FROSTBITE_SECS + 0.2));
            assert_eq!(factor(&mut sim, near), None, "thawed");
        }
    }

    #[test]
    fn live_wire_shortens_proc_cooldowns_per_stack_and_overclocked_removes_them() {
        assert_eq!(proc_cooldown(10.0, 0, false), 10.0);
        assert!((proc_cooldown(10.0, 1, false) - 8.5).abs() < 1e-5);
        assert!((proc_cooldown(10.0, 2, false) - 7.225).abs() < 1e-5);
        assert_eq!(proc_cooldown(10.0, 0, true), 0.0);
        assert_eq!(proc_cooldown(10.0, 3, true), 0.0);
    }

    #[test]
    fn only_player_kills_set_off_kill_triggers() {
        use Trigger::*;
        assert_eq!(kill_triggers(Attack::Bullet, false), vec![AnyKill]);
        assert_eq!(kill_triggers(Attack::Bullet, true), vec![HeadshotKill, AnyKill]);
        assert_eq!(kill_triggers(Attack::Knife, false), vec![KnifeKill, AnyKill]);
        for attack in [
            Attack::AmmoMod(Pay::Paid),
            Attack::AmmoMod(Pay::Unpaid),
            Attack::Burn,
            Attack::Trap,
            Attack::Nuke,
            Attack::Barrier,
        ] {
            assert!(kill_triggers(attack, true).is_empty(), "{attack:?} is no Player kill");
        }
    }

    #[test]
    fn shock_fire_knockback_frost_and_blast_are_paid_and_phantom_guns_and_turns_unpaid() {
        for id in ["arc_light", "hot_brass", "elbow_room", "parting_shot", "frostbite", "firebrand", "static_discharge", "wildfire"] {
            assert_eq!(def(id).effect.pay(), Pay::Paid, "{id}");
        }
        for id in ["spoils_of_war", "turncoat", "firing_squad", "raise_the_dead"] {
            assert_eq!(def(id).effect.pay(), Pay::Unpaid, "{id}");
        }
    }

    #[test]
    fn a_proc_cue_waits_out_its_gap_and_plays_at_most_four_at_once() {
        let mut gate = CueGate::default();
        assert!(gate.allow(0.0));
        assert!(!gate.allow(0.05), "inside the gap");
        assert!(gate.allow(0.1));
        assert!(gate.allow(0.2));
        assert!(gate.allow(0.3));
        assert!(!gate.allow(0.4), "four still playing");
        assert!(gate.allow(0.55), "the first has finished");
    }

    #[test]
    fn proc_visuals_past_the_cap_cast_unseen_and_accents_past_theirs_are_dropped() {
        let mut gate = VisualGate::default();
        let shown: Vec<_> = (0..10).map(|_| gate.allow(0.0)).collect();
        assert_eq!(shown.iter().filter(|(show, _)| *show).count(), 8);
        assert_eq!(shown.iter().filter(|(_, accent)| *accent).count(), 4);
        assert_eq!(gate.allow(1.0), (true, true), "all gone by now");
    }
}
