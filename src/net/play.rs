//! A Run played over a `Link`: Players' movement. Solo adds none of this.
//!
//! Every tick a client sends the input its Player is stepped by, the last few ticks over again in
//! case one was lost, and steps its own Player by it at once. The Host steps each remote Player by
//! their input through the jitter buffer (`Remote`), never rewinding, and 32 times a second tells
//! each Player how things stand: their own Player in full, the others packed. A client puts its
//! Player where the Host says, then steps it again by the input the Host hadn't heard yet, which
//! mostly lands where it already was; where it doesn't, the gap is eased out of how the Player is
//! drawn over a few frames rather than jumped (see `draw_correction`). The other Players are
//! puppets: not stepped at all, but shown where the Host had them a couple of snapshots ago, as
//! are the Zombies and Hellhounds (see `zombies`).

use std::collections::VecDeque;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_transform_interpolation::{TransformEasingSystems, TranslationEasingState};

use super::away::{self, Rejoin};
use super::hits::{self, Hits};
use super::link::{Event, Link};
use super::protocol::{HOST, PROTOCOL, Peer, Refusal, Score, Seat, ToHost, ToPlayer, decode, encode, token};
use super::remote::{Next, Remote};
use super::world::{self, Mirror, NetIds};
use super::state::{
    Inputs, Own, Sight, Snapshot, Step, Tick, pack_pitch, pack_position, pack_yaw, unpack_pitch, unpack_yaw,
};
use super::zombies::{
    Crowd, Horde, ZombieIds, describe_horde, forget_zombies, name_zombies, show_zombies,
};
use crate::authority::Authority;
use crate::last_stand::LastStand;
use crate::leave_run::PlayerLeft;
use crate::perk::Drinking;
use crate::respawn::{Disconnected, SeatDropped, SeatReturned};
use crate::player::{
    LocalPlayer, Locomotion, MoveContext, MoveInput, Player, Seat as PlayerSeat, fill_move_input, move_player,
    step_move,
};
use crate::modifier::Modifiers;
use crate::points::Points;
use crate::state::{Choosing, GameState, world_runs};
use crate::teammate::{Pose, Shots, Told};
use crate::tuning::*;
use crate::weapon::Loadout;

/// How many ticks a snapshot comes after the one before.
pub const SNAPSHOT_EVERY: Tick = FIXED_HZ / SNAPSHOT_HZ;

/// The `Link` of a Run in Co-op, whichever end this is.
#[derive(Resource)]
pub struct Wire(pub Box<dyn Link>);

/// This machine's count of ticks, from 1. Not shared: the Host maps a client's ticks through
/// `Remote`, and says which of them it has used.
#[derive(Resource, Default)]
pub struct Clock {
    pub tick: Tick,
}

/// What has come in over the `Wire` and not been read yet.
#[derive(Resource, Default)]
pub struct Inbox {
    inputs: Vec<(Peer, Inputs)>,
    snapshots: Vec<Snapshot>,
}

/// Which Player is whose to the Host: a Peer and the Seat they hold.
#[derive(Resource, Default)]
pub struct Roster(pub Vec<(Peer, Seat)>);

/// What a Host's Run knows of its Players away from it: what each comes back by, and which of them
/// are away now. A Seat is held for the Run whichever way its Player's link goes.
#[derive(Resource, Default)]
pub struct Held {
    pub tokens: Vec<(Seat, u64)>,
    pub away: Vec<Seat>,
}

impl Held {
    /// The Seat that `token` comes back to, if it is one the Host gave. Never for 0.
    fn seat_of(&self, token: Option<u64>) -> Option<Seat> {
        let token = token.filter(|&token| token != 0)?;
        self.tokens.iter().find(|&&(_, held)| held == token).map(|&(seat, _)| seat)
    }

    fn renew(&mut self, seat: Seat, token: u64) {
        if let Some(held) = self.tokens.iter_mut().find(|(held, _)| *held == seat) {
            held.1 = token;
        }
        self.away.retain(|&away| away != seat);
    }
}

/// A Player on a client who is someone else's: not stepped here, but shown where the Host says.
#[derive(Component)]
pub struct Puppet;

/// How each Player did, as the Host counted it: said to every machine as the Run ends, and read
/// by the Records.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct Scoreboard(pub Vec<Score>);

