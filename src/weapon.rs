use avian3d::prelude::*;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;

use crate::arsenal::{FireMode, WeaponClass, WeaponId, WeaponStats};
use crate::layers::GameLayer;
use crate::modifier::{Modifier, Modifiers, Stat, resolve, resolve_count};
use crate::player::{Locomotion, Player, PlayerCamera};
use crate::state::Pause;
use crate::tuning::*;
use crate::zombie::{Attack, DamageZombie, HitArea, HitZone};

/// One Weapon the Player carries: its ammo and its own modifier entries.
pub struct WeaponCopy {
    pub id: WeaponId,
    pub magazine: u32,
    pub reserve: u32,
    pub modifiers: Vec<Modifier>,
}

impl WeaponCopy {
    /// A freshly got Weapon: a full Magazine and its starting Reserve Ammo.
    pub fn new(id: WeaponId) -> Self {
        let stats = id.stats();
        Self {
            id,
            magazine: stats.magazine,
            reserve: stats.starting_reserve(),
            modifiers: Vec::new(),
        }
    }

    pub fn stats(&self) -> &'static WeaponStats {
        self.id.stats()
    }
}

/// A Reload in progress. The time it takes is resolved every frame, so only progress is kept.
pub struct Reload {
    pub elapsed: f32,
    /// The unmodified seconds, fixed when the Reload starts (tactical or empty, bullets to load).
    base_secs: f32,
}

/// Putting one Weapon away and bringing another up.
pub struct Swap {
    pub from: WeaponId,
    pub elapsed: f32,
    pub drop_secs: f32,
    pub raise_secs: f32,
}

impl Swap {
    fn between(from: WeaponId, to: WeaponId) -> Self {
        Self {
            from,
            elapsed: 0.0,
            drop_secs: from.stats().handling().drop_secs,
            raise_secs: to.stats().handling().raise_secs,
        }
    }

    pub fn secs(&self) -> f32 {
        self.drop_secs + self.raise_secs
    }
}

/// The Player's two Weapon slots. The held slot is always filled; the Knife is separate.
#[derive(Component)]
pub struct Loadout {
    slots: [Option<WeaponCopy>; 2],
    held: usize,
    pub reload: Option<Reload>,
    pub swap: Option<Swap>,
    since_shot: f32,
    /// Shots left in the current burst.
    burst_left: u32,
}

