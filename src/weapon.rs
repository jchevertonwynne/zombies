use avian3d::prelude::*;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;

use crate::hands::{ReloadPose, ReloadStyle, knife_pose, reload_pose, stab_lunge};
use crate::controls::{Action, Controls};
use crate::ammo_mod::{AmmoModHit, AmmoMods, glint_colour, pick_ammo_mod};
use crate::arsenal::{Burn, FireMode, Splash, WeaponId, WeaponStats};
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::attachment::{
    Attachment, FlameFired, Underbarrel, UnderbarrelAmmo, built_in_underbarrel, free_mounts, pick_attachment,
};
use crate::burn::Ignite;
use crate::combat::{Crossing, Sights, hit_damage, pierce, spread_direction};
use crate::explosive::{Explosion, Explosive, Launched, flight, makes_crawlers};
use crate::gun_model::{GunModels, model_length};
use crate::layers::GameLayer;
use crate::modifier::{Effect, Modifier, Modifiers, Source, Stat, resolve, resolve_count};
use crate::perk::{Drinking, hands_free};
use crate::player::{Locomotion, Player, PlayerCamera};
use crate::state::Pause;
use crate::tuning::*;
use crate::wonder::{WonderFired, is_wonder};
use crate::zombie::{Attack, DamageZombie, HitArea, HitZone, ZombieKilled};

/// One Weapon the Player carries: its ammo and its own modifier entries.
pub struct WeaponCopy {
    pub id: WeaponId,
    /// Whether it has been through Pack-a-Punch, which swaps in its upgraded row.
    pub upgraded: bool,
    pub magazine: u32,
    pub reserve: u32,
    pub modifiers: Vec<Modifier>,
    /// The Ammo Mods its tap Repacks added.
    pub ammo_mods: AmmoMods,
    /// Repacks of this copy so far, of either kind; each costs more than the last.
    pub repacks: u32,
    /// The Attachments its hold Repacks added, at most one per mount.
    pub attachments: Vec<Attachment>,
    /// Its Underbarrel and that gun's own ammo: an Attachment, or built in.
    pub underbarrel: Option<UnderbarrelAmmo>,
    /// Reloads of the main gun finished, for Dual Mag's every-second Reload.
    pub reloads: u32,
}

/// Whether a Weapon with Dual Mag, having finished `reloads` Reloads, takes the flat
/// `DUAL_MAG_RELOAD_SECS` for the next: every second one does.
pub fn dual_mag_flat(reloads: u32) -> bool {
    reloads % 2 == 1
}

impl WeaponCopy {
    /// A freshly got Weapon: a full Magazine and its starting Reserve Ammo.
    pub fn new(id: WeaponId) -> Self {
        let stats = id.stats();
        Self {
            id,
            upgraded: false,
            magazine: stats.magazine,
            reserve: stats.starting_reserve(),
            modifiers: Vec::new(),
            ammo_mods: AmmoMods::NONE,
            repacks: 0,
            attachments: Vec::new(),
            underbarrel: None,
            reloads: 0,
        }
    }

    /// Its row in the Weapon table: the base row, or the upgraded one.
    pub fn stats(&self) -> &'static WeaponStats {
        if self.upgraded { self.id.upgraded() } else { self.id.stats() }
    }

    /// A full Magazine of its own, before the Player's entries.
    pub fn magazine_size(&self) -> u32 {
        resolve_count(Stat::MagazineSize, self.stats().magazine, &self.modifiers)
    }

    /// The most Reserve Ammo it carries, after its own entries (Extended Mag).
    pub fn reserve_max(&self) -> u32 {
        resolve_count(Stat::ReserveSize, self.stats().reserve_max, &self.modifiers)
    }

    /// The most Reserve Ammo it carries for the Player, after their entries and its own.
    pub fn reserve_cap(&self, player: &[Modifier]) -> u32 {
        resolve_count(Stat::ReserveSize, self.stats().reserve_max, player.iter().chain(&self.modifiers))
    }

    /// Fill its Magazine, its Reserve Ammo and its Underbarrel's ammo.
    fn filled(self) -> Self {
        Self {
            magazine: self.magazine_size(),
            reserve: self.reserve_max(),
            underbarrel: self.underbarrel.map(|underbarrel| UnderbarrelAmmo::full(underbarrel.kind)),
            ..self
        }
    }

    /// Its Pack-a-Punch form: the upgraded row, with a full Magazine and full Reserve Ammo, and
    /// any built-in Underbarrel loaded. Its own modifier entries stay with it, ready for Repacks.
    pub fn upgrade(self) -> Self {
        Self {
            upgraded: true,
            underbarrel: built_in_underbarrel(self.id).map(UnderbarrelAmmo::full),
            ..self
        }
        .filled()
    }

    /// Its tap Repack: a random Ammo Mod it lacks, picked by `roll` in [0, 1), and full ammo, as
    /// out of the upgrade. A copy with no room for another Ammo Mod (of `slots`) comes back
    /// unchanged.
    pub fn repack(self, roll: f32, slots: usize) -> Self {
        let Some(ammo_mod) = pick_ammo_mod(self.ammo_mods, roll).filter(|_| self.ammo_mods.has_room(slots)) else {
            return self;
        };
        Self {
            ammo_mods: self.ammo_mods.with(ammo_mod),
            repacks: self.repacks.saturating_add(1),
            ..self
        }
        .filled()
    }

    /// Whether a hold Repack has a mount to fill on it.
    pub fn has_free_mount(&self) -> bool {
        self.upgraded && !free_mounts(self.stats(), &self.attachments).is_empty()
    }

    /// Its hold Repack: a random Attachment in a free mount, picked by the two rolls in [0, 1),
    /// with its entries as their own source and full ammo. An Underbarrel comes loaded. A copy
    /// with no free mount comes back unchanged.
    pub fn repack_attachment(mut self, mount_roll: f32, attachment_roll: f32) -> Self {
        let free = free_mounts(self.stats(), &self.attachments);
        let Some(attachment) = pick_attachment(&free, mount_roll, attachment_roll).filter(|_| self.upgraded) else {
            return self;
        };
        self.attachments.push(attachment);
        self.modifiers.extend(attachment.modifiers());
        if let Some(kind) = attachment.underbarrel() {
            self.underbarrel = Some(UnderbarrelAmmo::full(kind));
        }
        Self {
            repacks: self.repacks.saturating_add(1),
            ..self
        }
        .filled()
    }
}

/// A Reload in progress. The time it takes is resolved every frame, so only progress is kept.
pub struct Reload {
    pub elapsed: f32,
    /// The unmodified seconds, fixed when the Reload starts (tactical or empty, bullets to load).
    base_secs: f32,
    /// Dual Mag's flat Reload, which no modifier shortens.
    flat: bool,
}

/// Putting one Weapon away and bringing another up.
pub struct Swap {
    pub from: &'static WeaponStats,
    /// The glint on the Weapon being put away.
    pub from_glint: Option<AmmoMods>,
    pub elapsed: f32,
    pub drop_secs: f32,
    pub raise_secs: f32,
}

impl Swap {
    fn between(from: &WeaponCopy, to: &'static WeaponStats) -> Self {
        let (from_glint, from) = (glint_of(from), from.stats());
        Self {
            from,
            from_glint,
            elapsed: 0.0,
            drop_secs: from.handling().drop_secs,
            raise_secs: to.handling().raise_secs,
        }
    }

    pub fn secs(&self) -> f32 {
        self.drop_secs + self.raise_secs
    }
}

/// The Player's Weapon slots: `WEAPON_SLOTS` of them, or as many as the stat entries say. The held
/// slot is always filled; the Knife is separate.
#[derive(Component)]
pub struct Loadout {
    slots: Vec<Option<WeaponCopy>>,
    held: usize,
    pub reload: Option<Reload>,
    pub swap: Option<Swap>,
    since_shot: f32,
    /// Shots left in the current burst.
    burst_left: u32,
    /// How far the sights are up, 0 (hip) to 1 (aimed).
    pub aim: f32,
    /// How far the hip spread has bloomed, 0 (fresh) to 1 (full).
    bloom: f32,
    /// Seconds of sprint-out left before the Weapon can fire or aim.
    sprint_out: f32,
    /// Whether the held Weapon's Underbarrel is up instead of its main gun.
    underbarrel_up: bool,
    /// Seconds left of a toggle to or from the Underbarrel.
    pub toggle: f32,
}

impl Loadout {
    pub fn new(id: WeaponId) -> Self {
        Self {
            slots: std::iter::once(Some(WeaponCopy::new(id)))
                .chain(std::iter::repeat_with(|| None))
                .take(WEAPON_SLOTS as usize)
                .collect(),
            held: 0,
            reload: None,
            swap: None,
            since_shot: f32::INFINITY,
            burst_left: 0,
            aim: 0.0,
            bloom: 0.0,
            sprint_out: 0.0,
            underbarrel_up: false,
            toggle: 0.0,
        }
    }

    pub fn held(&self) -> &WeaponCopy {
        self.slots[self.held].as_ref().expect("the held slot is always filled")
    }

    pub fn held_mut(&mut self) -> &mut WeaponCopy {
        self.slots[self.held].as_mut().expect("the held slot is always filled")
    }

    pub fn slot(&self, slot: usize) -> Option<&WeaponCopy> {
        self.slots.get(slot).and_then(Option::as_ref)
    }

    /// Every Weapon carried, in slot order.
    pub fn weapons(&self) -> impl Iterator<Item = &WeaponCopy> {
        self.slots.iter().flatten()
    }

    /// How many slots there are, filled or not.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Grows or shrinks to `count` slots (at least one). Shrinking drops the Weapons in the slots
    /// that go; if the one in hand goes, the first slot's Weapon comes up.
    pub fn set_slot_count(&mut self, count: usize) {
        let count = count.max(1);
        if count == self.slots.len() {
            return;
        }
        if count > self.slots.len() {
            self.slots.resize_with(count, || None);
            return;
        }
        // Weapons in the slots that go move into empty slots that stay, while there are any.
        for from in count..self.slots.len() {
            if self.slots[from].is_some()
                && let Some(to) = self.slots[..count].iter().position(Option::is_none)
            {
                self.slots.swap(from, to);
                if self.held == from {
                    self.held = to;
                }
            }
        }
        self.slots.truncate(count);
        // The Weapon in hand went only if every slot that stays is filled, so the first is.
        if self.held >= count {
            self.held = 0;
            self.raise();
        }
    }

    /// The held Weapon's Underbarrel, while it's up instead of the main gun.
    pub fn underbarrel(&self) -> Option<&UnderbarrelAmmo> {
        self.held().underbarrel.as_ref().filter(|_| self.underbarrel_up)
    }