/// What the Host said over the reliable stream that the Run reacts to, as a client hears it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostSaid {
    /// The Host will not take this Player back into their Seat.
    Refused(Refusal),
    /// Every Player is down.
    RunOver,
    /// The Host has left: the Run and the Room are over.
    Closed,
    /// The Host is back in the Room.
    BackToRoom,
}

/// Puts a Run's systems in the app. Each end's run only while that end's resources are there
/// (`Roster` is the Host's, `Prediction` a client's), so the app can be either, run after run.
pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        hits::add(app);
        world::add(app);
        app.add_message::<PlayerLeft>()
            .add_message::<HostSaid>()
            .add_message::<SeatDropped>()
            .add_message::<SeatReturned>()
            .init_resource::<Scoreboard>()
            .add_systems(PreUpdate, (pump_host.run_if(resource_exists::<Roster>), pump_client.run_if(resource_exists::<Prediction>)))
            .add_systems(
                Update,
                release_remotes.run_if(resource_exists::<Roster>).run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (away::redial, away::hold_local)
                    .chain()
                    .run_if(resource_exists::<Rejoin>)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                FixedUpdate,
                (tick, adopt_remotes, hear_inputs, drive_remotes)
                    .chain()
                    .before(move_player)
                    .run_if(world_runs)
                    .run_if(resource_exists::<Roster>),
            )
            .add_systems(
                FixedUpdate,
                (forget_zombies, name_zombies, describe_horde)
                    .chain()
                    .after(move_player)
                    .before(send_snapshots)
                    .run_if(world_runs)
                    .run_if(resource_exists::<Roster>),
            )
            .add_systems(
                FixedUpdate,
                (pose_by_steps, send_snapshots)
                    .chain()
                    .after(move_player)
                    .run_if(world_runs)
                    .run_if(resource_exists::<Roster>),
            )
            .add_systems(
                FixedUpdate,
                (tick, adopt_puppets, reconcile, send_inputs)
                    .chain()
                    .after(fill_move_input)
                    .before(move_player)
                    .run_if(world_runs)
                    .run_if(resource_exists::<Prediction>),
            )
            .add_systems(
                FixedUpdate,
                show_puppets.after(move_player).run_if(world_runs).run_if(resource_exists::<Prediction>),
            )
            .add_systems(
                FixedUpdate,
                (forget_zombies, show_zombies)
                    .chain()
                    .after(show_puppets)
                    .run_if(world_runs)
                    .run_if(resource_exists::<Prediction>),
            )
            // Each frame, once the easing has drawn the `LocalPlayer` between its last two ticks,
            // and before it notes having done so (see `crate::player::LookSystems`).
            .add_systems(
                RunFixedMainLoop,
                draw_correction
                    .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop)
                    .after(TransformEasingSystems::Ease)
                    .before(TransformEasingSystems::UpdateEasingTick)
                    .run_if(resource_exists::<Prediction>),
            );
    }
}

/// Makes this machine the Host of a Run with `remotes` joined: they are stepped by what they send.
pub fn begin_host(world: &mut World, link: Box<dyn Link>, remotes: Vec<(Peer, Seat)>) {
    world.insert_resource(Wire(link));
    world.insert_resource(Roster(remotes));
    world.insert_resource(Held::default());
    world.insert_resource(Clock::default());
    world.insert_resource(Inbox::default());
    world.insert_resource(Hits::default());
    world.insert_resource(Mirror::default());
    world.insert_resource(NetIds::default());
    world.insert_resource(ZombieIds::default());
    world.insert_resource(Horde::default());
    world.insert_resource(Scoreboard::default());
}

/// Makes this machine a client of a Run: its `LocalPlayer` is stepped here, and every other Player
/// is a puppet.
pub fn begin_client(world: &mut World, link: Box<dyn Link>, rejoin: Rejoin) {
    world.insert_resource(Wire(link));
    world.insert_resource(rejoin);
    world.insert_resource(Clock::default());
    world.insert_resource(Inbox::default());
    world.insert_resource(Hits::default());
    world.insert_resource(Mirror::default());
    world.insert_resource(NetIds::default());
    world.insert_resource(Prediction::default());
    world.insert_resource(ZombieIds::default());
    world.insert_resource(Crowd::default());
    world.insert_resource(Scoreboard::default());
    if let Some(mut next) = world.get_resource_mut::<NextState<Authority>>() {
        next.set(Authority::Client);
    }
}

