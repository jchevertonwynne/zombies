//! Zombies and Hellhounds over the network. The Host gives each one an id that lasts as long as it
//! does, and every snapshot tells where each is, which way it faces and what it is doing. A
//! client has a puppet for each: a body with no AI and nothing to move it, put where the Host had
//! it a couple of snapshots ago and posed by its rig from what it was told it is doing. A Special
//! Zombie is told of by name in every snapshot, so its puppet has its own body: a Napalm's glow,
//! a Shrieker's pallor, the Sapper's armour and helmet, a Nova Crawler's low crawl and its gas, a
//! Hellhound's lightning strike.
//!
//! `ZombieIds` goes from an id to its Zombie and back, on either end, for anything that must name
//! one (a hit report names the Zombie it hit).

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use super::play::{Clock, Prediction, Puppet, SNAPSHOT_EVERY};
use super::state::{
    Snapshot, SpecialBody, SpecialSight, Tick, ZombieSight, pack_anim, pack_position, pack_special, pack_yaw, unpack_anim,
    unpack_special, unpack_yaw,
};
use crate::maps::barrier::Entering;
use crate::enemies::hellhound::{HellhoundAssetsParam, Telegraph, end_strike, spawn_hellhound_puppet};
use crate::enemies::nova::{Burst, NovaAssetsParam, show_gas, spawn_nova_puppet};
use crate::enemies::sapper::{self, Sapper, SapperAssetsParam};
use crate::enemies::special::{self, Shrieker, SpecialAssetsParam, SpecialKind};
use crate::tuning::{PUPPET_SNAP_METRES, SNAPSHOT_MAX_HELLHOUNDS, SNAPSHOT_MAX_SPECIALS, SNAPSHOT_MAX_ZOMBIES};
use crate::enemies::zombie::rig::{Activity, Shown, Swing, describe};
use crate::enemies::zombie::{
    Dying, KnockedDown, Rising, Stunned, Zombie, ZombieAssetsParam, make_puppet_crawler, spawn_puppet,
};

/// Which Zombie or Hellhound this is on the wire. The Host numbers them; a client's puppets carry
/// the Host's number.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZombieId(pub u16);

/// Every Zombie and Hellhound that has an id, by id and by entity.
#[derive(Resource, Default)]
pub struct ZombieIds {
    by_id: HashMap<u16, Entity>,
    by_entity: HashMap<Entity, u16>,
    next: u16,
}

impl ZombieIds {
    /// The Zombie or Hellhound with `id`, if there is one.
    pub fn entity(&self, id: u16) -> Option<Entity> {
        self.by_id.get(&id).copied()
    }

    /// The id of `entity`, if it has one.
    pub fn id(&self, entity: Entity) -> Option<u16> {
        self.by_entity.get(&entity).copied()
    }

    fn insert(&mut self, id: u16, entity: Entity) {
        self.by_id.insert(id, entity);
        self.by_entity.insert(entity, id);
    }

    /// Gives `entity` the next id not in use. Ids count up and come round again, so one is not
    /// back in use until 65 535 others have been.
    fn allocate(&mut self, entity: Entity) -> u16 {
        let id = loop {
            let id = self.next;
            self.next = self.next.wrapping_add(1);
            if !self.by_id.contains_key(&id) {
                break id;
            }
        };
        self.insert(id, entity);
        id
    }

    fn forget(&mut self, entity: Entity) {
        if let Some(id) = self.by_entity.remove(&entity)
            && self.by_id.get(&id) == Some(&entity)
        {
            self.by_id.remove(&id);
        }
    }
}

/// Lets go of the ids of Zombies that are gone.
pub fn forget_zombies(mut ids: ResMut<ZombieIds>, mut gone: RemovedComponents<ZombieId>) {
    for entity in gone.read() {
        ids.forget(entity);
    }
}

/// Gives every Zombie and Hellhound on the Host an id.
pub fn name_zombies(
    mut commands: Commands,
    mut ids: ResMut<ZombieIds>,
    new: Query<Entity, (With<Zombie>, Without<ZombieId>)>,
) {
    for entity in &new {
        let id = ids.allocate(entity);
        commands.entity(entity).insert(ZombieId(id));
    }
}

/// How the Host's Zombies and Hellhounds stand, as each snapshot tells it.
#[derive(Resource, Default)]
pub struct Horde {
    pub zombies: Vec<ZombieSight>,
    pub hellhounds: Vec<ZombieSight>,
    pub specials: Vec<SpecialSight>,
}