impl Loadout {
    pub fn new(id: WeaponId) -> Self {
        Self {
            slots: [Some(WeaponCopy::new(id)), None],
            held: 0,
            reload: None,
            swap: None,
            since_shot: f32::INFINITY,
            burst_left: 0,
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

    /// A stat of the held Weapon, resolved from its base and the Player's and the Weapon's entries.
    pub fn resolve(&self, stat: Stat, base: f32, player: &[Modifier]) -> f32 {
        resolve(stat, base, player.iter().chain(&self.held().modifiers))
    }

    pub fn magazine_size(&self, player: &[Modifier]) -> u32 {
        self.slot_magazine_size(self.held, player)
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

    /// Whether the Weapon in a filled slot has a full Magazine and full Reserve Ammo.
    pub fn ammo_full(&self, slot: usize, player: &[Modifier]) -> bool {
        let weapon = self.slot(slot).expect("a filled slot");
        weapon.magazine >= self.slot_magazine_size(slot, player) && weapon.reserve >= weapon.stats().reserve_max
    }

    /// Fill the Magazine and Reserve Ammo of the Weapon in a filled slot, as an ammo buy does.
    /// A Reload of that Weapon has nothing left to do, so it ends.
    pub fn refill(&mut self, slot: usize, player: &[Modifier]) {
        let size = self.slot_magazine_size(slot, player);
        let weapon = self.slots[slot].as_mut().expect("a filled slot");
        weapon.magazine = weapon.magazine.max(size);
        weapon.reserve = weapon.reserve.max(weapon.stats().reserve_max);
        if slot == self.held {
            self.reload = None;
        }
    }

    pub fn damage(&self, player: &[Modifier]) -> f32 {
        self.resolve(Stat::Damage, self.held().stats().damage_max, player)
    }

    pub fn fire_interval(&self, player: &[Modifier]) -> f32 {
        self.resolve(Stat::FireInterval, self.held().stats().fire_interval, player)
    }

    /// Seconds the current Reload takes in all, if reloading.
    pub fn reload_secs(&self, player: &[Modifier]) -> Option<f32> {
        let reload = self.reload.as_ref()?;
        Some(self.resolve(Stat::ReloadTime, reload.base_secs, player))
    }

    /// Bring up the Weapon in a slot. A switch cancels a Reload, losing its progress.
    pub fn switch_to(&mut self, slot: usize) -> bool {
        if self.swap.is_some() || slot == self.held || self.slot(slot).is_none() {
            return false;
        }
        let from = self.held().id;
        self.held = slot;
        self.begin_swap(from);
        true
    }

    /// Switch to the other slot.
    pub fn cycle(&mut self) -> bool {
        self.switch_to(1 - self.held)
    }

    /// Take a new Weapon and bring it up: into the empty slot if there is one, otherwise in
    /// place of the Weapon in hand.
    pub fn give(&mut self, id: WeaponId) {
        let from = self.held().id;
        if let Some(empty) = self.slots.iter().position(Option::is_none) {
            self.held = empty;
        }
        self.slots[self.held] = Some(WeaponCopy::new(id));
        self.begin_swap(from);
    }

    fn begin_swap(&mut self, from: WeaponId) {
        self.swap = Some(Swap::between(from, self.held().id));
        self.reload = None;
        self.burst_left = 0;
    }

    /// Advance the clocks that don't depend on any stat.
    pub fn tick(&mut self, dt: f32) {
        self.since_shot += dt;
        if let Some(swap) = &mut self.swap {
            swap.elapsed += dt;
            if swap.elapsed >= swap.secs() {
                self.swap = None;
            }
        }
    }

    /// Start a Reload, if the Magazine has room and there is Reserve Ammo to fill it from.
    pub fn start_reload(&mut self, player: &[Modifier]) -> bool {
        let size = self.magazine_size(player);
        let weapon = self.held();
        if self.swap.is_some() || self.reload.is_some() || weapon.magazine >= size || weapon.reserve == 0 {
            return false;
        }
        let loading = (size - weapon.magazine).min(weapon.reserve);
        let base_secs = weapon.stats().reload.secs(weapon.magazine, loading);
        self.reload = Some(Reload { elapsed: 0.0, base_secs });
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
            let weapon = self.held_mut();
            let taken = size.saturating_sub(weapon.magazine).min(weapon.reserve);
            weapon.reserve -= taken;
            weapon.magazine += taken;
        }
    }

    /// Whether the held Weapon fires now, given the trigger. Spends the bullet when it does.
    pub fn try_fire(&mut self, pressed: bool, just_pressed: bool, player: &[Modifier]) -> bool {
        let wants = match self.held().stats().fire {
            FireMode::Semi => just_pressed,
            FireMode::Auto => pressed,
            FireMode::Burst(shots) => {
                if just_pressed && self.burst_left == 0 {
                    self.burst_left = shots;
                }
                self.burst_left > 0
            }
        };
        let ready = self.swap.is_none() && self.reload.is_none() && self.since_shot >= self.fire_interval(player);
        if !wants || !ready {
            return false;
        }
        if self.held().magazine == 0 {
            self.burst_left = 0;
            return false;
        }
        self.held_mut().magazine -= 1;
        self.since_shot = 0.0;
        self.burst_left = self.burst_left.saturating_sub(1);
        true
    }
}

#[derive(Component, Default)]
pub struct Knife {
    cooldown: f32,
}

/// The held Weapon went off (for audio and effects).
#[derive(Message)]
pub struct ShotFired;

/// The on-screen Weapon model, a child of the camera.
#[derive(Component)]
struct ViewModel {
    kick: f32,
}

pub struct WeaponPlugin;

impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ShotFired>()
            .add_systems(Update, spawn_view_model)
            .add_systems(
                Update,
                (switch_weapons, reload, fire, knife, animate_view_model)
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn spawn_view_model(
    mut commands: Commands,
    cameras: Query<Entity, Added<PlayerCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for camera in &cameras {
        let metal = materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.15, 0.17),
            metallic: 0.8,
            perceptual_roughness: 0.4,
            ..default()
        });
        commands.entity(camera).with_child((
            ViewModel { kick: 0.0 },
            Mesh3d(meshes.add(Cuboid::new(0.06, 0.1, 0.3))),
            MeshMaterial3d(metal),
            Transform::from_xyz(0.22, -0.2, -0.45),
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

/// Scroll, Q, or 1/2 switch Weapons.
fn switch_weapons(
    mut loadout: Single<&mut Loadout, With<Player>>,
    input: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
) {
    loadout.tick(time.delta_secs());
    if input.just_pressed(KeyCode::Digit1) {
        loadout.switch_to(0);
    } else if input.just_pressed(KeyCode::Digit2) {
        loadout.switch_to(1);
    } else if input.just_pressed(KeyCode::KeyQ) || scroll.delta.y != 0.0 {
        loadout.cycle();
    }
}

fn reload(
    player: Single<(&mut Loadout, &Modifiers), With<Player>>,
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let (mut loadout, modifiers) = player.into_inner();
    if input.just_pressed(KeyCode::KeyR) {
        loadout.start_reload(&modifiers.0);
    }
    loadout.advance_reload(time.delta_secs(), &modifiers.0);
}

fn fire(
    player: Single<(Entity, &mut Loadout, &Modifiers, &Locomotion), With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    mut view_model: Single<&mut ViewModel>,
    mouse: Res<ButtonInput<MouseButton>>,
    spatial: SpatialQuery,
    hit_areas: Query<(&HitArea, &ChildOf)>,
    mut damage: MessageWriter<DamageZombie>,
    mut shots: MessageWriter<ShotFired>,
) {
    let (entity, mut loadout, modifiers, locomotion) = player.into_inner();
    let entries = &modifiers.0;
    if locomotion.sprinting {
        return;
    }
    let (pressed, just_pressed) = (mouse.pressed(MouseButton::Left), mouse.just_pressed(MouseButton::Left));
    if !loadout.try_fire(pressed, just_pressed, entries) {
        // Pulling the trigger on an empty Magazine starts a Reload.
        if just_pressed && loadout.held().magazine == 0 {
            loadout.start_reload(entries);
        }
        return;
    }
    view_model.kick = 1.0;
    shots.write(ShotFired);

    if let Some(hit) = spatial.cast_ray(
        camera.translation(),
        camera.forward(),
        HITSCAN_RANGE,
        true,
        &attack_filter(entity),
    ) && let Some((zombie, zone)) = zombie_for(hit.entity, &hit_areas)
    {
        // Pellets, falloff and the other zones' multipliers arrive with the combat model.
        let multiplier = if zone == HitZone::Head { loadout.held().stats().headshot } else { 1.0 };
        damage.write(DamageZombie {
            zombie,
            damage: loadout.damage(entries) * multiplier,
            zone,
            attack: Attack::Bullet,
        });
    }

    // Like the real thing: firing the last bullet starts a Reload.
    if loadout.held().magazine == 0 {
        loadout.start_reload(entries);
    }
}

fn knife(
    player: Single<(Entity, &mut Knife), With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    input: Res<ButtonInput<KeyCode>>,
    spatial: SpatialQuery,
    hit_areas: Query<(&HitArea, &ChildOf)>,
    mut damage: MessageWriter<DamageZombie>,
    time: Res<Time>,
) {
    let (entity, mut knife) = player.into_inner();
    knife.cooldown = (knife.cooldown - time.delta_secs()).max(0.0);
    if !input.just_pressed(KeyCode::KeyV) || knife.cooldown > 0.0 {
        return;
    }
    knife.cooldown = KNIFE_COOLDOWN_SECS;

    if let Some(hit) = spatial.cast_shape(
        &Collider::sphere(KNIFE_RADIUS),
        camera.translation(),
        Quat::IDENTITY,
        camera.forward(),
        &ShapeCastConfig::from_max_distance(KNIFE_RANGE),
        &attack_filter(entity),
    ) && let Some((zombie, zone)) = zombie_for(hit.entity, &hit_areas)
    {
        damage.write(DamageZombie {
            zombie,
            damage: KNIFE_DAMAGE,
            zone,
            attack: Attack::Knife,
        });
    }
}

/// How long the greybox model is for each class, so a switch is easy to see.
fn model_length(class: WeaponClass) -> f32 {
    match class {
        WeaponClass::Pistol | WeaponClass::DualWield | WeaponClass::BallisticKnife => 1.0,
        WeaponClass::RayGun => 1.3,
        WeaponClass::Smg | WeaponClass::Crossbow => 1.7,
        WeaponClass::Thundergun => 2.0,
        WeaponClass::Rifle | WeaponClass::Shotgun => 2.3,
        WeaponClass::Lmg | WeaponClass::Sniper | WeaponClass::Launcher => 2.7,
    }
}

fn animate_view_model(
    mut view_model: Single<(&mut ViewModel, &mut Transform)>,
    player: Single<(&Loadout, &Modifiers), With<Player>>,
    time: Res<Time>,
) {
    let (ref mut model, ref mut transform) = *view_model;
    let (loadout, modifiers) = *player;
    model.kick = (model.kick - time.delta_secs() * 8.0).max(0.0);

    // Dip the gun out of view while reloading; drop the old gun and raise the new one on a switch.
    let mut class = loadout.held().stats().class;
    let dip = if let Some(swap) = &loadout.swap {
        if swap.elapsed < swap.drop_secs {
            class = swap.from.stats().class;
            swap.elapsed / swap.drop_secs * 0.4
        } else {
            (1.0 - (swap.elapsed - swap.drop_secs) / swap.raise_secs) * 0.4
        }
    } else if let (Some(reload), Some(secs)) = (&loadout.reload, loadout.reload_secs(&modifiers.0)) {
        let t = (reload.elapsed / secs).min(1.0);
        (t * std::f32::consts::PI).sin() * 0.25
    } else {
        0.0
    };
    let length = model_length(class);
    transform.translation = Vec3::new(0.22, -0.2 - dip, -0.45 - 0.15 * (length - 1.0) + model.kick * 0.05);
    transform.rotation = Quat::from_rotation_x(model.kick * 0.25 - dip * 2.0);
    transform.scale = Vec3::new(1.0, 1.0, length);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::{Effect, Source};

    const NONE: &[Modifier] = &[];

    /// Ticks the Loadout and any Reload for this long.
    fn run(loadout: &mut Loadout, secs: f32, player: &[Modifier]) {
        loadout.tick(secs);
        loadout.advance_reload(secs, player);
    }

    #[test]
    fn the_player_starts_with_the_m1911_at_bo1_ammo() {
        let loadout = Loadout::new(STARTING_WEAPON);
        let held = loadout.held();
        assert_eq!(held.id, WeaponId::M1911);
        assert_eq!((held.magazine, held.reserve), (8, 32));
        assert_eq!(loadout.damage(NONE), 20.0);
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
    fn an_empty_magazine_doesnt_fire() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.held_mut().magazine = 0;
        assert!(!loadout.try_fire(true, true, NONE));
    }
}