/// The Run is over for this machine: its link is given back, for the Room (or nothing) to keep.
pub fn end_run(world: &mut World) -> Option<Box<dyn Link>> {
    world.remove_resource::<Roster>();
    world.remove_resource::<Held>();
    world.remove_resource::<Rejoin>();
    world.remove_resource::<Prediction>();
    world.remove_resource::<Clock>();
    world.remove_resource::<Inbox>();
    world.remove_resource::<Hits>();
    world.remove_resource::<Mirror>();
    world.remove_resource::<NetIds>();
    world.remove_resource::<Wire>().map(|wire| wire.0)
}

/// Makes this machine the Host of a Run with `remotes` joined: they are stepped by what they send.
pub fn install_host(app: &mut App, link: impl Link, remotes: Vec<(Peer, Seat)>) {
    begin_host(app.world_mut(), Box::new(link), remotes);
    if !app.is_plugin_added::<PlayPlugin>() {
        app.add_plugins(PlayPlugin);
    }
}

/// Makes this machine a client of a Run: its `LocalPlayer` is stepped here, and every other Player
/// is a puppet.
pub fn install_client(app: &mut App, link: impl Link) {
    begin_client(app.world_mut(), Box::new(link), Rejoin::new(0, ToHost::LeftRun));
    if !app.is_plugin_added::<PlayPlugin>() {
        app.add_plugins(PlayPlugin);
    }
}