/// Writes down how every Zombie and Hellhound stands, on a tick a snapshot is sent. Those still
/// alive come first, so that if there are more than a snapshot has room for it is the dead that
/// are left out.
pub fn describe_horde(
    clock: Res<Clock>,
    map: Res<crate::maps::map::MapId>,
    mut horde: ResMut<Horde>,
    zombies: Query<(
        &ZombieId,
        &Transform,
        &Zombie,
        Option<&Entering>,
        Has<Rising>,
        Has<Stunned>,
        Has<KnockedDown>,
        Option<&Dying>,
        Option<&Shrieker>,
        Has<Telegraph>,
        Option<&Sapper>,
        Has<Burst>,
    )>,
) {
    if !clock.tick.is_multiple_of(SNAPSHOT_EVERY) {
        return;
    }
    let origin = super::state::wire_origin(*map);
    let (mut walkers, mut hounds, mut specials) = (Vec::new(), Vec::new(), Vec::new());
    for (id, transform, zombie, entering, rising, stunned, down, dying, shrieker, striking, sapper, burst) in &zombies {
        let screaming = shrieker.is_some_and(Shrieker::screaming);
        // A hound in its lightning strike is shown as rising from the ground.
        let activity = describe(0, Some(zombie), entering, rising || striking, stunned, down, dying, screaming);
        let sight = ZombieSight {
            id: id.0,
            position: pack_position(origin, transform.translation),
            yaw: pack_yaw(transform.rotation.to_euler(EulerRot::YXZ).0),
            anim: pack_anim(activity, zombie.is_crawler()),
        };
        let list = if zombie.is_hellhound() { &mut hounds } else { &mut walkers };
        list.push((dying.is_some(), sight));
        let body = match zombie.special_kind() {
            Some(SpecialKind::Napalm) => Some((SpecialBody::Napalm, false)),
            Some(SpecialKind::Shrieker) => Some((SpecialBody::Shrieker, false)),
            Some(SpecialKind::Sapper) => Some((SpecialBody::Sapper, sapper.is_some_and(Sapper::helmet_lost))),
            None if zombie.is_nova_crawler() => Some((SpecialBody::NovaCrawler, burst)),
            None => None,
        };
        if let Some((body, marked)) = body {
            specials.push((dying.is_some(), SpecialSight { id: id.0, body: pack_special(body, marked) }));
        }
    }
    let tell = |mut list: Vec<(bool, ZombieSight)>, most: usize| -> Vec<ZombieSight> {
        list.sort_by_key(|(dead, sight)| (*dead, sight.id));
        list.truncate(most);
        list.into_iter().map(|(_, sight)| sight).collect()
    };
    horde.zombies = tell(walkers, SNAPSHOT_MAX_ZOMBIES);
    horde.hellhounds = tell(hounds, SNAPSHOT_MAX_HELLHOUNDS);
    specials.sort_by_key(|(dead, sight)| (*dead, sight.id));
    specials.truncate(SNAPSHOT_MAX_SPECIALS);
    horde.specials = specials.into_iter().map(|(_, sight)| sight).collect();
}

/// Where a Zombie or Hellhound was said to be, and what it was doing, at one of the Host's ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Glimpse {
    tick: Tick,
    position: Vec3,
    yaw: f32,
    anim: u8,
}

/// What has been said of one Zombie or Hellhound, oldest first.
#[derive(Debug)]
struct Glimpses {
    hellhound: bool,
    /// What it was last told to be, if a Special Zombie, see `pack_special`; 0 if not.
    special: u8,
    seen: VecDeque<Glimpse>,
}

/// What a client has been told of the Zombies and Hellhounds.
#[derive(Resource, Default)]
pub struct Crowd {
    glimpses: HashMap<u16, Glimpses>,
}

impl Crowd {
    /// Takes in what a snapshot says of them.
    pub fn hear(&mut self, snapshot: &Snapshot, origin: Vec3) {
        for (hellhound, list) in [(false, &snapshot.zombies), (true, &snapshot.hellhounds)] {
            for sight in list {
                let glimpses = self.glimpses.entry(sight.id).or_insert_with(|| Glimpses { hellhound, special: 0, seen: VecDeque::new() });
                glimpses.seen.push_back(Glimpse {
                    tick: snapshot.tick,
                    position: sight.position(origin),
                    yaw: unpack_yaw(sight.yaw),
                    anim: sight.anim,
                });
            }
        }
        for special in &snapshot.specials {
            if let Some(glimpses) = self.glimpses.get_mut(&special.id) {
                glimpses.special = special.body;
            }
        }
    }
}

/// Where one is shown at the Host's tick `shown`: between the two glimpses either side of it, or
/// at the only one, or at the newest if `shown` is past it. One that has moved farther than a
/// Zombie walks between them was moved, and is where the earlier said until the later is shown.
fn glimpse(seen: &VecDeque<Glimpse>, shown: f32) -> Option<Glimpse> {
    let (&a, &b) = match (seen.front(), seen.get(1)) {
        (Some(a), Some(b)) => (a, b),
        (Some(&only), None) => return Some(only),
        _ => return None,
    };
    let t = ((shown - a.tick as f32) / (b.tick - a.tick) as f32).clamp(0.0, 1.0);
    let turn = (b.yaw - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let (position, anim) = if a.position.distance(b.position) > PUPPET_SNAP_METRES {
        if t < 1.0 { (a.position, a.anim) } else { (b.position, b.anim) }
    } else {
        (a.position.lerp(b.position, t), if t < 1.0 { a.anim } else { b.anim })
    };
    Some(Glimpse { tick: a.tick, position, yaw: a.yaw + turn * t, anim })
}

/// Lets go of what was said of one before the pair either side of `shown`.
fn forget_before(seen: &mut VecDeque<Glimpse>, shown: f32) {
    while seen.len() > 2 && seen[1].tick as f32 <= shown {
        seen.pop_front();
    }
}

/// Everything a client needs to build a puppet's body.
#[derive(bevy::ecs::system::SystemParam)]
pub struct PuppetAssets<'w> {
    zombie: ZombieAssetsParam<'w>,
    hound: HellhoundAssetsParam<'w>,
    nova: NovaAssetsParam<'w>,
    special: SpecialAssetsParam<'w>,
    sapper: SapperAssetsParam<'w>,
}