    /// The row of the gun that fires now: the held Weapon's, or its Underbarrel's.
    pub fn gun_stats(&self) -> &'static WeaponStats {
        self.underbarrel().map_or(self.held().stats(), |underbarrel| underbarrel.kind.stats())
    }

    /// The entries on the gun that fires now. An Underbarrel takes the Player's but none of the
    /// Weapon's own: its Attachments belong to the main gun.
    fn gun_entries<'a>(&'a self, player: &'a [Modifier]) -> impl Iterator<Item = &'a Modifier> {
        let own: &[Modifier] = if self.underbarrel().is_some() { &[] } else { &self.held().modifiers };
        player.iter().chain(own)
    }

    /// The Magazine and Reserve Ammo of the gun that fires now.
    pub fn gun_ammo(&self) -> (u32, u32) {
        match self.underbarrel() {
            Some(underbarrel) => (underbarrel.magazine, underbarrel.reserve),
            None => (self.held().magazine, self.held().reserve),
        }
    }

    fn gun_ammo_mut(&mut self) -> (&mut u32, &mut u32) {
        let up = self.underbarrel_up;
        let copy = self.slots[self.held].as_mut().expect("the held slot is always filled");
        match copy.underbarrel.as_mut() {
            Some(underbarrel) if up => (&mut underbarrel.magazine, &mut underbarrel.reserve),
            _ => (&mut copy.magazine, &mut copy.reserve),
        }
    }

    /// A stat of the gun that fires now, resolved from its base and the Player's and the Weapon's
    /// entries.
    pub fn resolve(&self, stat: Stat, base: f32, player: &[Modifier]) -> f32 {
        resolve(stat, base, self.gun_entries(player))
    }

    /// A full Magazine for the gun that fires now. An Underbarrel's is fixed.
    pub fn magazine_size(&self, player: &[Modifier]) -> u32 {
        match self.underbarrel() {
            Some(underbarrel) => underbarrel.kind.stats().magazine,
            None => self.slot_magazine_size(self.held, player),
        }
    }

    /// The Magazine size of the Weapon in a filled slot.
    fn slot_magazine_size(&self, slot: usize, player: &[Modifier]) -> u32 {
        let weapon = self.slot(slot).expect("a filled slot");
        resolve_count(Stat::MagazineSize, weapon.stats().magazine, player.iter().chain(&weapon.modifiers))
    }

    /// The slot holding this Weapon, if the Player carries it.
    pub fn slot_of(&self, id: WeaponId) -> Option<usize> {
        self.slots.iter().position(|slot| slot.as_ref().is_some_and(|weapon| weapon.id == id))
    }

    /// Whether the Weapon in a filled slot has a full Magazine, full Reserve Ammo and a full
    /// Underbarrel.
    pub fn ammo_full(&self, slot: usize, player: &[Modifier]) -> bool {
        let weapon = self.slot(slot).expect("a filled slot");
        weapon.magazine >= self.slot_magazine_size(slot, player)
            && weapon.reserve >= weapon.reserve_cap(player)
            && weapon.underbarrel.is_none_or(|underbarrel| underbarrel.is_full())
    }

    /// Fill the Magazine, Reserve Ammo and Underbarrel of the Weapon in a filled slot, as an ammo
    /// buy or Max Ammo does. A Reload of that Weapon has nothing left to do, so it ends.
    pub fn refill(&mut self, slot: usize, player: &[Modifier]) {
        let size = self.slot_magazine_size(slot, player);
        let weapon = self.slots[slot].as_mut().expect("a filled slot");
        weapon.magazine = weapon.magazine.max(size);
        weapon.reserve = weapon.reserve.max(weapon.reserve_cap(player));
        if let Some(underbarrel) = &mut weapon.underbarrel {
            *underbarrel = UnderbarrelAmmo::full(underbarrel.kind);
        }
        if slot == self.held {
            self.reload = None;
        }
    }

    /// Max Ammo: every Weapon carried gets full Reserve Ammo (Extended Mag's included) and a full
    /// Underbarrel. Magazines are left as they are.
    pub fn max_ammo(&mut self) {
        for weapon in self.slots.iter_mut().flatten() {
            weapon.reserve = weapon.reserve.max(weapon.reserve_max());
            if let Some(underbarrel) = &mut weapon.underbarrel {
                *underbarrel = UnderbarrelAmmo::full(underbarrel.kind);
            }
        }
    }

    /// Damage per bullet at a distance, after falloff, before the hit zone and penetration.
    /// Falloff itself goes through the seam, so a Long Barrel can take it away.
    pub fn damage_at(&self, distance: f32, player: &[Modifier]) -> f32 {
        let stats = self.gun_stats();
        let falloff = self.resolve(Stat::DamageFalloff, 1.0, player);
        let fallen = stats.damage_max + (stats.damage_at(distance) - stats.damage_max) * falloff;
        self.resolve(Stat::Damage, fallen, player)
    }

    /// Whether the sights are fully up, so optics count.
    pub fn aimed(&self) -> bool {
        self.aim >= 1.0
    }

    /// What the held Weapon's optic adds to a bullet: only while aimed, and never to an
    /// Underbarrel's.
    pub fn sights(&self, player: &[Modifier]) -> Sights {
        if !self.aimed() || self.underbarrel().is_some() {
            return Sights::PLAIN;
        }
        Sights {
            headshot: self.resolve(Stat::AimedHeadshot, 1.0, player),
            extra_pierce: self.resolve(Stat::AimedPierce, 0.0, player).round().max(0.0) as u32,
            pierce_damage: self.resolve(Stat::AimedPierceDamage, PENETRATION_DAMAGE_FACTOR, player),
        }
    }

    /// Zombies past the first the gun that fires now passes its bullets through: its penetration
    /// tier, plus any stat entries.
    pub fn penetration(&self, player: &[Modifier]) -> u32 {
        let base = self.gun_stats().penetration.extra_zombies();
        resolve_count(Stat::Penetration, base, self.gun_entries(player))
    }

    /// The held Weapon's damage multiplier for a hit zone. The head's resolves through the seam.
    pub fn zone_multiplier(&self, zone: HitZone, player: &[Modifier]) -> f32 {
        let base = self.held().stats().zone_multiplier(zone);
        if zone == HitZone::Head { self.resolve(Stat::HeadshotMultiplier, base, player) } else { base }
    }

    /// A kill puts bullets back in the held Weapon's Magazine, up to a full one, if the Player's
    /// entries say so. A Reload of a Magazine that fills has nothing left to do, so it ends.
    pub fn refill_on_kill(&mut self, player: &[Modifier]) {
        let bullets = resolve_count(Stat::MagazinePerKill, 0, player.iter().chain(&self.held().modifiers));
        if bullets == 0 {
            return;
        }
        let size = self.magazine_size(player);
        let weapon = self.held_mut();
        weapon.magazine = weapon.magazine.saturating_add(bullets).min(size).max(weapon.magazine);
        if weapon.magazine >= size {
            self.reload = None;
        }
    }

    /// The Player's move speed from a base: the held Weapon's class factor is one more entry
    /// through the modifier seam.
    pub fn move_speed(&self, base: f32, player: &[Modifier]) -> f32 {
        let class = Modifier {
            stat: Stat::MoveSpeed,
            source: Source("Weapon Class"),
            effect: Effect::Mult(self.held().stats().handling().move_speed),
        };
        resolve(Stat::MoveSpeed, base, player.iter().chain(&self.held().modifiers).chain([&class]))
    }

    /// Whether the held Weapon can be aimed at all (dual-wield can't).
    pub fn can_aim(&self) -> bool {
        self.held().stats().ads_fov().is_some()
    }

    /// Advance sprint-out, aiming and spread recovery. Sprinting holds the Weapon down; aiming
    /// waits out the sprint-out, a switch and a Reload.
    pub fn update_stance(&mut self, dt: f32, sprinting: bool, wants_aim: bool) {
        if sprinting {
            self.sprint_out = self.held().stats().handling().sprint_out_secs;
        } else {
            self.sprint_out = (self.sprint_out - dt).max(0.0);
        }
        let aiming = wants_aim
            && self.can_aim()
            && !sprinting
            && self.sprint_out <= 0.0
            && self.swap.is_none()
            && self.reload.is_none();
        let target = if aiming { 1.0 } else { 0.0 };
        self.aim = move_towards(self.aim, target, dt / ADS_SECS);
        self.bloom = (self.bloom - dt / SPREAD_RECOVERY_SECS).max(0.0);
    }

    /// The current spread in degrees of the gun that fires now: the bloomed hip spread (through
    /// the Weapon's own entries, so a Grip tightens it), narrowing to the aimed spread as the
    /// sights come up.
    pub fn spread(&self) -> f32 {
        let stats = self.gun_stats();
        let (fresh, full) = stats.hip_spread();
        let grip = self.resolve(Stat::HipSpread, 1.0, &[]);
        let hip = (fresh + (full - fresh) * self.bloom) * grip;
        hip + (stats.aimed_spread() - hip) * self.aim
    }

    /// The factor aiming puts on the FOV now, through the held Weapon's own entries (an ACOG
    /// zooms further).
    pub fn fov_factor(&self) -> f32 {
        let held = self.held();
        let ads = resolve(Stat::AimedFov, held.stats().ads_fov().unwrap_or(1.0), &held.modifiers);
        1.0 + (ads - 1.0) * self.aim
    }

    pub fn fire_interval(&self, player: &[Modifier]) -> f32 {
        self.resolve(Stat::FireInterval, self.gun_stats().fire_interval, player)
    }

    /// Seconds the current Reload takes in all, if reloading. Dual Mag's flat Reload ignores
    /// every modifier.
    pub fn reload_secs(&self, player: &[Modifier]) -> Option<f32> {
        let reload = self.reload.as_ref()?;
        Some(if reload.flat { reload.base_secs } else { self.resolve(Stat::ReloadTime, reload.base_secs, player) })
    }

    /// Switch between the held Weapon's main gun and its Underbarrel, taking
    /// `UNDERBARREL_TOGGLE_SECS`. Like a switch, it ends a Reload and drops the sights.
    pub fn toggle_underbarrel(&mut self) -> bool {
        if self.held().underbarrel.is_none() || self.swap.is_some() || self.toggle > 0.0 {
            return false;
        }
        let up = !self.underbarrel_up;
        self.reset_for_swap();
        self.underbarrel_up = up;
        self.toggle = UNDERBARREL_TOGGLE_SECS;
        true
    }

    /// Bring up the Weapon in a slot. A switch cancels a Reload, losing its progress.
    pub fn switch_to(&mut self, slot: usize) -> bool {
        if self.swap.is_some() || slot == self.held || self.slot(slot).is_none() {
            return false;
        }
        let to = self.slot(slot).expect("a filled slot").stats();
        self.swap = Some(Swap::between(self.held(), to));
        self.held = slot;
        self.reset_for_swap();
        true
    }

    /// Switch to the next filled slot after the one in hand, wrapping round.
    pub fn cycle(&mut self) -> bool {
        match self.next_filled() {
            Some(slot) => self.switch_to(slot),
            None => false,
        }
    }

    /// The next filled slot after the one in hand, wrapping round, if there is another.
    fn next_filled(&self) -> Option<usize> {
        let count = self.slots.len();
        (1..count).map(|step| (self.held + step) % count).find(|&slot| self.slots[slot].is_some())
    }

    /// Take a new Weapon and bring it up: into the empty slot if there is one, otherwise in
    /// place of the Weapon in hand.
    pub fn give(&mut self, id: WeaponId) {
        self.give_copy(WeaponCopy::new(id));
    }

    /// Bring the held Weapon up from empty hands, as at the start and end of a Last Stand: just
    /// its raise, with nothing to put away first.
    pub fn raise(&mut self) {
        self.swap = Some(Swap::between(self.held(), self.held().stats()));
        self.reset_for_swap();
        if let Some(swap) = &mut self.swap {
            swap.drop_secs = 0.0;
        }
    }

    /// Take a Weapon copy and bring it up: in place of another copy of the same Weapon if one is
    /// carried, else into the empty slot, else in place of the Weapon in hand.
    pub fn give_copy(&mut self, copy: WeaponCopy) {
        let mut target = self.held;
        if let Some(same) = self.slot_of(copy.id) {
            target = same;
        } else if let Some(empty) = self.slots.iter().position(Option::is_none) {
            target = empty;
        }
        self.swap = Some(Swap::between(self.held(), copy.stats()));
        self.held = target;
        self.slots[target] = Some(copy);
        self.reset_for_swap();
    }

    /// Hand over the Weapon in hand, as Pack-a-Punch takes it, and bring up the next one. Its
    /// slot stays free until something fills it. The held slot is always filled, so the Player
    /// must carry another Weapon.
    pub fn take_held(&mut self) -> Option<WeaponCopy> {
        let other = self.next_filled()?;
        let taken = self.slots[self.held].take().expect("the held slot is always filled");
        self.held = other;
        self.begin_swap(&taken);
        Some(taken)
    }

    /// Add one bullet to the Reserve Ammo of a carried Weapon, as picking a Ballistic Knife's blade
    /// back up does. Returns whether it was taken: not if the Weapon isn't carried or is full.
    pub fn pick_up_bullet(&mut self, id: WeaponId) -> bool {
        let Some(slot) = self.slot_of(id) else { return false };
        let weapon = self.slots[slot].as_mut().expect("a filled slot");
        if weapon.reserve >= weapon.reserve_max() {
            return false;
        }
        weapon.reserve += 1;
        true
    }

    /// Whether the Player carries a Weapon besides the one in hand.
    pub fn has_spare(&self) -> bool {
        self.next_filled().is_some()
    }

    /// Start putting `from` away and bringing up the Weapon now in hand.
    fn begin_swap(&mut self, from: &WeaponCopy) {
        self.swap = Some(Swap::between(from, self.held().stats()));
        self.reset_for_swap();
    }

    /// A switch ends a Reload and a burst, drops the sights and the bloom, and puts the main
    /// gun back up.
    fn reset_for_swap(&mut self) {
        self.reload = None;
        self.burst_left = 0;
        self.aim = 0.0;
        self.bloom = 0.0;
        self.underbarrel_up = false;
        self.toggle = 0.0;
    }

    /// Holds every Weapon's Reserve Ammo to the most the Player's entries let it carry. Max Ammo
    /// and new Weapons fill to the Weapon's own most; this trims them.
    pub fn cap_reserves(&mut self, player: &[Modifier]) {
        for weapon in self.slots.iter_mut().flatten() {
            weapon.reserve = weapon.reserve.min(weapon.reserve_cap(player));
        }
    }

    /// With a `Stat::ReserveFeed` entry, the gun that fires now takes its bullets straight from
    /// Reserve Ammo: its Magazine is topped up from it at once, so it never needs a Reload.
    pub fn feed_from_reserve(&mut self, player: &[Modifier]) {
        if resolve(Stat::ReserveFeed, 0.0, player) < 1.0 {
            return;
        }
        let size = self.magazine_size(player);
        let (magazine, reserve) = self.gun_ammo_mut();
        let taken = size.saturating_sub(*magazine).min(*reserve);
        *reserve -= taken;
        *magazine += taken;
        self.reload = None;
    }

    /// Advance the clocks that don't depend on any stat.
    pub fn tick(&mut self, dt: f32) {
        self.since_shot += dt;
        self.toggle = (self.toggle - dt).max(0.0);
        if let Some(swap) = &mut self.swap {
            swap.elapsed += dt;
            if swap.elapsed >= swap.secs() {
                self.swap = None;
            }
        }
        // A Flamethrower recharges whenever it isn't the gun firing.
        let firing = self.underbarrel().is_some() && self.since_shot < FLAMETHROWER.fire_interval * 2.0;
        for (slot, weapon) in self.slots.iter_mut().enumerate() {
            if let Some(underbarrel) = weapon.as_mut().and_then(|weapon| weapon.underbarrel.as_mut())
                && !(firing && slot == self.held)
            {
                underbarrel.recharge(dt);
            }
        }
    }

    /// Start a Reload of the gun that fires now, if its Magazine has room and there is Reserve
    /// Ammo to fill it from. The Flamethrower recharges instead.
    pub fn start_reload(&mut self, player: &[Modifier]) -> bool {
        let size = self.magazine_size(player);
        let (magazine, reserve) = self.gun_ammo();
        let recharges = self.underbarrel().is_some_and(|underbarrel| underbarrel.kind == Underbarrel::Flamethrower);
        if self.swap.is_some() || self.reload.is_some() || magazine >= size || reserve == 0 || recharges {
            return false;
        }
        let loading = (size - magazine).min(reserve);
        let held = self.held();
        let dual_mag = self.underbarrel().is_none() && held.attachments.contains(&Attachment::DualMag);
        let flat = dual_mag && dual_mag_flat(held.reloads);
        let base_secs = if flat { DUAL_MAG_RELOAD_SECS } else { self.gun_stats().reload.secs(magazine, loading) };
        self.reload = Some(Reload {
            elapsed: 0.0,
            base_secs,
            flat,
        });
        self.burst_left = 0;
        true
    }

    /// Advance a Reload against its resolved time, filling the Magazine when it's done.
    pub fn advance_reload(&mut self, dt: f32, player: &[Modifier]) {
        let Some(secs) = self.reload_secs(player) else { return };
        let size = self.magazine_size(player);
        let reload = self.reload.as_mut().expect("reloading");
        reload.elapsed += dt;
        if reload.elapsed >= secs {
            self.reload = None;
            if self.underbarrel().is_none() {
                let held = self.held_mut();
                held.reloads = held.reloads.wrapping_add(1);
            }
            let (magazine, reserve) = self.gun_ammo_mut();
            let taken = size.saturating_sub(*magazine).min(*reserve);
            *reserve -= taken;
            *magazine += taken;
        }
    }

    /// Whether the gun that fires now fires, given the trigger. Spends the bullet when it does.
    pub fn try_fire(&mut self, pressed: bool, just_pressed: bool, player: &[Modifier]) -> bool {
        let wants = match self.gun_stats().fire {
            FireMode::Semi => just_pressed,
            FireMode::Auto => pressed,
            FireMode::Burst(shots) => {
                if just_pressed && self.burst_left == 0 {
                    self.burst_left = shots;
                }
                self.burst_left > 0
            }
        };
        let ready = self.swap.is_none()
            && self.toggle <= 0.0
            && self.reload.is_none()
            && self.sprint_out <= 0.0
            && self.since_shot >= self.fire_interval(player);
        if !wants || !ready {
            return false;
        }
        let (magazine, _) = self.gun_ammo_mut();
        if *magazine == 0 {
            self.burst_left = 0;
            return false;
        }
        *magazine -= 1;
        self.since_shot = 0.0;
        self.burst_left = self.burst_left.saturating_sub(1);
        self.bloom = (self.bloom + SPREAD_BLOOM_PER_SHOT).min(1.0);
        true
    }
}