/// Reads what the Host's Players have sent.
fn pump_host(
    mut wire: ResMut<Wire>,
    mut inbox: ResMut<Inbox>,
    mut roster: ResMut<Roster>,
    mut hits: ResMut<Hits>,
    mut mirror: ResMut<Mirror>,
    mut held: ResMut<Held>,
    mut left: MessageWriter<PlayerLeft>,
    mut dropped: MessageWriter<SeatDropped>,
    mut returned: MessageWriter<SeatReturned>,
) {
    while let Some(event) = wire.0.poll() {
        match event {
            // A Player's way breaking, without their having said they are leaving for good,
            // holds their Seat.
            Event::Left(peer) | Event::Lost(peer) => {
                if let Some(at) = roster.0.iter().position(|(on, _)| *on == peer) {
                    let (_, seat) = roster.0.remove(at);
                    held.away.push(seat);
                    dropped.write(SeatDropped { seat: PlayerSeat(seat) });
                }
            }
            Event::Datagram(peer, bytes) => match decode::<Inputs>(&bytes) {
                Ok(inputs) => inbox.inputs.push((peer, inputs)),
                Err(error) => debug!("player {peer}: {error}"),
            },
            Event::Message(peer, bytes) => {
                let known = roster.0.iter().find(|(held, _)| *held == peer).map(|&(_, seat)| seat);
                match (decode::<ToHost>(&bytes), known) {
                    (Ok(ToHost::LeftRun), Some(seat)) => {
                        roster.0.retain(|(held, _)| *held != peer);
                        left.write(PlayerLeft { seat: PlayerSeat(seat) });
                    }
                    (Ok(message @ (ToHost::Hit(_) | ToHost::Launch(_))), Some(_)) => hits.hear_player(peer, &message),
                    (Ok(ToHost::Request(request)), Some(_)) => mirror.hear_player(peer, request),
                    // A Player coming back by the token the Host gave takes their Seat again. The
                    // Room has no Seat for anyone else who comes after the Run has begun.
                    (Ok(ToHost::Hello { version, back, .. }), None) => match held.seat_of(back) {
                        Some(seat) if version == PROTOCOL => {
                            // Their old link may not have been found broken yet.
                            if let Some(at) = roster.0.iter().position(|&(_, taken)| taken == seat) {
                                let (old, _) = roster.0.remove(at);
                                wire.0.close(old);
                                dropped.write(SeatDropped { seat: PlayerSeat(seat) });
                            }
                            let token = token();
                            held.renew(seat, token);
                            roster.0.push((peer, seat));
                            wire.0.send(peer, encode(&ToPlayer::Welcome { token, seat }));
                            returned.write(SeatReturned { seat: PlayerSeat(seat) });
                        }
                        _ => {
                            let why = if back.is_some() && version == PROTOCOL { Refusal::NoSeat } else { Refusal::Started };
                            wire.0.send(peer, encode(&ToPlayer::Refused(why)));
                            wire.0.close(peer);
                        }
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// Reads what the Host has sent.
fn pump_client(
    mut wire: ResMut<Wire>,
    mut inbox: ResMut<Inbox>,
    mut hits: ResMut<Hits>,
    mut mirror: ResMut<Mirror>,
    mut scoreboard: ResMut<Scoreboard>,
    mut rejoin: ResMut<Rejoin>,
    mut said: MessageWriter<HostSaid>,
) {
    while let Some(event) = wire.0.poll() {
        match event {
            // The way to the Host broke, or a dial of it failed: it may be dialled again.
            Event::Lost(HOST) | Event::Failed(_) => rejoin.lost(),
            // A Host that says it is going has gone for good.
            Event::Left(HOST) => {
                said.write(HostSaid::Closed);
            }
            // Dialled again: the Host is told who is back.
            Event::Joined(HOST) if rejoin.is_away() => wire.0.send(HOST, encode(&rejoin.greeting())),
            Event::Datagram(HOST, bytes) => match decode::<Snapshot>(&bytes) {
                Ok(snapshot) => inbox.snapshots.push(snapshot),
                Err(error) => debug!("host: {error}"),
            },
            Event::Message(HOST, bytes) => match decode::<ToPlayer>(&bytes) {
                Ok(ToPlayer::Welcome { token, .. }) => rejoin.welcomed(token),
                Ok(ToPlayer::Refused(why)) => {
                    said.write(HostSaid::Refused(why));
                }
                Ok(ToPlayer::Scoreboard(rows)) => scoreboard.0 = rows,
                Ok(ToPlayer::RunOver) => {
                    said.write(HostSaid::RunOver);
                }
                Ok(ToPlayer::Closed) => {
                    said.write(HostSaid::Closed);
                }
                // What comes after it is the Room's to read.
                Ok(ToPlayer::BackToRoom) => {
                    said.write(HostSaid::BackToRoom);
                    break;
                }
                Ok(message @ (ToPlayer::Launched(_) | ToPlayer::Burst(_))) => hits.hear_host(&message),
                Ok(ToPlayer::Event(event)) => mirror.hear_host(event),
                _ => {}
            },
            _ => {}
        }
    }
}

/// A Player whose link has dropped is no longer stepped by the Host's buffer of their input, nor
/// told anything: they take it up again when they are back.
fn release_remotes(
    mut commands: Commands,
    mut dropped: MessageReader<SeatDropped>,
    players: Query<(Entity, &PlayerSeat), (With<Player>, With<Remote>)>,
) {
    for &SeatDropped { seat } in dropped.read() {
        for (player, _) in players.iter().filter(|(_, held)| **held == seat) {
            commands.entity(player).remove::<Remote>();
        }
    }
}

fn tick(mut clock: ResMut<Clock>) {
    clock.tick += 1;
}

/// Hands each Player in the roster to the Peer whose game steps them.
fn adopt_remotes(
    mut commands: Commands,
    roster: Res<Roster>,
    players: Query<(Entity, &PlayerSeat), (With<Player>, Without<Remote>, Without<LocalPlayer>)>,
) {
    for (entity, seat) in &players {
        if let Some(&(peer, _)) = roster.0.iter().find(|(_, held)| *held == seat.0) {
            commands.entity(entity).insert((Remote::new(peer), Told));
        }
    }
}

/// Takes in what the remote Players say they pressed, and what their bodies are doing.
fn hear_inputs(mut inbox: ResMut<Inbox>, mut remotes: Query<(&mut Remote, &mut Pose, &mut Shots, Has<LastStand>)>) {
    for (peer, inputs) in inbox.inputs.drain(..) {
        let Some((mut remote, mut pose, mut shots, last_stand)) =
            remotes.iter_mut().find(|(remote, ..)| remote.peer == peer)
        else {
            continue;
        };
        remote.hear(inputs.last, &inputs.steps);
        // Whether they are in Last Stand is the Host's to say.
        pose.set_if_neq(Pose { last_stand, ..Pose::unpack(inputs.pose) });
        shots.set(inputs.shots);
    }
}

/// Sets each remote Player's input for the tick to what they pressed in the next of theirs. A tick
/// that hasn't come is stepped as the last was, with no Jump newly pressed; when none has come for
/// a while the Player stands still.
fn drive_remotes(mut players: Query<(&mut Remote, &mut MoveInput, &mut Transform, Has<Choosing>, Has<Disconnected>)>) {
    for (mut remote, mut input, mut transform, choosing, disconnected) in &mut players {
        match remote.next() {
            Next::Step(step) => *input = step.input(),
            Next::Nothing => {}
            Next::Missing | Next::Held => input.jump = false,
        }
        // An open Offer stops a Player, whatever their game says.
        if remote.stalled() || choosing || disconnected {
            *input = MoveInput { yaw: input.yaw, pitch: input.pitch, ..default() };
        }
        // `step_move` reads the yaw from the input; the body and the others' view of it read the
        // Transform.
        transform.rotation = Quat::from_rotation_y(input.yaw);
    }
}

/// Poses each remote Player's body as the Host has stepped it: their Stance, and whether they're
/// off the floor (Prone ∧ airborne is a Dive in flight). Their game's own word on these is only
/// an echo, and teammates, Zombies and Host-judged harm all go by where the Host has the body.
fn pose_by_steps(mut remotes: Query<(&Locomotion, &mut Pose), With<Remote>>) {
    for (locomotion, mut pose) in &mut remotes {
        let stepped = Pose { stance: locomotion.stance, airborne: !locomotion.grounded(), ..*pose };
        pose.set_if_neq(stepped);
    }
}

/// Tells each remote Player how things stand: themselves in full, and the other Players packed.
fn send_snapshots(
    clock: Res<Clock>,
    horde: Res<Horde>,
    mut wire: ResMut<Wire>,
    players: Query<
        (&PlayerSeat, &Transform, &Locomotion, &MoveInput, &Pose, &Shots, Option<&Points>, Option<&Remote>),
        With<Player>,
    >,
) {
    if !clock.tick.is_multiple_of(SNAPSHOT_EVERY) {
        return;
    }
    let balance = |points: Option<&Points>| points.map_or(0, |points| points.balance);
    let sights: Vec<Sight> = players
        .iter()
        .map(|(seat, transform, _, input, pose, shots, points, _)| Sight {
            seat: seat.0,
            position: pack_position(transform.translation),
            yaw: pack_yaw(input.yaw),
            pitch: pack_pitch(input.pitch),
            pose: pose.pack(),
            shots: shots.count(),
            points: balance(points).min(u64::from(u32::MAX)) as u32,
        })
        .collect();
    for (seat, transform, locomotion, _, _, _, points, remote) in &players {
        let Some(remote) = remote else { continue };
        let snapshot = Snapshot {
            tick: clock.tick,
            used: remote.used(),
            own: Own::of(transform.translation, locomotion, balance(points)),
            players: sights.iter().filter(|sight| sight.seat != seat.0).copied().collect(),
            zombies: horde.zombies.clone(),
            hellhounds: horde.hellhounds.clone(),
            specials: horde.specials.clone(),
        };
        let bytes = encode(&snapshot);
        debug_assert!(bytes.len() <= SNAPSHOT_MAX_BYTES, "a snapshot of {}", bytes.len());
        wire.0.datagram(remote.peer, bytes);
    }
}

/// What the client remembers between ticks.
#[derive(Resource, Default)]
pub struct Prediction {
    /// The ticks sent that the Host hasn't yet said it used, each with the `Locomotion` it was
    /// stepped from. A correction replays from the one after the Host's last used tick, so the
    /// stance and Dive timers — which no snapshot carries — run each tick once, not again on
    /// every replay.
    sent: VecDeque<(Tick, Step, Locomotion)>,
    /// The Host's tick of the newest snapshot heard.
    pub(super) latest: Option<Tick>,
    /// Where each Seat's Player has been said to be, oldest first.
    seen: [VecDeque<Seen>; ROOM_SEATS],
    /// The Host's tick the puppets are being shown at.
    pub(super) shown: f32,
    /// How far the last snapshot moved the `LocalPlayer`, in metres.
    pub corrected: f32,
    /// How far from where it is the `LocalPlayer` is drawn: what is left of the corrections, eased
    /// out over `CORRECTION_EASE_SECS` (see `draw_correction`).
    pub drawn: Vec3,
}

/// Another Player as the Host said they were at one of its ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seen {
    tick: Tick,
    position: Vec3,
    yaw: f32,
    pitch: f32,
    pose: Pose,
    shots: u8,
}

impl Seen {
    fn of(tick: Tick, sight: &Sight) -> Self {
        Seen {
            tick,
            position: sight.position(),
            yaw: unpack_yaw(sight.yaw),
            pitch: unpack_pitch(sight.pitch),
            pose: Pose::unpack(sight.pose),
            shots: sight.shots,
        }
    }
}

/// Where a Player is shown at the Host's tick `shown`: between the two things said of them either
/// side of it, or at the only one, or at the newest if `shown` is past it. A snapshot lost on the
/// way is a longer way between its neighbours, and nothing else.
fn show(seen: &VecDeque<Seen>, shown: f32) -> Option<Seen> {
    let (&a, &b) = match (seen.front(), seen.get(1)) {
        (Some(a), Some(b)) => (a, b),
        (Some(&only), None) => return Some(only),
        _ => return None,
    };
    let t = ((shown - a.tick as f32) / (b.tick - a.tick) as f32).clamp(0.0, 1.0);
    let turn = (b.yaw - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let (pose, shots) = if t < 1.0 { (a.pose, a.shots) } else { (b.pose, b.shots) };
    Some(Seen {
        tick: a.tick,
        position: a.position.lerp(b.position, t),
        yaw: a.yaw + turn * t,
        pitch: a.pitch + (b.pitch - a.pitch) * t,
        pose,
        shots,
    })
}

/// Lets go of what was said of a Player before the pair either side of `shown`.
fn forget_before(seen: &mut VecDeque<Seen>, shown: f32) {
    while seen.len() > 2 && seen[1].tick as f32 <= shown {
        seen.pop_front();
    }
}

/// Marks every Player but the `LocalPlayer` as a puppet.
fn adopt_puppets(
    mut commands: Commands,
    players: Query<Entity, (With<Player>, Without<LocalPlayer>, Without<Puppet>)>,
) {
    for entity in &players {
        commands.entity(entity).insert((Puppet, Told));
    }
}

/// Takes in what the Host has said: everyone's Points and where the others are, and where the
/// `LocalPlayer` was when the Host had stepped it as far as it had heard. Then the `LocalPlayer` is
/// put there and stepped again by the input sent since, which mostly lands where it already was.
fn reconcile(
    clock: Res<Clock>,
    time: Res<Time>,
    mut inbox: ResMut<Inbox>,
    mut prediction: ResMut<Prediction>,
    mut crowd: ResMut<Crowd>,
    mut commands: Commands,
    mut local: Query<
        (
            Entity,
            &mut Transform,
            &mut Locomotion,
            &Loadout,
            &Modifiers,
            Has<Drinking>,
            Has<LastStand>,
            Option<&mut TranslationEasingState>,
        ),
        With<LocalPlayer>,
    >,
    mut points: Query<(&PlayerSeat, &mut Points), With<Player>>,
    move_and_slide: MoveAndSlide,
) {
    let mut heard = std::mem::take(&mut inbox.snapshots);
    heard.retain(|snapshot| prediction.latest.is_none_or(|latest| snapshot.tick > latest));
    heard.sort_by_key(|snapshot| snapshot.tick);
    for snapshot in &heard {
        for sight in &snapshot.players {
            if let Some(seen) = prediction.seen.get_mut(usize::from(sight.seat)) {
                seen.push_back(Seen::of(snapshot.tick, sight));
            }
        }
        crowd.hear(snapshot);
    }
    prediction.corrected = 0.0;
    let Some(snapshot) = heard.pop() else { return };
    prediction.latest = Some(snapshot.tick);
    for (seat, mut points) in &mut points {
        let balance = snapshot.players.iter().find(|sight| sight.seat == seat.0).map(|sight| sight.points);
        if let Some(balance) = balance.map(u64::from)
            && points.balance != balance
        {
            points.balance = balance;
        }
    }
    let Ok((entity, mut transform, mut locomotion, loadout, modifiers, drinking, last_stand, easing)) =
        local.single_mut()
    else {
        return;
    };
    if let Ok((_, mut own)) = points.get_mut(entity) {
        own.balance = u64::from(snapshot.own.points);
    }
    // A word that has the Player stepped further than this game has (in the place of ticks the
    // Host hadn't heard while this game was stopped) is of a moment this game hasn't come to.
    if snapshot.used >= clock.tick {
        return;
    }
    let was = transform.translation;
    let mut position = was;
    let shape_was = locomotion.shape();
    prediction.sent.retain(|(tick, ..)| *tick > snapshot.used);
    // Back to how this game had the Player just after the Host's last used tick, when nothing has
    // come since to say otherwise. A Down or its end since then is the reliable events' to say,
    // so across one the Player is left as they are.
    if let Some((.., after_used)) = prediction.sent.front()
        && after_used.down == locomotion.down
    {
        *locomotion = after_used.clone();
    }
    snapshot.own.put(&mut position, &mut locomotion);
    let mut context = MoveContext { entity, loadout, modifiers, drinking, last_stand };
    for (_, step, _) in &prediction.sent {
        step_move(&mut position, &mut locomotion, &step.input(), &mut context, time.delta(), &move_and_slide);
    }
    transform.translation = position;
    if locomotion.shape() != shape_was {
        commands.entity(entity).insert(locomotion.shape().collider());
    }
    prediction.corrected = was.distance(position);
    // The tick's easing starts from where the Player now is, so it doesn't spread the correction
    // over the tick as well: the gap is all in `drawn`, eased out from where it was drawn. A gap
    // too wide is a move, shown at once.
    if let Some(mut easing) = easing
        && easing.start.is_some()
    {
        easing.start = Some(position);
    }
    let drawn = prediction.drawn + (was - position);
    prediction.drawn = if drawn.length() > CORRECTION_SNAP_METRES { Vec3::ZERO } else { drawn };
}

/// Draws the `LocalPlayer` this frame where it is plus what is left of the corrections, which
/// shrinks a little every frame. Only the drawn Transform moves: the easing has just written it,
/// and puts it back where the Player is before the next tick steps it, so the step never sees
/// the gap. While there is no easing to put it back, nothing is drawn off.
fn draw_correction(
    time: Res<Time>,
    mut prediction: ResMut<Prediction>,
    local: Single<(&mut Transform, &TranslationEasingState), With<LocalPlayer>>,
) {
    prediction.drawn *= (-time.delta_secs() / CORRECTION_EASE_SECS).exp();
    if prediction.drawn.length_squared() < 1e-8 {
        prediction.drawn = Vec3::ZERO;
        return;
    }
    let (mut transform, easing) = local.into_inner();
    if easing.start.is_some() && easing.end.is_some() {
        transform.translation += prediction.drawn;
    }
}

/// Sends this tick's input, with the last few ticks' again in case they were lost.
fn send_inputs(
    clock: Res<Clock>,
    mut wire: ResMut<Wire>,
    mut prediction: ResMut<Prediction>,
    local: Single<(&mut MoveInput, &Locomotion, &Pose, &Shots), With<LocalPlayer>>,
) {
    let (mut input, locomotion, pose, shots) = local.into_inner();
    let step = Step::of(&input);
    // The Player is stepped by exactly what the Host is told.
    *input = step.input();
    prediction.sent.push_back((clock.tick, step, locomotion.clone()));
    while prediction.sent.len() > REPLAY_TICKS {
        prediction.sent.pop_front();
    }
    let steps: Vec<Step> =
        prediction.sent.iter().rev().take(INPUT_RESEND).rev().map(|(_, step, _)| *step).collect();
    wire.0.datagram(
        HOST,
        encode(&Inputs { last: clock.tick, steps, pose: pose.pack(), shots: shots.count() }),
    );
}

/// Moves the moment the puppets are shown at on by a tick, towards `wanted`. It runs evenly within
/// `PUPPET_SLACK_TICKS` of where it should be, and is drawn back gently when it has strayed
/// farther. Behind by more than `PUPPET_JUMP_TICKS`, it jumps to where it should be. Ahead, as it
/// is while snapshots come late, it slows to a stop (about 22 ticks ahead), and never runs
/// backwards: a puppet stepping back before it jumps on looks worse than one standing still.
fn advance(shown: f32, wanted: f32) -> f32 {
    let astray = wanted - (shown + 1.0);
    if astray > PUPPET_JUMP_TICKS || astray < -PUPPET_REWIND_TICKS {
        wanted
    } else if astray.abs() > PUPPET_SLACK_TICKS {
        shown + (1.0 + (astray - PUPPET_SLACK_TICKS.copysign(astray)) * 0.05).max(0.0)
    } else {
        shown + 1.0
    }
}

/// Puts the puppets where the Host had them a moment ago.
fn show_puppets(
    mut prediction: ResMut<Prediction>,
    mut puppets: Query<(&PlayerSeat, &mut Transform, &mut MoveInput, &mut Pose, &mut Shots), With<Puppet>>,
) {
    let Some(latest) = prediction.latest else { return };
    // The moment shown moves on a tick each tick. The Host's word comes every other tick, so
    // where the moment should be goes in twos; it is left to run evenly within that, and only
    // drawn back, gently, when it has strayed farther.
    let behind = (INTERPOLATION_SNAPSHOTS * SNAPSHOT_EVERY) as f32;
    prediction.shown = advance(prediction.shown, latest as f32 - behind);
    let shown = prediction.shown;
    for (seat, mut transform, mut input, mut pose, mut shots) in &mut puppets {
        let Some(seen) = prediction.seen.get_mut(usize::from(seat.0)) else { continue };
        forget_before(seen, shown);
        let Some(at) = show(seen, shown) else { continue };
        transform.translation = at.position;
        transform.rotation = Quat::from_rotation_y(at.yaw);
        input.yaw = at.yaw;
        input.pitch = at.pitch;
        pose.set_if_neq(at.pose);
        shots.set(at.shots);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(tick: Tick, x: f32) -> Seen {
        Seen { tick, position: Vec3::new(x, 0.0, 0.0), yaw: 0.0, pitch: 0.0, pose: Pose::default(), shots: 0 }
    }

    #[test]
    fn the_snapshot_rate_divides_the_tick_rate() {
        assert_eq!(FIXED_HZ % SNAPSHOT_HZ, 0);
        assert_eq!(Time::<Fixed>::default().timestep().as_secs_f64(), 1.0 / f64::from(FIXED_HZ));
    }

    /// A snapshot lost on the way shows nothing: the Player is where a straight line between its
    /// neighbours has them.
    #[test]
    fn a_dropped_snapshot_costs_nothing_visible() {
        let whole: VecDeque<Seen> = (0..4).map(|i| seen(i * 2, i as f32 * 0.2)).collect();
        let dropped: VecDeque<Seen> = [whole[0], whole[2]].into();
        for shown in [0.0, 0.5, 1.0, 2.0, 3.5] {
            let (mut whole, mut dropped) = (whole.clone(), dropped.clone());
            forget_before(&mut whole, shown);
            forget_before(&mut dropped, shown);
            let a = show(&whole, shown).unwrap().position;
            let b = show(&dropped, shown).unwrap().position;
            assert!(a.distance(b) < 1e-5, "at {shown}: {a} against {b}");
        }
    }

    #[test]
    fn a_puppet_is_shown_between_two_snapshots_and_held_at_the_newest() {
        let two: VecDeque<Seen> = [seen(10, 0.0), seen(12, 1.0)].into();
        assert_eq!(show(&two, 11.0).unwrap().position.x, 0.5);
        assert_eq!(show(&two, 20.0).unwrap().position.x, 1.0);
        assert_eq!(show(&two, 0.0).unwrap().position.x, 0.0);
        assert!(show(&VecDeque::new(), 1.0).is_none());
    }

    /// Snapshots stopping slow the moment shown to a stop, never back; when they come again it
    /// carries on forwards and settles back to where it should be.
    #[test]
    fn the_moment_shown_stops_for_late_snapshots_and_never_runs_back() {
        let (mut shown, mut wanted) = (100.0, 101.0);
        let mut was = shown;
        for tick in 0..400 {
            // Nothing comes for 200 ticks, then a snapshot every tick again, the first 12 on.
            if tick == 200 {
                wanted += 12.0;
            } else if tick > 200 {
                wanted += 1.0;
            }
            shown = advance(shown, wanted);
            assert!(shown >= was, "ran back from {was} to {shown} at {tick}");
            assert!(shown - wanted < PUPPET_REWIND_TICKS, "ran on to {shown} at {tick}");
            was = shown;
        }
        assert!((wanted - shown).abs() <= PUPPET_SLACK_TICKS + 1.0, "{shown} against {wanted}");
    }

    #[test]
    fn the_moment_shown_jumps_on_when_far_behind_and_back_when_far_ahead() {
        assert_eq!(advance(10.0, 30.0), 30.0);
        assert_eq!(advance(100.0, 50.0), 50.0);
        assert_eq!(advance(10.0, 11.0), 11.0);
    }

    #[test]
    fn a_puppet_turns_the_short_way_round() {
        let mut a = seen(0, 0.0);
        let mut b = seen(2, 0.0);
        (a.yaw, b.yaw) = (3.0, -3.0);
        let yaw = show(&[a, b].into(), 1.0).unwrap().yaw;
        assert!(yaw.abs() > 3.0, "went the long way to {yaw}");
    }
}