/// Puts the puppets where the Host had the Zombies and Hellhounds a moment ago, spawns one for each
/// that has come into being and lets go of each that has gone. One the Host shows dying falls over
/// by itself and goes after its corpse has lain a while, as it does on the Host.
pub fn show_zombies(
    mut commands: Commands,
    assets: PuppetAssets,
    prediction: Res<Prediction>,
    mut crowd: ResMut<Crowd>,
    mut ids: ResMut<ZombieIds>,
    mut puppets: Query<
        (&mut Transform, &Zombie, &mut Shown, Has<Dying>, Option<&mut Sapper>, Has<Burst>, Option<&Telegraph>),
        With<Puppet>,
    >,
) {
    let Some(latest) = prediction.latest else { return };
    let shown = prediction.shown;
    let mut over = Vec::new();
    for (&id, glimpses) in &mut crowd.glimpses {
        forget_before(&mut glimpses.seen, shown);
        let Some(newest) = glimpses.seen.back().map(|newest| newest.tick) else { continue };
        // Not in the newest snapshot, and the moment shown has come to its last: it is gone.
        if newest < latest && shown >= newest as f32 {
            over.push(id);
            continue;
        }
        let Some(at) = glimpse(&glimpses.seen, shown) else { continue };
        let (activity, crawler) = unpack_anim(at.anim);
        let special = unpack_special(glimpses.special);
        let dying = matches!(activity, Activity::Dying { .. } | Activity::Tipped { .. });
        let Some(entity) = ids.entity(id) else {
            // One first seen dying is a corpse the Host is about to lose: not worth a body.
            if dying {
                continue;
            }
            let entity = if glimpses.hellhound {
                spawn_hellhound_puppet(&mut commands, &assets.hound, at.position, activity == Activity::Rising)
            } else if let Some((SpecialBody::NovaCrawler, _)) = special {
                spawn_nova_puppet(&mut commands, &assets.nova, at.position)
            } else {
                let entity = spawn_puppet(&mut commands, &assets.zombie, at.position, at.yaw, crawler);
                match special {
                    Some((SpecialBody::Napalm, _)) => special::dress_puppet(&mut commands, &assets.special, entity, SpecialKind::Napalm),
                    Some((SpecialBody::Shrieker, _)) => special::dress_puppet(&mut commands, &assets.special, entity, SpecialKind::Shrieker),
                    Some((SpecialBody::Sapper, lost)) => sapper::dress_puppet(&mut commands, &assets.sapper, entity, at.position, !lost),
                    _ => {}
                }
                entity
            };
            commands.entity(entity).insert((Puppet, ZombieId(id), Shown(activity)));
            ids.insert(id, entity);
            continue;
        };
        let Ok((mut transform, zombie, mut was, falling, sapper, burst, striking)) = puppets.get_mut(entity) else { continue };
        if transform.translation != at.position {
            transform.translation = at.position;
        }
        if dying && !falling {
            commands.entity(entity).insert(Dying::default());
        }
        // A body falling over is turned by that, not by where it was facing.
        if !dying && !falling {
            let facing = Quat::from_rotation_y(at.yaw);
            if transform.rotation != facing {
                transform.rotation = facing;
            }
        }
        // The Sapper slams with both arms.
        let activity = match activity {
            Activity::Swinging { progress, .. } if zombie.is_sapper() => Activity::Swinging { progress, arm: Swing::Both },
            other => other,
        };
        was.set_if_neq(Shown(activity));
        match special {
            Some((SpecialBody::Sapper, true)) => {
                if let Some(mut sapper) = sapper
                    && !sapper.helmet_lost()
                {
                    sapper.lose_helmet(&mut commands);
                }
            }
            Some((SpecialBody::NovaCrawler, true)) if !burst => {
                commands.entity(entity).insert(Burst);
                show_gas(&mut commands, &assets.nova, at.position);
            }
            _ => {}
        }
        // A hound is loose once the Host has it out of its strike.
        if let Some(telegraph) = striking
            && activity != Activity::Rising
        {
            end_strike(&mut commands, entity, telegraph);
        }
        if crawler && !zombie.is_crawler() && !dying && !falling && !glimpses.hellhound {
            make_puppet_crawler(&mut commands, &assets.zombie, entity);
        }
    }
    for id in over {
        crowd.glimpses.remove(&id);
        if let Some(entity) = ids.entity(id) {
            commands.entity(entity).despawn();
        }
    }
}