fn move_towards(from: f32, to: f32, step: f32) -> f32 {
    if from < to { (from + step).min(to) } else { (from - step).max(to) }
}

#[derive(Component, Default)]
pub struct Knife {
    cooldown: f32,
    /// The Bowie Knife, bought once from the wall, replaces the Knife's damage.
    pub bowie: bool,
}

impl Knife {
    /// How far (0 to 1) through its swing the Knife is, while it's swinging.
    pub fn swing(&self) -> Option<f32> {
        (self.cooldown > 0.0).then(|| 1.0 - self.cooldown / KNIFE_COOLDOWN_SECS)
    }
}

/// The Knife's melee damage, or the Bowie Knife's once bought. With the Ballistic Knife in hand
/// the melee uses its blade instead: `ballistic` is `Some(upgraded)` then.
pub fn knife_damage(bowie: bool, ballistic: Option<bool>) -> f32 {
    match (ballistic, bowie) {
        (None, false) => KNIFE_DAMAGE,
        (None, true) => BOWIE_KNIFE_DAMAGE,
        (Some(false), false) => BALLISTIC_KNIFE_MELEE_DAMAGE,
        (Some(false), true) => BALLISTIC_KNIFE_BOWIE_MELEE_DAMAGE,
        (Some(true), false) => KRAUSS_MELEE_DAMAGE,
        (Some(true), true) => KRAUSS_BOWIE_MELEE_DAMAGE,
    }
}

/// The held Weapon went off (for audio and effects): which Weapon, whether it's Upgraded, and the
/// Underbarrel it fired through, if any.
#[derive(Message, Debug, Clone, Copy)]
pub struct ShotFired {
    pub weapon: WeaponId,
    pub upgraded: bool,
    pub underbarrel: Option<Underbarrel>,
}

/// The on-screen Weapon model, a child of the camera.
#[derive(Component)]
pub struct ViewModel {
    kick: f32,
}

/// The purple of an Upgraded Weapon: its name on the HUD, and its plain glint.
pub const UPGRADED_PURPLE: Color = Color::srgb(0.62, 0.2, 1.0);

/// What colours a Weapon copy's glint: its Ammo Mods (maybe none), or `None` if it doesn't glint
/// because it isn't Upgraded.
pub fn glint_of(copy: &WeaponCopy) -> Option<AmmoMods> {
    copy.upgraded.then_some(copy.ammo_mods)
}

/// The band of light sweeping along an Upgraded Weapon's model, a child of the view model.
#[derive(Component)]
struct Glint {
    material: Handle<StandardMaterial>,
    colour: Color,
    sweep: f32,
    /// Sweeps so far, so two Ammo Mods take turns.
    pass: u32,
}

/// The moving parts around the view model. The Stretch stretches the Attachments and the glint
/// along the Weapon to its class's `model_length`; the magazine and the left hand come off the
/// model in a Reload; the knife hand is the camera's own, and slashes when the Knife is used.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewPart {
    Stretch,
    Magazine,
    LeftHand,
    Knife,
}

/// Poses the view model from scratch each frame; camera sway adds to the pose after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnimateViewModel;

pub struct WeaponPlugin;

impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ShotFired>()
            .init_resource::<GunModels>()
            .add_systems(Update, spawn_view_model)
            .add_systems(
                Update,
                // A Perk drink puts the Weapon away and keeps the Knife out of reach.
                ((switch_weapons, reload, aim, fire, knife).chain().run_if(hands_free), animate_view_model.in_set(AnimateViewModel))
                    .chain()
                    .run_if(in_state(Pause::Running)),
            )
            .add_systems(Update, (resolve_slots, refill_on_kill).run_if(in_state(Pause::Running)));
    }
}

fn spawn_view_model(
    mut commands: Commands,
    cameras: Query<Entity, Added<PlayerCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    guns: Res<GunModels>,
) {
    for camera in &cameras {
        let glint = materials.add(StandardMaterial {
            base_color: UPGRADED_PURPLE,
            emissive: LinearRgba::from(UPGRADED_PURPLE) * 8.0,
            unlit: true,
            ..default()
        });
        // `animate_view_model` swaps in the parts of the Weapon in hand.
        let part = |part: ViewPart| {
            (
                part,
                Mesh3d(guns.knife.clone()),
                MeshMaterial3d(guns.material.clone()),
                Transform::default(),
                Visibility::Hidden,
            )
        };
        commands.entity(camera).with_child(part(ViewPart::Knife));
        commands.entity(camera).with_child((
            ViewModel { kick: 0.0 },
            Mesh3d(guns.view(STARTING_WEAPON)),
            MeshMaterial3d(guns.material.clone()),
            Transform::from_xyz(0.22, -0.2, -0.45),
            Visibility::default(),
            children![
                (
                    ViewPart::Stretch,
                    Transform::default(),
                    Visibility::default(),
                    children![(
                        Glint {
                            material: glint.clone(),
                            colour: UPGRADED_PURPLE,
                            sweep: 0.0,
                            pass: 0,
                        },
                        // A thin band just proud of the model's sides.
                        Mesh3d(meshes.add(Cuboid::new(0.066, 0.106, 0.04))),
                        MeshMaterial3d(glint),
                        Transform::default(),
                        Visibility::Hidden,
                    )],
                ),
                part(ViewPart::Magazine),
                part(ViewPart::LeftHand),
            ],
        ));
    }
}

/// Resolves a collider hit to the Zombie it belongs to, and the hit zone.
fn zombie_for(hit: Entity, hit_areas: &Query<(&HitArea, &ChildOf)>) -> Option<(Entity, HitZone)> {
    hit_areas.get(hit).ok().map(|(area, parent)| (parent.parent(), area.0))
}

/// Attacks hit the world and Zombies' hit areas, never the body capsules they move with.
fn attack_filter(player: Entity) -> SpatialQueryFilter {
    SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::ZombieHitArea]).with_excluded_entities([player])
}

/// Scroll, the Switch Weapon key, or the Weapon 1/2 (and 3, with a third slot) keys switch Weapons.
fn switch_weapons(
    mut loadout: Single<&mut Loadout, With<Player>>,
    controls: Controls,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
) {
    loadout.tick(time.delta_secs());
    if controls.just_pressed(Action::Weapon1) {
        loadout.switch_to(0);
    } else if controls.just_pressed(Action::Weapon2) {
        loadout.switch_to(1);
    } else if controls.just_pressed(Action::Weapon3) {
        loadout.switch_to(2);
    } else if controls.just_pressed(Action::SwitchWeapon) || scroll.delta.y != 0.0 {
        loadout.cycle();
    }
}

fn reload(
    player: Single<(&mut Loadout, &Modifiers), With<Player>>,
    controls: Controls,
    time: Res<Time>,
) {
    let (mut loadout, modifiers) = player.into_inner();
    if controls.just_pressed(Action::Reload) {
        loadout.start_reload(&modifiers.0);
    }
    loadout.advance_reload(time.delta_secs(), &modifiers.0);
    loadout.feed_from_reserve(&modifiers.0);
}

/// The Aim key aims down the sights.
fn aim(
    player: Single<(&mut Loadout, &Locomotion), With<Player>>,
    controls: Controls,
    time: Res<Time>,
) {
    let (mut loadout, locomotion) = player.into_inner();
    loadout.update_stance(time.delta_secs(), locomotion.sprinting, controls.pressed(Action::Aim));
}

fn fire(
    player: Single<(Entity, &mut Loadout, &Modifiers), With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    mut view_model: Single<&mut ViewModel>,
    controls: Controls,
    spatial: SpatialQuery,
    hit_areas: Query<(&HitArea, &ChildOf)>,
    mut damage: MessageWriter<DamageZombie>,
    mut ammo_mod_hits: MessageWriter<AmmoModHit>,
    mut ignite: MessageWriter<Ignite>,
    mut shots: MessageWriter<ShotFired>,
    mut launched: MessageWriter<Launched>,
    mut explosions: MessageWriter<Explosion>,
    mut wonders: MessageWriter<WonderFired>,
    mut flames: MessageWriter<FlameFired>,
    mut cues: MessageWriter<PlayCue>,
) {
    let (entity, mut loadout, modifiers) = player.into_inner();
    let entries = &modifiers.0;
    let (pressed, just_pressed) = (controls.pressed(Action::Fire), controls.just_pressed(Action::Fire));
    // Spread is read before the shot blooms it.
    let spread = loadout.spread();
    if !loadout.try_fire(pressed, just_pressed, entries) {
        // Pulling the trigger on an empty Magazine clicks, and starts a Reload.
        if just_pressed && loadout.gun_ammo().0 == 0 {
            cues.write(PlayCue { cue: Cue::EmptyClick, at: None });
            loadout.start_reload(entries);
        }
        return;
    }
    view_model.kick = 1.0;

    // The gun that fired: the held Weapon, or its Underbarrel.
    let stats = loadout.gun_stats();
    let underbarrel = loadout.underbarrel().map(|underbarrel| underbarrel.kind);
    if underbarrel == Some(Underbarrel::Flamethrower) {
        // Each tick burns everything in the cone.
        flames.write(FlameFired {
            origin: camera.translation(),
            forward: camera.forward(),
            damage: loadout.damage_at(0.0, entries),
            burn: Burn {
                damage: loadout.resolve(Stat::Damage, FLAMETHROWER_BURN.damage, entries),
                ..FLAMETHROWER_BURN
            },
        });
        return;
    }
    shots.write(ShotFired {
        weapon: loadout.held().id,
        upgraded: loadout.held().upgraded,
        underbarrel,
    });

    // Every damage event from this Weapon rolls its Ammo Mods.
    let ammo_mods = loadout.held().ammo_mods;
    let mut roll = |zombie, damage| {
        if !ammo_mods.is_empty() {
            ammo_mod_hits.write(AmmoModHit { zombie, damage, ammo_mods });
        }
    };
    // Incendiary Weapons set every Zombie they hit alight.
    let burn = stats.burn.map(|burn| Burn {
        damage: loadout.resolve(Stat::Damage, burn.damage, entries),
        ..burn
    });
    // The Thundergun, Crossbow and Ballistic Knife fire their own way.
    if is_wonder(stats.class) {
        wonders.write(WonderFired {
            origin: camera.translation(),
            direction: spread_direction(camera.forward(), spread, rand::random(), rand::random()),
            stats,
            upgraded: loadout.held().upgraded,
            damage: loadout.damage_at(0.0, entries),
            splash: stats.splash.map(|splash| Splash {
                max: loadout.resolve(Stat::Damage, splash.max, entries),
                min: loadout.resolve(Stat::Damage, splash.min, entries),
                ..splash
            }),
        });
    }
    let filter = attack_filter(entity);
    let sights = loadout.sights(entries);
    let pellets = if is_wonder(stats.class) { 0 } else { stats.pellets };
    for _ in 0..pellets {
        let direction = spread_direction(camera.forward(), spread, rand::random(), rand::random());
        // Splash resolves through the seam like any damage: both ends of its falloff.
        let splash = stats.splash.map(|splash| Splash {
            max: loadout.resolve(Stat::Damage, splash.max, entries),
            min: loadout.resolve(Stat::Damage, splash.min, entries),
            ..splash
        });
        let flight = match underbarrel {
            Some(underbarrel) => underbarrel.flight(),
            None => flight(stats.id),
        };
        if let (Some(flight), Some(splash)) = (flight, splash) {
            // Launchers fire a real projectile, which bursts on the first thing it touches.
            launched.write(Launched {
                origin: camera.translation() + *direction * 0.5,
                direction,
                flight,
                impact: loadout.damage_at(0.0, entries),
                impact_headshot: loadout.zone_multiplier(HitZone::Head, entries),
                // The Grenade Launcher never scores a Headshot.
                headshots: underbarrel.is_none(),
                splash,
                ammo_mods,
                // Only the Grenade Launcher flies among the underbarrels.
                crawlers: makes_crawlers(match underbarrel {
                    Some(_) => Explosive::GrenadeLauncher,
                    None => Explosive::Weapon(stats.id),
                }),
            });
            continue;
        }
        if let Some(splash) = splash {
            // The other explosives burst where their hitscan line lands.
            let Some(hit) = spatial.cast_ray(camera.translation(), direction, HITSCAN_RANGE, true, &filter) else {
                continue;
            };
            if let Some((zombie, zone)) = zombie_for(hit.entity, &hit_areas) {
                let amount = loadout.damage_at(hit.distance, entries);
                damage.write(DamageZombie {
                    zombie,
                    damage: amount,
                    zone,
                    attack: Attack::Bullet,
                });
                roll(zombie, amount);
            }
            // Burst just short of the surface, so the world doesn't block its own blast.
            let impact = camera.translation() + *direction * (hit.distance - 0.1).max(0.0);
            explosions.write(Explosion {
                at: impact,
                splash,
                hurts_player: true,
                ammo_mods,
                // Only the Grenade Launcher flies among the underbarrels.
                crawlers: makes_crawlers(match underbarrel {
                    Some(_) => Explosive::GrenadeLauncher,
                    None => Explosive::Weapon(stats.id),
                }),
            });
            continue;
        }

        // Everything the bullet's line crosses, nearest first.
        let mut crossed = Vec::new();
        spatial.ray_hits_callback(camera.translation(), direction, HITSCAN_RANGE, true, &filter, |hit| {
            crossed.push(hit);
            true
        });
        crossed.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        let crossings = crossed.iter().map(|hit| match zombie_for(hit.entity, &hit_areas) {
            Some((zombie, zone)) => Crossing::Zombie {
                zombie,
                zone,
                distance: hit.distance,
            },
            None => Crossing::World,
        });
        for hit in pierce(crossings, loadout.penetration(entries) + sights.extra_pierce) {
            let amount = hit_damage(loadout.zone_multiplier(hit.zone, entries), loadout.damage_at(hit.distance, entries), &hit, sights);
            damage.write(DamageZombie {
                zombie: hit.zombie,
                damage: amount,
                zone: hit.zone,
                attack: Attack::Bullet,
            });
            roll(hit.zombie, amount);
            if let Some(burn) = burn {
                ignite.write(Ignite { zombie: hit.zombie, burn });
            }
        }
    }

    // Like the real thing: firing the last bullet starts a Reload.
    if loadout.gun_ammo().0 == 0 {
        loadout.start_reload(entries);
    }
}

