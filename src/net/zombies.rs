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
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::maps::barrier::Entering;
use crate::run::gauntlet::{Elite, EliteTrait};
use crate::enemies::hellhound::{HellhoundAssetsParam, Telegraph, end_strike, spawn_hellhound_puppet};
use crate::enemies::nova::{Burst, NovaAssetsParam, show_gas, spawn_nova_puppet};
use crate::enemies::sapper::{self, Sapper, SapperAssetsParam};
use crate::enemies::special::{self, Shrieker, SpecialAssetsParam, SpecialKind};
use crate::tuning::{PUPPET_SNAP_METRES, SNAPSHOT_MAX_CREATURES, SNAPSHOT_MAX_SPECIALS};
use crate::enemies::zombie::rig::{Activity, Shown, Swing, describe};
use crate::enemies::zombie::{
    Dying, KnockedDown, Rising, Stunned, Zombie, ZombieAssetsParam, make_puppet_crawler, spawn_puppet,
};

/// Every Zombie id is below this, so on the wire it is a varint of at most 2 bytes and a
/// `ZombieSight` at most 10: room in a snapshot for `SNAPSHOT_MAX_CREATURES`.
pub const ZOMBIE_ID_SPAN: u16 = 1 << 14;

/// Which Zombie or Hellhound this is on the wire. The Host numbers them; a client's puppets carry
/// the Host's number.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZombieId(pub u16);

