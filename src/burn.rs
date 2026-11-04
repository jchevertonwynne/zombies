//! Burn damage over time. Anything that sets Zombies alight sends `Ignite`: the Hades' pellets
//! now, and later the Blast Furnace and the Flamethrower. A burning Zombie takes its Burn's
//! damage every tick until the Burn runs out. Burn kills are Paid and never Headshots.

use bevy::prelude::*;

use crate::arsenal::Burn;
use crate::authority::HostSimulation;
use crate::state::Pause;
use crate::zombie::{Attack, DamageZombie, Dying, HitZone, Zombie};

/// Set a Zombie alight. `burn.damage` is already resolved through the stat-modifier seam.
#[derive(Message)]
pub struct Ignite {
    /// The Player who set it alight, who its burn credits.
    pub player: Entity,
    pub zombie: Entity,
    pub burn: Burn,
}

/// A Zombie on fire.
#[derive(Component, Debug)]
pub struct Burning {
    /// The Player who set it alight.
    player: Option<Entity>,
    burn: Burn,
    elapsed: f32,
    /// Ticks already dealt.
    ticks: u32,
}

impl Burning {
    pub fn new(burn: Burn) -> Self {
        Self {
            player: None,
            burn,
            elapsed: 0.0,
            ticks: 0,
        }
    }

    /// Ticks a Burn deals in all. The first lands one tick after ignition.
    fn total_ticks(&self) -> u32 {
        (self.burn.secs / self.burn.tick_secs).floor() as u32
    }

    /// The same Burn, credited to `player`.
    pub fn by(self, player: Entity) -> Self {
        Self { player: Some(player), ..self }
    }

    /// Being set alight again restarts the Burn, keeping the stronger damage. The latest to set it
    /// alight is credited.
    pub fn reignite(&mut self, burn: Burn, player: Entity) {
        let damage = self.burn.damage.max(burn.damage);
        *self = Self::new(Burn { damage, ..burn }).by(player);
    }

    /// Advance the Burn. Returns the damage due this frame (several ticks on a long frame).
    pub fn advance(&mut self, dt: f32) -> f32 {
        self.elapsed += dt;
        let due = ((self.elapsed / self.burn.tick_secs).floor() as u32).min(self.total_ticks());
        let new = due.saturating_sub(self.ticks);
        self.ticks = due;
        new as f32 * self.burn.damage
    }

    /// Whether the Burn has dealt every tick.
    pub fn done(&self) -> bool {
        self.ticks >= self.total_ticks()
    }
}

pub struct BurnPlugin;

impl Plugin for BurnPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Ignite>()
            .add_systems(Update, (ignite, burn).chain().in_set(HostSimulation).run_if(in_state(Pause::Running)));
    }
}

fn ignite(
    mut commands: Commands,
    mut requests: MessageReader<Ignite>,
    mut zombies: Query<Option<&mut Burning>, (With<Zombie>, Without<Dying>)>,
) {
    for Ignite { player, zombie, burn } in requests.read() {
        match zombies.get_mut(*zombie) {
            Ok(Some(mut burning)) => burning.reignite(*burn, *player),
            Ok(None) => {
                commands.entity(*zombie).insert(Burning::new(*burn).by(*player));
            }
            Err(_) => {}
        }
    }
}

fn burn(
    mut commands: Commands,
    mut burning: Query<(Entity, &mut Burning, Has<Dying>)>,
    mut damage: MessageWriter<DamageZombie>,
    time: Res<Time>,
) {
    for (zombie, mut burning, dying) in &mut burning {
        let due = if dying { 0.0 } else { burning.advance(time.delta_secs()) };
        if due > 0.0 {
            damage.write(DamageZombie {
                player: burning.player,
                zombie,
                damage: due,
                zone: HitZone::Torso,
                attack: Attack::Burn,
            });
        }
        if dying || burning.done() {
            commands.entity(zombie).remove::<Burning>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BURN: Burn = Burn {
        damage: 50.0,
        tick_secs: 0.5,
        secs: 3.0,
    };

    #[test]
    fn a_burn_ticks_its_damage_until_it_runs_out() {
        let mut burning = Burning::new(BURN);
        assert_eq!(burning.advance(0.4), 0.0, "the first tick lands a tick after ignition");
        assert_eq!(burning.advance(0.1), 50.0);
        let mut total = 50.0;
        for _ in 0..20 {
            total += burning.advance(0.5);
        }
        assert_eq!(total, 300.0, "six ticks of 50 over 3 s");
        assert!(burning.done());
    }

    #[test]
    fn a_long_frame_deals_every_tick_it_covers() {
        let mut burning = Burning::new(BURN);
        assert_eq!(burning.advance(1.6), 150.0);
        assert_eq!(burning.advance(10.0), 150.0, "never more than the Burn's ticks");
    }

    #[test]
    fn reigniting_restarts_the_burn_and_keeps_the_stronger_damage() {
        let mut burning = Burning::new(BURN);
        burning.advance(2.9);
        burning.reignite(Burn { damage: 20.0, ..BURN }, Entity::PLACEHOLDER);
        assert!(!burning.done());
        assert_eq!(burning.advance(3.0), 300.0, "a fresh six ticks at the stronger 50");
    }
}