fn knife(
    player: Single<(Entity, &mut Knife, &Loadout, &Modifiers), With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    controls: Controls,
    spatial: SpatialQuery,
    hit_areas: Query<(&HitArea, &ChildOf)>,
    mut damage: MessageWriter<DamageZombie>,
    mut cues: MessageWriter<PlayCue>,
    time: Res<Time>,
) {
    let (entity, mut knife, loadout, modifiers) = player.into_inner();
    knife.cooldown = (knife.cooldown - time.delta_secs()).max(0.0);
    if !controls.just_pressed(Action::Knife) || knife.cooldown > 0.0 {
        return;
    }
    knife.cooldown = KNIFE_COOLDOWN_SECS;
    cues.write(PlayCue { cue: Cue::KnifeSwing, at: None });

    if let Some(hit) = spatial.cast_shape(
        &Collider::sphere(KNIFE_RADIUS),
        camera.translation(),
        Quat::IDENTITY,
        camera.forward(),
        &ShapeCastConfig::from_max_distance(KNIFE_RANGE),
        &attack_filter(entity),
    ) && let Some((zombie, zone)) = zombie_for(hit.entity, &hit_areas)
    {
        let base = knife_damage(knife.bowie, ballistic_in_hand(loadout));
        damage.write(DamageZombie {
            zombie,
            damage: knife_hit(base, &modifiers.0, rand::random()),
            zone,
            attack: Attack::Knife,
        });
        cues.write(PlayCue { cue: Cue::KnifeHit, at: None });
    }
}

/// A Knife hit's damage after the Player's entries: their Knife damage, or a kill outright on a
/// `roll` in `[0, 1)` under their `Stat::KnifeKillChance`.
pub fn knife_hit(base: f32, player: &[Modifier], roll: f64) -> f32 {
    if roll < f64::from(resolve(Stat::KnifeKillChance, 0.0, player)) {
        f32::INFINITY
    } else {
        resolve(Stat::KnifeDamage, base, player)
    }
}

/// The Player's Weapon slots, and how much Reserve Ammo each Weapon carries, follow their stat
/// entries.
fn resolve_slots(mut player: Single<(&mut Loadout, &Modifiers), With<Player>>) {
    let (ref mut loadout, modifiers) = *player;
    let count = resolve_count(Stat::WeaponSlots, WEAPON_SLOTS, &modifiers.0) as usize;
    if loadout.slot_count() != count {
        loadout.set_slot_count(count);
    }
    loadout.cap_reserves(&modifiers.0);
}

/// Kills can put bullets back in the held Weapon's Magazine, through the modifier seam. A Trap's,
/// a Barrier's, the Teleporter's or a Nova Crawler blast's kill is the Map's, not the Player's.
fn refill_on_kill(
    mut kills: MessageReader<ZombieKilled>,
    player: Single<(&mut Loadout, &Modifiers), With<Player>>,
) {
    let (mut loadout, modifiers) = player.into_inner();
    for kill in kills.read() {
        if !matches!(kill.attack, Attack::Trap | Attack::Barrier | Attack::Teleporter | Attack::NovaBlast) {
            loadout.refill_on_kill(&modifiers.0);
        }
    }
}

/// `Some(upgraded)` while the Ballistic Knife is in hand, whose blade the melee uses.
fn ballistic_in_hand(loadout: &Loadout) -> Option<bool> {
    let held = loadout.held();
    (held.id == WeaponId::BallisticKnife).then_some(held.upgraded)
}