/// A lasting fact about a Zombie that is told once, over the reliable stream, rather than in every
/// snapshot (see `WorldEvent::Tags`).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    /// An Elite, with its trait's `EliteTrait::code`.
    Elite(u8),
    /// Frosted for this many more tenths of a second.
    Frosted { tenths: u8 },
}

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

    /// Gives `entity` the next id not in use. Ids count up below `ZOMBIE_ID_SPAN` and come round
    /// again, so one is not back in use until 16 383 others have been.
    fn allocate(&mut self, entity: Entity) -> u16 {
        let id = loop {
            let id = self.next;
            self.next = (self.next + 1) % ZOMBIE_ID_SPAN;
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

/// Dresses each puppet the Host has told is an Elite as one: its trait, so its tint and rings show,
/// and the cue of its spawn, which is inferred from the tag rather than sent.
pub fn dress_elites(
    mut commands: Commands,
    crowd: Res<Crowd>,
    puppets: Query<(Entity, &ZombieId, &Transform), (With<Puppet>, Without<Elite>)>,
    mut cues: MessageWriter<PlayCue>,
) {
    if crowd.tags.is_empty() {
        return;
    }
    for (entity, id, transform) in &puppets {
        let Some(Tag::Elite(code)) = crowd.tag_of(id.0) else { continue };
        let Some(elite) = EliteTrait::from_code(code) else { continue };
        commands.entity(entity).insert(Elite(elite));
        cues.write(PlayCue::new(Cue::EliteSpawn, Some(transform.translation)));
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
    (horde.zombies, horde.hellhounds) = fit_horde(walkers, hounds);
    specials.sort_by_key(|(dead, sight)| (*dead, sight.id));
    specials.truncate(SNAPSHOT_MAX_SPECIALS);
    horde.specials = specials.into_iter().map(|(_, sight)| sight).collect();
}

/// The Zombies and Hellhounds a snapshot tells of, each `(dying, sight)`: all of them while they
/// fit in `SNAPSHOT_MAX_CREATURES` between them, and past that the dying and then the highest ids
/// left out, whichever list they are in.
pub fn fit_horde(walkers: Vec<(bool, ZombieSight)>, hounds: Vec<(bool, ZombieSight)>) -> (Vec<ZombieSight>, Vec<ZombieSight>) {
    let mut all: Vec<(bool, bool, ZombieSight)> = walkers
        .into_iter()
        .map(|(dead, sight)| (dead, false, sight))
        .chain(hounds.into_iter().map(|(dead, sight)| (dead, true, sight)))
        .collect();
    all.sort_by_key(|(dead, _, sight)| (*dead, sight.id));
    all.truncate(SNAPSHOT_MAX_CREATURES);
    let (hounds, walkers): (Vec<_>, Vec<_>) = all.into_iter().partition(|(_, hound, _)| *hound);
    let sights = |list: Vec<(bool, bool, ZombieSight)>| list.into_iter().map(|(_, _, sight)| sight).collect();
    (sights(walkers), sights(hounds))
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
    /// What the Host has told of each Zombie, by id, with the seconds it has gone unseen: the
    /// reliable stream can beat the datagram that first shows the Zombie, so a tag waits here for
    /// its puppet, and goes when the id does.
    tags: HashMap<u16, (Tag, f32)>,
}

/// How long a tag may wait for a puppet that never comes (a corpse the Host dropped).
const TAG_PATIENCE_SECS: f32 = 10.0;

impl Crowd {
    /// Takes in a tag the Host told of the Zombie `id`.
    pub fn tag(&mut self, id: u16, tag: Tag) {
        self.tags.insert(id, (tag, 0.0));
    }

    /// Lets a tag whose Zombie has not shown wait `dt` longer, and drops those that have waited
    /// out `TAG_PATIENCE_SECS`, so that an id the Host reuses is never dressed by an old tag.
    fn age_tags(&mut self, dt: f32) {
        let Crowd { glimpses, tags } = self;
        tags.retain(|id, (_, unseen)| {
            *unseen = if glimpses.contains_key(id) { 0.0 } else { *unseen + dt };
            *unseen < TAG_PATIENCE_SECS
        });
    }

    /// Whether any tag is held.
    pub fn has_tags(&self) -> bool {
        !self.tags.is_empty()
    }

    /// What is known of the Zombie `id`.
    pub fn tag_of(&self, id: u16) -> Option<Tag> {
        self.tags.get(&id).map(|&(tag, _)| tag)
    }

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
    time: Res<Time>,
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
    let dt = time.delta_secs();
    crowd.age_tags(dt);
    for id in over {
        crowd.glimpses.remove(&id);
        crowd.tags.remove(&id);
        if let Some(entity) = ids.entity(id) {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::run::gauntlet::{Elite, EliteTrait};

    #[test]
    fn a_tag_told_before_its_puppet_waits_and_dresses_it_when_it_comes() {
        let mut app = App::new();
        app.add_message::<PlayCue>().init_resource::<Crowd>().add_systems(Update, dress_elites);
        app.world_mut().resource_mut::<Crowd>().tag(7, Tag::Elite(EliteTrait::Rallying.code()));
        app.update();
        let puppet = app.world_mut().spawn((Puppet, ZombieId(7), Transform::default())).id();
        let other = app.world_mut().spawn((Puppet, ZombieId(8), Transform::default())).id();
        app.update();
        assert_eq!(app.world().get::<Elite>(puppet).map(|elite| elite.0), Some(EliteTrait::Rallying));
        assert!(app.world().get::<Elite>(other).is_none());
        let cues = app.world().resource::<Messages<PlayCue>>().len();
        assert_eq!(cues, 1, "the spawn cue is inferred from the tag, once");
        app.update();
        assert_eq!(app.world().resource::<Messages<PlayCue>>().len(), 1);
    }

    #[test]
    fn a_tag_whose_zombie_never_shows_is_dropped_so_its_id_cannot_dress_another() {
        let mut crowd = Crowd::default();
        crowd.tag(3, Tag::Elite(0));
        crowd.age_tags(TAG_PATIENCE_SECS - 1.0);
        assert!(crowd.has_tags());
        crowd.age_tags(1.0);
        assert!(!crowd.has_tags());
    }
    use super::*;

    fn sight(id: u16) -> ZombieSight {
        ZombieSight { id, position: [0; 6], yaw: 0, anim: 0 }
    }

    #[test]
    fn ids_stay_below_their_span_and_skip_those_in_use() {
        let mut ids = ZombieIds { next: ZOMBIE_ID_SPAN - 2, ..default() };
        ids.insert(0, Entity::from_raw_u32(900).unwrap());
        let [a, b, c] = [1, 2, 3].map(|n| ids.allocate(Entity::from_raw_u32(n).unwrap()));
        assert_eq!((a, b, c), (ZOMBIE_ID_SPAN - 2, ZOMBIE_ID_SPAN - 1, 1), "round again, past the 0 in use");
    }

    /// The Gauntlet's worst case of 78 alive, as a Pack Alpha Round can have it, is told in full.
    #[test]
    fn a_full_gauntlet_horde_and_pack_are_told_in_full() {
        let walkers: Vec<_> = (0..69).map(|id| (false, sight(id))).collect();
        let hounds: Vec<_> = (100..109).map(|id| (false, sight(id))).collect();
        let (zombies, hellhounds) = fit_horde(walkers, hounds);
        assert_eq!((zombies.len(), hellhounds.len()), (69, 9));
    }

    #[test]
    fn past_the_room_the_dying_and_then_the_highest_ids_are_left_out() {
        let walkers: Vec<_> = (0..SNAPSHOT_MAX_CREATURES as u16).map(|id| (false, sight(id * 2))).collect();
        let hounds = vec![(true, sight(1)), (false, sight(3)), (false, sight(9999))];
        let (zombies, hellhounds) = fit_horde(walkers, hounds);
        assert_eq!(zombies.len() + hellhounds.len(), SNAPSHOT_MAX_CREATURES);
        assert_eq!(hellhounds.iter().map(|s| s.id).collect::<Vec<_>>(), vec![3], "the dying hound and the highest id go");
        assert!(!zombies.iter().any(|s| s.id == (SNAPSHOT_MAX_CREATURES as u16 - 1) * 2), "the highest Zombie id goes");
    }
}