fn animate_view_model(
    mut view_model: Single<(&mut ViewModel, &mut Transform, &mut Mesh3d)>,
    mut glint: Single<(&mut Glint, &mut Transform, &mut Visibility), Without<ViewModel>>,
    mut parts: Query<
        (&ViewPart, &mut Transform, Option<&mut Mesh3d>, &mut Visibility),
        (Without<ViewModel>, Without<Glint>),
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player: Single<(&Loadout, &Modifiers, &Knife, Has<Drinking>), With<Player>>,
    guns: Res<GunModels>,
    time: Res<Time>,
) {
    let (ref mut model, ref mut transform, ref mut mesh) = *view_model;
    let (loadout, modifiers, knife, drinking) = *player;
    model.kick = (model.kick - time.delta_secs() * 8.0).max(0.0);

    // Drop the old gun and raise the new one on a switch.
    let mut shown_id = loadout.held().id;
    let mut class = loadout.held().stats().class;
    let mut shown = glint_of(loadout.held());
    let dip = if drinking {
        // Out of sight while the Perk is drunk.
        0.6
    } else if let Some(swap) = &loadout.swap {
        if swap.elapsed < swap.drop_secs {
            shown_id = swap.from.id;
            class = swap.from.class;
            shown = swap.from_glint;
            swap.elapsed / swap.drop_secs * 0.4
        } else {
            (1.0 - (swap.elapsed - swap.drop_secs) / swap.raise_secs) * 0.4
        }
    } else {
        0.0
    };
    let length = model_length(class);
    let gun = guns.view(shown_id);
    if mesh.0 != gun {
        mesh.0 = gun;
    }
    let magazine = guns.magazine(shown_id);
    let left_hand = guns.left_hand(shown_id);

    // A Reload: the magazine swapped, or the gun fed by hand.
    let reload = match (&loadout.reload, loadout.reload_secs(&modifiers.0)) {
        (Some(reload), Some(secs)) if loadout.swap.is_none() => {
            let hand_rest = left_hand.map_or(Vec3::ZERO, |hand| hand.rest.translation);
            let (style, grab) = match magazine.filter(|_| loadout.underbarrel().is_none()) {
                Some(magazine) => (ReloadStyle::Magazine, magazine.rest.translation + Vec3::new(-0.03, -0.06, 0.0) - hand_rest),
                // The loading port, under the receiver.
                None => (ReloadStyle::Feed, Vec3::new(0.0, -0.07, 0.04) - hand_rest),
            };
            reload_pose(style, reload.elapsed, secs, grab)
        }
        _ => ReloadPose::REST,
    };

    // The Knife: the Ballistic Knife in hand stabs with its own blade; otherwise the Weapon drops
    // out of the way of the knife hand.
    let stabbing = ballistic_in_hand(loadout).is_some();
    let (knife_dip, knife_hand) = match knife.swing() {
        Some(t) if !stabbing => knife_pose(t),
        _ => (0.0, None),
    };
    let lunge = knife.swing().filter(|_| stabbing).map_or(0.0, stab_lunge);

    transform.translation = Vec3::new(
        0.22 + 0.1 * knife_dip,
        -0.2 - dip - 0.3 * knife_dip,
        -0.45 - 0.15 * (length - 1.0) + model.kick * 0.05 - lunge,
    ) + reload.shift;
    transform.rotation = Quat::from_rotation_x(model.kick * 0.25 - dip * 2.0)
        * Quat::from_rotation_z(-0.6 * knife_dip)
        * reload.turn;

    for (part, mut part_transform, part_mesh, mut visibility) in &mut parts {
        let (shown_mesh, placed) = match part {
            ViewPart::Stretch => {
                part_transform.scale = Vec3::new(1.0, 1.0, length);
                continue;
            }
            ViewPart::Magazine => (
                magazine.map(|magazine| magazine.mesh.clone()),
                magazine.map(|magazine| magazine.rest.with_translation(magazine.rest.translation + reload.magazine)),
            ),
            ViewPart::LeftHand => (
                left_hand.map(|hand| hand.mesh.clone()),
                left_hand.map(|hand| hand.rest.with_translation(hand.rest.translation + reload.hand)),
            ),
            ViewPart::Knife => (
                Some(if knife.bowie { guns.bowie.clone() } else { guns.knife.clone() }),
                knife_hand,
            ),
        };
        match (shown_mesh, placed) {
            (Some(shown_mesh), Some(placed)) => {
                if let Some(mut part_mesh) = part_mesh
                    && part_mesh.0 != shown_mesh
                {
                    part_mesh.0 = shown_mesh;
                }
                *part_transform = placed;
                visibility.set_if_neq(Visibility::Inherited);
            }
            _ => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }

    // The glint sweeps from the back of the model to the muzzle, over and over.
    let (ref mut glint, ref mut glint_transform, ref mut visibility) = *glint;
    visibility.set_if_neq(if shown.is_some() { Visibility::Inherited } else { Visibility::Hidden });
    if let Some(ammo_mods) = shown {
        let colour = glint_colour(ammo_mods, glint.pass, UPGRADED_PURPLE);
        if glint.colour != colour
            && let Some(mut material) = materials.get_mut(&glint.material)
        {
            material.base_color = colour;
            material.emissive = LinearRgba::from(colour) * 8.0;
            glint.colour = colour;
        }
        let swept = glint.sweep + time.delta_secs() / GLINT_SWEEP_SECS;
        glint.pass = glint.pass.wrapping_add(swept as u32);
        glint.sweep = swept.fract();
        glint_transform.translation.z = 0.15 - 0.3 * glint.sweep;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: &[Modifier] = &[];

    /// Ticks the Loadout and any Reload for this long.
    fn run(loadout: &mut Loadout, secs: f32, player: &[Modifier]) {
        loadout.tick(secs);
        loadout.advance_reload(secs, player);
    }

    #[test]
    fn the_bowie_knife_does_1000_against_the_knifes_150() {
        assert_eq!(knife_damage(false, None), 150.0);
        assert_eq!(knife_damage(true, None), 1000.0);
    }

    #[test]
    fn the_ballistic_knife_in_hand_stabs_for_500_or_1000_with_the_bowie() {
        assert_eq!(knife_damage(false, Some(false)), 500.0);
        assert_eq!(knife_damage(true, Some(false)), 1000.0);
        // The Krauss Refibrillator: 1000, or 1500 with the Bowie.
        assert_eq!(knife_damage(false, Some(true)), 1000.0);
        assert_eq!(knife_damage(true, Some(true)), 1500.0);
    }

    #[test]
    fn a_picked_up_blade_adds_a_bullet_until_the_reserve_is_full() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        assert!(!loadout.pick_up_bullet(WeaponId::BallisticKnife), "not carried");
        loadout.give(WeaponId::BallisticKnife);
        loadout.held_mut().reserve = 3;
        assert!(loadout.pick_up_bullet(WeaponId::BallisticKnife));
        assert_eq!(loadout.held().reserve, 4);
        assert!(!loadout.pick_up_bullet(WeaponId::BallisticKnife), "full at 4");
    }

    #[test]
    fn the_player_starts_with_the_m1911_at_bo1_ammo() {
        let loadout = Loadout::new(STARTING_WEAPON);
        let held = loadout.held();
        assert_eq!(held.id, WeaponId::M1911);
        assert_eq!((held.magazine, held.reserve), (8, 32));
        assert_eq!(loadout.damage_at(0.0, NONE), 20.0);
        assert!(loadout.slot(1).is_none());
    }

    #[test]
    fn a_new_weapon_fills_the_empty_slot_then_replaces_the_one_in_hand() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.give(WeaponId::Mp40);
        assert_eq!(loadout.held().id, WeaponId::Mp40);
        assert_eq!(loadout.slot(0).unwrap().id, WeaponId::M1911);

        run(&mut loadout, 5.0, NONE);
        loadout.give(WeaponId::M14);
        assert_eq!(loadout.held().id, WeaponId::M14);
        assert_eq!(loadout.slot(0).unwrap().id, WeaponId::M1911);
        assert_eq!(loadout.slot(1).unwrap().id, WeaponId::M14);
    }

    #[test]
    fn a_switch_takes_the_old_drop_plus_the_new_raise() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.give(WeaponId::Mp40);
        run(&mut loadout, 5.0, NONE);

        // SMG drop 0.5 + pistol raise 0.55.
        assert!(loadout.switch_to(0));
        assert!((loadout.swap.as_ref().unwrap().secs() - 1.05).abs() < 1e-5);
        run(&mut loadout, 1.0, NONE);
        assert!(!loadout.try_fire(true, true, NONE), "can't fire mid-switch");
        run(&mut loadout, 0.06, NONE);
        assert!(loadout.swap.is_none());
        assert!(loadout.try_fire(true, true, NONE));
    }

    #[test]
    fn switching_needs_another_weapon() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        assert!(!loadout.cycle());
        assert!(!loadout.switch_to(0));
        assert!(!loadout.switch_to(1));
    }

    #[test]
    fn a_switch_cancels_a_reload_and_loses_its_progress() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.give(WeaponId::Mp40);
        run(&mut loadout, 5.0, NONE);
        loadout.switch_to(0);
        run(&mut loadout, 5.0, NONE);

        loadout.held_mut().magazine = 3;
        assert!(loadout.start_reload(NONE));
        run(&mut loadout, 1.5, NONE);
        assert!(loadout.cycle());
        assert!(loadout.reload.is_none());
        run(&mut loadout, 5.0, NONE);
        loadout.cycle();
        run(&mut loadout, 5.0, NONE);
        assert_eq!(loadout.held().magazine, 3, "the cancelled Reload loaded nothing");

        // A new Reload starts from scratch.
        loadout.start_reload(NONE);
        run(&mut loadout, 1.5, NONE);
        assert_eq!(loadout.held().magazine, 3);
        run(&mut loadout, 0.2, NONE);
        assert_eq!((loadout.held().magazine, loadout.held().reserve), (8, 27));
    }

    #[test]
    fn reloads_take_tactical_or_empty_time() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.held_mut().magazine = 0;
        loadout.start_reload(NONE);
        assert_eq!(loadout.reload_secs(NONE), Some(1.8));
        run(&mut loadout, 1.8, NONE);

        loadout.held_mut().magazine = 7;
        loadout.start_reload(NONE);
        assert_eq!(loadout.reload_secs(NONE), Some(1.63));
    }

    #[test]
    fn reserve_ammo_runs_out() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.held_mut().magazine = 0;
        loadout.held_mut().reserve = 3;
        loadout.start_reload(NONE);
        run(&mut loadout, 2.0, NONE);
        assert_eq!((loadout.held().magazine, loadout.held().reserve), (3, 0));

        loadout.held_mut().magazine = 0;
        assert!(!loadout.start_reload(NONE), "nothing left to load");
    }

    #[test]
    fn a_full_magazine_doesnt_reload() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        assert!(!loadout.start_reload(NONE));
    }

    #[test]
    fn reload_time_is_resolved_every_frame() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.held_mut().magazine = 0;
        loadout.start_reload(NONE);
        run(&mut loadout, 1.0, NONE);

        // An entry arriving mid-Reload takes effect at once: 1.0 s in, a 0.9 s Reload is done.
        let faster = [Modifier {
            stat: Stat::ReloadTime,
            source: Source("test"),
            effect: Effect::Mult(0.5),
        }];
        run(&mut loadout, 0.0, &faster);
        assert!(loadout.reload.is_none());
        assert_eq!(loadout.held().magazine, 8);
    }

    #[test]
    fn magazine_size_resolves_through_the_seam() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        let bigger = [Modifier {
            stat: Stat::MagazineSize,
            source: Source("test"),
            effect: Effect::Mult(1.5),
        }];
        assert_eq!(loadout.magazine_size(&bigger), 12);
        assert!(loadout.start_reload(&bigger));
        run(&mut loadout, 2.0, &bigger);
        assert_eq!((loadout.held().magazine, loadout.held().reserve), (12, 28));
    }

    #[test]
    fn only_the_head_multiplier_resolves_through_the_seam() {
        let loadout = Loadout::new(WeaponId::M1911);
        let sharper = [Modifier {
            stat: Stat::HeadshotMultiplier,
            source: Source("test"),
            effect: Effect::Flat(0.5),
        }];
        assert_eq!(loadout.zone_multiplier(HitZone::Head, NONE), 3.5);
        assert_eq!(loadout.zone_multiplier(HitZone::Head, &sharper), 4.0);
        assert_eq!(loadout.zone_multiplier(HitZone::Torso, &sharper), 1.0);
    }

    #[test]
    fn a_kill_puts_bullets_back_only_with_an_entry_and_never_past_full() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.held_mut().magazine = 6;
        loadout.refill_on_kill(NONE);
        assert_eq!(loadout.held().magazine, 6);

        let one = [Modifier {
            stat: Stat::MagazinePerKill,
            source: Source("test"),
            effect: Effect::Flat(1.0),
        }];
        loadout.refill_on_kill(&one);
        assert_eq!(loadout.held().magazine, 7);
        loadout.refill_on_kill(&one);
        loadout.refill_on_kill(&one);
        assert_eq!(loadout.held().magazine, 8);
        // The Reserve is untouched: the bullet comes from nowhere.
        assert_eq!(loadout.held().reserve, WeaponId::M1911.stats().starting_reserve());
    }

    #[test]
    fn an_extra_slot_carries_a_third_weapon_on_key_3() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        assert_eq!(loadout.slot_count(), 2);
        loadout.set_slot_count(3);
        loadout.give(WeaponId::Mp40);
        loadout.give(WeaponId::M14);
        // The third Weapon fills the new slot rather than replacing the one in hand.
        assert_eq!(loadout.weapons().map(|weapon| weapon.id).collect::<Vec<_>>(), [
            WeaponId::M1911,
            WeaponId::Mp40,
            WeaponId::M14
        ]);
        assert_eq!(loadout.held().id, WeaponId::M14);
        run(&mut loadout, 5.0, NONE);
        assert!(loadout.switch_to(0));
        run(&mut loadout, 5.0, NONE);
        assert!(loadout.switch_to(2));
        run(&mut loadout, 5.0, NONE);
        // Cycling wraps round all three.
        assert!(loadout.cycle());
        assert_eq!(loadout.held().id, WeaponId::M1911);
        run(&mut loadout, 5.0, NONE);
        assert!(loadout.cycle());
        assert_eq!(loadout.held().id, WeaponId::Mp40);
    }

    #[test]
    fn cycling_skips_empty_slots_and_pack_a_punch_takes_from_any() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.set_slot_count(3);
        loadout.give(WeaponId::Mp40);
        run(&mut loadout, 5.0, NONE);
        assert!(loadout.cycle());
        assert_eq!(loadout.held().id, WeaponId::M1911);
        assert!(loadout.has_spare());
        let m1911 = loadout.take_held().unwrap();
        assert_eq!(m1911.id, WeaponId::M1911);
        assert_eq!(loadout.held().id, WeaponId::Mp40);
        assert!(!loadout.has_spare());
    }

    #[test]
    fn losing_a_slot_keeps_weapons_while_there_is_room() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.set_slot_count(3);
        loadout.give(WeaponId::Mp40);
        loadout.give(WeaponId::M14);
        run(&mut loadout, 5.0, NONE);
        // An empty slot that stays takes the Weapon from the slot that goes.
        let m1911 = loadout.slots[0].take();
        loadout.set_slot_count(2);
        assert_eq!(loadout.slot(0).unwrap().id, WeaponId::M14);
        assert_eq!(loadout.held().id, WeaponId::M14);
        // With every slot that stays filled, the Weapon in the slot that goes is dropped, and the
        // first slot's comes up.
        loadout.slots[0] = m1911;
        loadout.set_slot_count(3);
        loadout.give(WeaponId::M14);
        assert_eq!(loadout.held().id, WeaponId::M14);
        loadout.set_slot_count(2);
        assert_eq!(loadout.slot_count(), 2);
        assert_eq!(loadout.held().id, WeaponId::M1911);
        assert_eq!(loadout.slot_of(WeaponId::M14), None);
    }

    #[test]
    fn penetration_entries_add_zombies_on_top_of_the_tier() {
        let mp40 = Loadout::new(WeaponId::Mp40);
        let tier = WeaponId::Mp40.stats().penetration.extra_zombies();
        assert_eq!(mp40.penetration(NONE), tier);
        let one = [Modifier {
            stat: Stat::Penetration,
            source: Source("test"),
            effect: Effect::Flat(1.0),
        }];
        assert_eq!(mp40.penetration(&one), tier + 1);
    }

    #[test]
    fn semi_fires_per_click_and_auto_while_held() {
        let mut pistol = Loadout::new(WeaponId::M1911);
        assert!(pistol.try_fire(true, true, NONE));
        run(&mut pistol, 1.0, NONE);
        assert!(!pistol.try_fire(true, false, NONE), "holding doesn't fire a semi-auto");

        let mut smg = Loadout::new(WeaponId::Mp40);
        assert!(smg.try_fire(true, true, NONE));
        assert!(!smg.try_fire(true, false, NONE), "too soon after the last shot");
        let interval = smg.fire_interval(NONE);
        run(&mut smg, interval, NONE);
        assert!(smg.try_fire(true, false, NONE));
        assert_eq!(smg.held().magazine, 30);
    }

    #[test]
    fn a_burst_fires_its_shots_from_one_click() {
        let mut g11 = Loadout::new(WeaponId::G11);
        let interval = g11.fire_interval(NONE);
        assert!(g11.try_fire(true, true, NONE));
        for _ in 0..4 {
            run(&mut g11, interval, NONE);
            g11.try_fire(false, false, NONE);
        }
        assert_eq!(g11.held().magazine, 45);
    }

    #[test]
    fn an_ammo_buy_fills_the_magazine_and_the_reserve() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.give(WeaponId::M14);
        assert_eq!(loadout.slot_of(WeaponId::M14), Some(1));
        assert_eq!(loadout.slot_of(WeaponId::Mp40), None);

        // A fresh M1911 has full Magazine but not full Reserve Ammo.
        assert!(!loadout.ammo_full(0, NONE));
        loadout.refill(0, NONE);
        let m1911 = loadout.slot(0).unwrap();
        assert_eq!((m1911.magazine, m1911.reserve), (8, 80));
        assert!(loadout.ammo_full(0, NONE));
    }

    #[test]
    fn ammo_is_full_only_when_both_magazine_and_reserve_are() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.refill(0, NONE);
        loadout.held_mut().magazine -= 1;
        assert!(!loadout.ammo_full(0, NONE), "one bullet short in the Magazine");
        loadout.refill(0, NONE);
        loadout.held_mut().reserve -= 1;
        assert!(!loadout.ammo_full(0, NONE), "one bullet short in the Reserve");
    }

    #[test]
    fn max_ammo_fills_the_reserve_of_every_weapon() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.give(WeaponId::M14);
        for weapon in loadout.slots.iter_mut().flatten() {
            weapon.magazine = 1;
            weapon.reserve = 0;
        }
        loadout.max_ammo();
        for slot in 0..2 {
            let weapon = loadout.slot(slot).unwrap();
            assert_eq!(weapon.reserve, weapon.stats().reserve_max);
            assert_eq!(weapon.magazine, 1, "the Magazine is left alone");
        }
    }

    #[test]
    fn a_knife_kill_chance_entry_makes_the_knife_kill() {
        let always = [Modifier { stat: Stat::KnifeKillChance, source: Source("A"), effect: Effect::Chance(1.0, 1) }];
        assert_eq!(knife_hit(KNIFE_DAMAGE, NONE, 0.0), KNIFE_DAMAGE);
        assert_eq!(knife_hit(KNIFE_DAMAGE, &always, 0.999), f32::INFINITY);
    }

    #[test]
    fn a_reserve_entry_on_the_player_caps_every_weapons_reserve() {
        let half = [Modifier { stat: Stat::ReserveSize, source: Source("A"), effect: Effect::Mult(0.5) }];
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.refill(0, NONE);
        assert_eq!(loadout.held().reserve, 80);
        loadout.cap_reserves(&half);
        assert_eq!(loadout.held().reserve, 40);
        assert!(loadout.ammo_full(0, &half), "full at the lower cap");
        loadout.max_ammo();
        loadout.cap_reserves(&half);
        assert_eq!(loadout.held().reserve, 40);
    }

    #[test]
    fn a_reserve_feed_entry_fires_straight_from_reserve_with_no_reload() {
        let feed = [Modifier { stat: Stat::ReserveFeed, source: Source("A"), effect: Effect::Flat(1.0) }];
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.held_mut().magazine = 3;
        loadout.feed_from_reserve(NONE);
        assert_eq!((loadout.held().magazine, loadout.held().reserve), (3, 32), "Classic waits for a Reload");
        loadout.feed_from_reserve(&feed);
        assert_eq!((loadout.held().magazine, loadout.held().reserve), (8, 27));
        assert!(!loadout.start_reload(&feed), "a full Magazine never Reloads");
        loadout.held_mut().reserve = 2;
        loadout.held_mut().magazine = 0;
        loadout.feed_from_reserve(&feed);
        assert_eq!((loadout.held().magazine, loadout.held().reserve), (2, 0));
    }

    #[test]
    fn refilling_the_weapon_in_hand_ends_its_reload() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.held_mut().magazine = 2;
        assert!(loadout.start_reload(NONE));
        loadout.refill(0, NONE);
        assert!(loadout.reload.is_none());
        assert_eq!((loadout.held().magazine, loadout.held().reserve), (8, 80));
    }

    #[test]
    fn a_weapon_buy_cancels_a_reload_and_comes_with_full_ammo() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.held_mut().magazine = 2;
        assert!(loadout.start_reload(NONE));
        loadout.give(WeaponId::M14);
        assert!(loadout.reload.is_none());
        let m14 = loadout.held();
        assert_eq!((m14.magazine, m14.reserve), (m14.stats().magazine, m14.stats().reserve_max));
    }

    #[test]
    fn handing_over_the_weapon_in_hand_frees_its_slot() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        assert!(loadout.take_held().is_none(), "the Player's only Weapon stays in hand");
        loadout.give(WeaponId::M14);
        run(&mut loadout, 5.0, NONE);

        let m14 = loadout.take_held().expect("the M1911 is left to bring up");
        assert_eq!(m14.id, WeaponId::M14);
        assert_eq!(loadout.held().id, WeaponId::M1911);
        assert!(loadout.swap.is_some());
        assert!(loadout.slot(1).is_none() && !loadout.has_spare());

        // A Wall Weapon bought meanwhile fills the free slot.
        loadout.give(WeaponId::Mp40);
        assert_eq!(loadout.slot(1).unwrap().id, WeaponId::Mp40);
    }

    #[test]
    fn an_upgraded_copy_comes_back_in_place_of_the_same_weapon() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.give(WeaponId::M14);
        let m14 = loadout.take_held().unwrap();
        loadout.give(WeaponId::M14);
        loadout.give_copy(m14.upgrade());
        assert_eq!(loadout.slot(1).unwrap().stats().name, "Mnesia");
        assert_eq!(loadout.slot(0).unwrap().id, WeaponId::M1911);
        assert_eq!(loadout.held().stats().name, "Mnesia");
    }

    #[test]
    fn an_upgraded_copy_uses_its_upgraded_stats() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.give_copy(WeaponCopy::new(WeaponId::M14).upgrade());
        assert_eq!(loadout.damage_at(0.0, NONE), 200.0);
        assert_eq!(loadout.magazine_size(NONE), 16);
        assert!(loadout.ammo_full(1, NONE));
    }

    #[test]
    fn an_empty_magazine_doesnt_fire() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.held_mut().magazine = 0;
        assert!(!loadout.try_fire(true, true, NONE));
    }

    #[test]
    fn sprint_out_holds_fire_after_a_sprint() {
        let mut pistol = Loadout::new(WeaponId::M1911);
        pistol.update_stance(0.1, true, false);
        assert!(!pistol.try_fire(true, true, NONE), "no firing mid-sprint");
        pistol.update_stance(0.25, false, false);
        assert!(!pistol.try_fire(true, true, NONE), "0.25 s into a 0.3 s sprint-out");
        pistol.update_stance(0.06, false, false);
        assert!(pistol.try_fire(true, true, NONE));

        // LMGs take 0.45 s.
        let mut lmg = Loadout::new(WeaponId::Rpk);
        lmg.update_stance(0.1, true, false);
        lmg.update_stance(0.4, false, false);
        assert!(!lmg.try_fire(true, true, NONE));
        lmg.update_stance(0.06, false, false);
        assert!(lmg.try_fire(true, true, NONE));
    }

    #[test]
    fn aiming_narrows_the_fov_and_tightens_the_spread() {
        let mut smg = Loadout::new(WeaponId::Mp40);
        assert_eq!((smg.fov_factor(), smg.spread()), (1.0, 2.0));
        smg.update_stance(1.0, false, true);
        assert_eq!(smg.aim, 1.0);
        assert!((smg.fov_factor() - 0.77).abs() < 1e-5);
        assert_eq!(smg.spread(), 0.0);

        let mut sniper = Loadout::new(WeaponId::Dragunov);
        sniper.update_stance(1.0, false, true);
        assert!((sniper.fov_factor() - 0.46).abs() < 1e-5);

        // The G11 has a scope; the Olympia still spreads aimed.
        let mut g11 = Loadout::new(WeaponId::G11);
        g11.update_stance(1.0, false, true);
        assert!((g11.fov_factor() - 0.46).abs() < 1e-5);
        let mut olympia = Loadout::new(WeaponId::Olympia);
        olympia.update_stance(1.0, false, true);
        assert_eq!(olympia.spread(), 5.5);
    }

    #[test]
    fn some_weapons_cant_aim_or_zoom() {
        let mut dual = Loadout::new(WeaponId::Cz75DualWield);
        dual.update_stance(1.0, false, true);
        assert_eq!((dual.aim, dual.fov_factor(), dual.spread()), (0.0, 1.0, 4.0));

        // Pistols aim without zooming.
        let mut pistol = Loadout::new(WeaponId::M1911);
        pistol.update_stance(1.0, false, true);
        assert_eq!((pistol.aim, pistol.fov_factor(), pistol.spread()), (1.0, 1.0, 0.0));
    }

    #[test]
    fn sprinting_and_reloading_drop_the_sights() {
        let mut smg = Loadout::new(WeaponId::Mp40);
        smg.update_stance(1.0, false, true);
        smg.update_stance(1.0, true, true);
        assert_eq!(smg.aim, 0.0);

        let mut smg = Loadout::new(WeaponId::Mp40);
        smg.held_mut().magazine = 10;
        smg.start_reload(NONE);
        smg.update_stance(1.0, false, true);
        assert_eq!(smg.aim, 0.0);
    }

    #[test]
    fn shots_bloom_the_hip_spread_and_it_settles() {
        let mut smg = Loadout::new(WeaponId::Mp40);
        let interval = smg.fire_interval(NONE);
        for _ in 0..10 {
            smg.try_fire(true, false, NONE);
            run(&mut smg, interval, NONE);
        }
        assert_eq!(smg.spread(), 5.0, "fully bloomed");
        smg.update_stance(SPREAD_RECOVERY_SECS, false, false);
        assert_eq!(smg.spread(), 2.0);
    }

    #[test]
    fn move_speed_goes_through_the_seam_by_class() {
        let lmg = Loadout::new(WeaponId::Rpk);
        assert!((lmg.move_speed(10.0, NONE) - 8.5).abs() < 1e-5);
        assert_eq!(Loadout::new(WeaponId::Mp40).move_speed(10.0, NONE), 10.0);

        // Another source multiplies with the class factor.
        let quicker = [Modifier {
            stat: Stat::MoveSpeed,
            source: Source("test"),
            effect: Effect::Mult(1.2),
        }];
        let rifle = Loadout::new(WeaponId::M14);
        assert!((rifle.move_speed(10.0, &quicker) - 10.0 * 0.95 * 1.2).abs() < 1e-4);
    }

    /// The copy a hold Repack makes when its rolls land on `attachment`.
    fn fit(copy: WeaponCopy, attachment: Attachment) -> WeaponCopy {
        let rolls = (0..100).map(|i| i as f32 / 100.0);
        let free = free_mounts(copy.stats(), &copy.attachments);
        let (mount_roll, attachment_roll) = rolls
            .clone()
            .flat_map(|a| rolls.clone().map(move |b| (a, b)))
            .find(|(a, b)| pick_attachment(&free, *a, *b) == Some(attachment))
            .expect("a free mount for it");
        copy.repack_attachment(mount_roll, attachment_roll)
    }

    fn upgraded(id: WeaponId, attachments: &[Attachment]) -> Loadout {
        let mut loadout = Loadout::new(WeaponId::M1911);
        let copy = attachments.iter().fold(WeaponCopy::new(id).upgrade(), |copy, a| fit(copy, *a));
        loadout.give_copy(copy);
        run(&mut loadout, 5.0, NONE);
        loadout
    }

    const SPEED_COLA: [Modifier; 1] = [Modifier {
        stat: Stat::ReloadTime,
        source: Source("Speed Cola"),
        effect: Effect::Mult(SPEED_COLA_RELOAD_MULT),
    }];

    #[test]
    fn a_hold_repack_fits_one_attachment_per_mount_and_counts_as_a_repack() {
        let copy = fit(WeaponCopy::new(WeaponId::Mp40).upgrade(), Attachment::RapidFire);
        assert_eq!((copy.attachments.as_slice(), copy.repacks), ([Attachment::RapidFire].as_slice(), 1));
        // The barrel is taken, so a Long Barrel can never follow.
        assert!(!free_mounts(copy.stats(), &copy.attachments).contains(&crate::attachment::Mount::Barrel));
        let full = [
            Attachment::RedDotSight,
            Attachment::ExtendedMag,
            Attachment::Flamethrower,
            Attachment::Grip,
        ]
        .into_iter()
        .fold(copy, fit);
        assert!(!full.has_free_mount());
        let unchanged = full.repack_attachment(0.5, 0.5);
        assert_eq!((unchanged.attachments.len(), unchanged.repacks), (5, 5));
        // A base Weapon takes no Attachments.
        assert!(!WeaponCopy::new(WeaponId::Mp40).has_free_mount());
    }

    #[test]
    fn rapid_fire_shortens_the_fire_interval_alongside_double_tap() {
        let loadout = upgraded(WeaponId::Mp40, &[Attachment::RapidFire]);
        let double_tap = [Modifier {
            stat: Stat::FireInterval,
            source: Source("Double Tap"),
            effect: Effect::Mult(DOUBLE_TAP_FIRE_INTERVAL_MULT),
        }];
        let base = WeaponId::Mp40.upgraded().fire_interval;
        assert!((loadout.fire_interval(&double_tap) - base * 0.6).abs() < 1e-6);
    }

    #[test]
    fn extended_mag_comes_full_and_counts_toward_full_ammo() {
        let mut loadout = upgraded(WeaponId::M14, &[Attachment::ExtendedMag]);
        // Mnesia's 16 + 192 become 24 + 240.
        assert_eq!((loadout.held().magazine, loadout.held().reserve), (24, 240));
        assert_eq!(loadout.magazine_size(NONE), 24);
        assert!(loadout.ammo_full(1, NONE));
        loadout.held_mut().reserve = 200;
        assert!(!loadout.ammo_full(1, NONE));
        loadout.refill(1, NONE);
        assert_eq!(loadout.held().reserve, 240);
    }

    #[test]
    fn long_barrel_keeps_max_damage_at_any_range() {
        let plain = upgraded(WeaponId::Mp40, &[]);
        let long = upgraded(WeaponId::Mp40, &[Attachment::LongBarrel]);
        assert_eq!(plain.damage_at(100.0, NONE), 120.0);
        assert_eq!(long.damage_at(100.0, NONE), 200.0);
        assert_eq!(long.damage_at(0.0, NONE), 200.0);
    }

    #[test]
    fn grip_tightens_the_hip_spread() {
        let loadout = upgraded(WeaponId::Mp40, &[Attachment::Grip]);
        assert!((loadout.spread() - 2.0 * 0.6).abs() < 1e-5);
    }

    #[test]
    fn optics_count_only_while_aimed() {
        let mut loadout = upgraded(WeaponId::Mp40, &[Attachment::AcogSight]);
        assert_eq!(loadout.sights(NONE), Sights::PLAIN, "from the hip");
        loadout.update_stance(1.0, false, true);
        let aimed = loadout.sights(NONE);
        assert_eq!((aimed.extra_pierce, aimed.pierce_damage), (2, 1.0));
        assert!((loadout.fov_factor() - ADS_FOV_IRON * ACOG_ADS_FOV_MULT).abs() < 1e-5);

        let mut red_dot = upgraded(WeaponId::Mp40, &[Attachment::RedDotSight]);
        red_dot.update_stance(1.0, false, true);
        assert_eq!(red_dot.sights(NONE).headshot, 1.5);
    }

    #[test]
    fn dual_mag_makes_every_second_reload_a_flat_0_4_s() {
        assert!(!dual_mag_flat(0) && dual_mag_flat(1) && !dual_mag_flat(2));
        let mut loadout = upgraded(WeaponId::Mp40, &[Attachment::DualMag]);
        loadout.held_mut().magazine = 10;
        loadout.start_reload(&SPEED_COLA);
        assert!((loadout.reload_secs(&SPEED_COLA).unwrap() - 2.3 * SPEED_COLA_RELOAD_MULT).abs() < 1e-5);
        run(&mut loadout, 5.0, &SPEED_COLA);

        // The second: flat, and Speed Cola doesn't shorten it.
        loadout.held_mut().magazine = 10;
        loadout.start_reload(&SPEED_COLA);
        assert_eq!(loadout.reload_secs(&SPEED_COLA), Some(DUAL_MAG_RELOAD_SECS));
        run(&mut loadout, DUAL_MAG_RELOAD_SECS, &SPEED_COLA);
        assert_eq!(loadout.held().magazine, 64);

        // And the third is ordinary again.
        loadout.held_mut().magazine = 10;
        loadout.start_reload(NONE);
        assert_eq!(loadout.reload_secs(NONE), Some(2.3));
    }

    #[test]
    fn the_aug_50m3_comes_with_its_masterkey() {
        let aug = WeaponCopy::new(WeaponId::Aug).upgrade();
        let masterkey = aug.underbarrel.expect("built in");
        assert_eq!((masterkey.kind, masterkey.magazine, masterkey.reserve), (Underbarrel::Masterkey, 6, 30));
        assert!(WeaponCopy::new(WeaponId::Famas).upgrade().underbarrel.is_none());
        assert!(WeaponCopy::new(WeaponId::Aug).underbarrel.is_none(), "only once Upgraded");
    }

    #[test]
    fn x_toggles_the_underbarrel_in_half_a_second() {
        let mut loadout = upgraded(WeaponId::Aug, &[]);
        assert!(loadout.toggle_underbarrel());
        assert_eq!(loadout.gun_stats().name, "Masterkey");
        assert!(!loadout.try_fire(true, true, NONE), "mid-toggle");
        assert!(!loadout.toggle_underbarrel(), "one toggle at a time");
        run(&mut loadout, UNDERBARREL_TOGGLE_SECS, NONE);

        // The Masterkey spends its own shells, not the AUG-50M3's bullets.
        assert!(loadout.try_fire(true, true, NONE));
        assert_eq!(loadout.gun_ammo(), (5, 30));
        assert_eq!(loadout.held().magazine, 30);
        assert_eq!(loadout.damage_at(0.0, NONE), 240.0);

        // Its Reload loads two shells at a time.
        assert!(loadout.start_reload(NONE));
        assert!((loadout.reload_secs(NONE).unwrap() - (0.9 + 0.3 + 0.95)).abs() < 1e-5);
        run(&mut loadout, 3.0, NONE);
        assert_eq!(loadout.gun_ammo(), (6, 29));

        assert!(loadout.toggle_underbarrel());
        assert_eq!(loadout.gun_stats().name, "AUG-50M3");
        assert!(!loadout.toggle_underbarrel(), "still toggling back");
    }

    #[test]
    fn a_weapon_without_an_underbarrel_ignores_x() {
        let mut loadout = upgraded(WeaponId::Mp40, &[Attachment::Grip]);
        assert!(!loadout.toggle_underbarrel());
        assert_eq!(loadout.gun_stats().name, "The Afterburner");
    }

    #[test]
    fn a_weapon_switch_puts_the_main_gun_back_up() {
        let mut loadout = upgraded(WeaponId::Aug, &[]);
        loadout.toggle_underbarrel();
        run(&mut loadout, 1.0, NONE);
        assert!(loadout.underbarrel().is_some());
        loadout.cycle();
        run(&mut loadout, 5.0, NONE);
        loadout.cycle();
        run(&mut loadout, 5.0, NONE);
        assert!(loadout.underbarrel().is_none());
        assert_eq!(loadout.gun_stats().name, "AUG-50M3");
    }

    #[test]
    fn the_underbarrel_takes_perks_but_not_the_weapons_attachments() {
        let mut loadout = upgraded(WeaponId::M14, &[Attachment::RapidFire, Attachment::GrenadeLauncher]);
        loadout.toggle_underbarrel();
        assert_eq!(loadout.gun_stats().name, "Grenade Launcher");
        assert_eq!(loadout.fire_interval(NONE), GRENADE_LAUNCHER.fire_interval);
        assert_eq!(loadout.magazine_size(NONE), 1);
    }

    #[test]
    fn max_ammo_and_the_chalk_refill_the_underbarrel() {
        let mut loadout = upgraded(WeaponId::Mp40, &[Attachment::GrenadeLauncher]);
        assert!(loadout.ammo_full(1, NONE));
        loadout.toggle_underbarrel();
        run(&mut loadout, 1.0, NONE);
        assert!(loadout.try_fire(true, true, NONE));
        assert_eq!(loadout.gun_ammo(), (0, 5));
        assert!(!loadout.ammo_full(1, NONE), "the Underbarrel is short");
        loadout.refill(1, NONE);
        assert_eq!(loadout.gun_ammo(), (1, 5));
        assert!(loadout.ammo_full(1, NONE));
    }

    #[test]
    fn the_flamethrower_burns_its_tank_and_recharges_when_idle() {
        let mut loadout = upgraded(WeaponId::Mp40, &[Attachment::Flamethrower]);
        loadout.toggle_underbarrel();
        run(&mut loadout, 1.0, NONE);
        for _ in 0..10 {
            assert!(loadout.try_fire(true, false, NONE));
            run(&mut loadout, FLAMETHROWER.fire_interval, NONE);
        }
        assert_eq!(loadout.gun_ammo(), (90, 300));
        assert!(!loadout.start_reload(NONE), "it recharges instead");
        // A second off the trigger refills a second of tank.
        run(&mut loadout, 1.0, NONE);
        let (tank, reserve) = loadout.gun_ammo();
        assert!(tank > 90 && tank + reserve == 390, "{tank} + {reserve}");
    }

    #[test]
    fn attachments_stay_with_their_copy_through_a_down_and_go_with_it() {
        let mut loadout = upgraded(WeaponId::Mp40, &[Attachment::ExtendedMag, Attachment::Masterkey]);
        let mut stand = crate::last_stand::LastStand::begin(&mut loadout);
        stand.self_revive(&mut loadout);
        assert_eq!(loadout.held().attachments, [Attachment::ExtendedMag, Attachment::Masterkey]);
        assert!(loadout.held().underbarrel.is_some());
        // Replacing the copy loses them.
        loadout.give(WeaponId::Mp40);
        assert!(loadout.held().attachments.is_empty() && loadout.held().underbarrel.is_none());
        assert_eq!(loadout.held().magazine, 32);
    }
}
