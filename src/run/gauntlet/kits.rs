//! Kits: how a Gauntlet Run starts. One is picked on the Mode screen before the Run. A Family Kit
//! sets the Player's `StartingLoadout`, counts as one held Boon of its Family for Offer bias, and
//! gives one small rule (never a stat bump, and it can't be toggled off). Standard is free, and each
//! Family Kit is earned by that Family's Commendation, never bought with Scrip.
//!
//! Like their Tracks, a Player's Kit helps only that Player, so in Co-op each machine's own Kit
//! applies to its own `LocalPlayer`.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use super::boon::Family;
use super::commendations::find as find_commendation;
use super::progress::GauntletFile;
use super::variety::Boost;
use crate::economy::buy::Purchased;
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::players::player::Player;
use crate::tuning::*;
use crate::ui::hud::{ROUND_RED, text};
use crate::ui::menu::{HINT_GREY, MenuButtonLook};
use crate::weapons::mystery_box::MysteryBox;
use crate::weapons::pack_a_punch::PackAPunch;
use crate::weapons::weapon::StartingLoadout;

/// The seven Kits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kit {
    #[default]
    Standard,
    Lead,
    Blade,
    Spark,
    Ember,
    Gold,
    Grit,
}

impl Kit {
    pub const ALL: [Kit; 7] = [Kit::Standard, Kit::Lead, Kit::Blade, Kit::Spark, Kit::Ember, Kit::Gold, Kit::Grit];

    /// The stable ID used in `gauntlet.ron` and Run Records.
    pub fn id(self) -> &'static str {
        match self {
            Kit::Standard => "standard",
            Kit::Lead => "lead",
            Kit::Blade => "blade",
            Kit::Spark => "spark",
            Kit::Ember => "ember",
            Kit::Gold => "gold",
            Kit::Grit => "grit",
        }
    }

    pub fn from_id(id: &str) -> Option<Kit> {
        Kit::ALL.into_iter().find(|kit| kit.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Kit::Standard => "Standard",
            Kit::Lead => "Lead",
            Kit::Blade => "Blade",
            Kit::Spark => "Spark",
            Kit::Ember => "Ember",
            Kit::Gold => "Gold",
            Kit::Grit => "Grit",
        }
    }

    /// The Family a Family Kit leans toward; `None` for Standard.
    pub fn family(self) -> Option<Family> {
        match self {
            Kit::Standard => None,
            Kit::Lead => Some(Family::Lead),
            Kit::Blade => Some(Family::Blade),
            Kit::Spark => Some(Family::Spark),
            Kit::Ember => Some(Family::Ember),
            Kit::Gold => Some(Family::Gold),
            Kit::Grit => Some(Family::Grit),
        }
    }

    /// What it gives, as the Kit picker shows it.
    pub fn blurb(self) -> &'static str {
        match self {
            Kit::Standard => "The usual start: an M1911.",
            Kit::Lead => "An M14 in place of the M1911, with a spare Magazine.",
            Kit::Blade => "A Bowie Knife. The first Boon Offer always has a Blade card.",
            Kit::Spark => "Pack-a-Punch costs 2,500 less, until the first one is bought.",
            Kit::Ember => "4 Lethals, and each Round restocks 2 more.",
            Kit::Gold => "The first Mystery Box spin is free, and never a Teddy Bear.",
            Kit::Grit => "One more Self-Revive.",
        }
    }

    /// The stable ID of the Commendation that earns this Kit, or `None` for Standard.
    pub fn commendation(self) -> Option<&'static str> {
        match self {
            Kit::Standard => None,
            Kit::Lead => Some("family_lead"),
            Kit::Blade => Some("family_blade"),
            Kit::Spark => Some("family_spark"),
            Kit::Ember => Some("family_ember"),
            Kit::Gold => Some("family_gold"),
            Kit::Grit => Some("family_grit"),
        }
    }

    /// Whether the Player can pick it: Standard always, a Family Kit once its Commendation is earned.
    pub fn is_earned(self, file: &GauntletFile) -> bool {
        self.commendation().is_none_or(|id| file.has_commendation(id))
    }

    /// The Loadout the Run opens with.
    pub fn starting_loadout(self) -> StartingLoadout {
        let usual = StartingLoadout::default();
        match self {
            Kit::Lead => {
                let magazine = KIT_LEAD_WEAPON.stats().magazine;
                StartingLoadout {
                    weapon: KIT_LEAD_WEAPON,
                    reserve: Some(KIT_LEAD_RESERVE_MAGAZINES * magazine),
                    ..usual
                }
            }
            Kit::Blade => StartingLoadout { bowie: true, ..usual },
            Kit::Ember => StartingLoadout { lethals: KIT_EMBER_STARTING_LETHALS, ..usual },
            Kit::Standard | Kit::Spark | Kit::Gold | Kit::Grit => usual,
        }
    }

    fn source(self) -> Source {
        Source(match self {
            Kit::Standard => "Kit: Standard",
            Kit::Lead => "Kit: Lead",
            Kit::Blade => "Kit: Blade",
            Kit::Spark => "Kit: Spark",
            Kit::Ember => "Kit: Ember",
            Kit::Gold => "Kit: Gold",
            Kit::Grit => "Kit: Grit",
        })
    }

    /// The stat-modifier entries its rule puts on the Player. Spark's and Gold's lapse once
    /// `spent`: the first Pack-a-Punch, or the first Mystery Box spin, was bought.
    pub fn entries(self, spent: bool) -> Vec<Modifier> {
        let entry = |stat, effect| Modifier { stat, source: self.source(), effect };
        match self {
            Kit::Spark if !spent => {
                vec![entry(Stat::PackAPunchPrice, Effect::Flat(-(KIT_SPARK_FIRST_PAP_DISCOUNT as f32)))]
            }
            Kit::Ember => vec![entry(Stat::LethalRestock, Effect::Flat(KIT_EMBER_RESTOCK_BONUS as f32))],
            Kit::Gold if !spent => vec![
                entry(Stat::BoxFreeSpinChance, Effect::Flat(1.0)),
                entry(Stat::TeddyChance, Effect::Mult(0.0)),
            ],
            Kit::Grit => vec![entry(Stat::SelfRevives, Effect::Flat(KIT_GRIT_SELF_REVIVES as f32))],
            _ => Vec::new(),
        }
    }
}

/// Whether `source` is a Kit's own, so the entries sync leaves everything else alone.
pub fn is_kit_source(source: Source) -> bool {
    Kit::ALL.iter().any(|kit| kit.source() == source)
}

/// The Kit the Run starts with: the one picked in the Armory if it's earned, otherwise Standard.
pub fn chosen_kit(file: &GauntletFile) -> Kit {
    file.kit
        .as_deref()
        .and_then(Kit::from_id)
        .filter(|kit| kit.is_earned(file))
        .unwrap_or_default()
}

/// The guarantee the Kit adds to a Boon Offer: the Blade Kit's first Offer always holds a Blade card.
pub fn first_offer_boost(kit: Kit, first: bool) -> Boost {
    if kit == Kit::Blade && first {
        Boost { families: vec![Family::Blade; KIT_BLADE_FIRST_OFFER_FAMILY_CARDS as usize], ..default() }
    } else {
        Boost::default()
    }
}

/// The Kit this Run's Player started with, and how far its one-off rules have gone. Only the
/// `LocalPlayer` has one besides Standard.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerKit {
    pub kit: Kit,
    /// The Kit's first Pack-a-Punch or Mystery Box spin has been bought, so its discount or free
    /// spin has lapsed.
    pub spent: bool,
    /// It was bought this frame; the rule lapses the next, once the buy has read the old entries.
    lapsing: bool,
    /// The first Boon Offer has been drawn.
    pub offered: bool,
    /// Mystery Box spins bought, for the Gold Kit's free ones.
    spins: u32,
    /// Taps on a Pack-a-Punch machine, for the Spark's first.
    pap_taps: u32,
}

impl PlayerKit {
    pub fn new(kit: Kit) -> Self {
        Self { kit, ..default() }
    }
}

/// A Spark Kit's first Pack-a-Punch (not a Repack) or a Gold Kit's first Mystery Box spin has been
/// bought: its rule lapses on the next frame.
pub fn spend_first_buys(
    mut purchases: MessageReader<Purchased>,
    machines: Query<(), With<PackAPunch>>,
    boxes: Query<(), With<MysteryBox>>,
    mut players: Query<&mut PlayerKit, With<Player>>,
) {
    // Last frame's buy has been read by now.
    for mut kit in &mut players {
        if kit.lapsing {
            kit.lapsing = false;
            kit.spent = true;
        }
    }
    for &Purchased { player, buyable } in purchases.read() {
        let Ok(mut kit) = players.get_mut(player) else {
            continue;
        };
        let first = match kit.kit {
            // The Player's first tap on a machine is its Upgrade: a Repack, Ammo Mod or collect
            // needs an Upgraded Weapon first (a hold is a different message). Counting taps, not
            // reading the Loadout, keeps this independent of when `use_machine` takes the Weapon.
            Kit::Spark if machines.contains(buyable) => {
                kit.pap_taps += 1;
                kit.pap_taps == 1
            }
            Kit::Gold if boxes.contains(buyable) => {
                kit.spins += 1;
                kit.spins >= KIT_GOLD_FREE_SPINS
            }
            _ => false,
        };
        if first && !kit.spent {
            kit.lapsing = true;
        }
    }
}

/// A Kit picker button on the Mode screen.
#[derive(Component, Clone, Copy)]
pub struct KitButton(Kit);

const KIT_BUTTON_IDLE: Color = Color::srgba(0.15, 0.15, 0.15, 0.9);

/// The Kit picker, shown on the Mode screen beside the Armory button while Gauntlet is highlighted:
/// a button per Kit with the picked one in red, a locked Kit greyed out, and a line saying what the
/// picked Kit gives, or which Commendation earns a locked one.
pub fn kit_picker(board: &mut ChildSpawnerCommands, file: &GauntletFile) {
    let picked = chosen_kit(file);
    board.spawn(text("KIT", 22.0, Color::WHITE));
    board
        .spawn(Node { column_gap: px(8.0), ..default() })
        .with_children(|row| {
            for kit in Kit::ALL {
                let colour = if kit == picked {
                    ROUND_RED
                } else if kit.is_earned(file) {
                    Color::WHITE
                } else {
                    HINT_GREY
                };
                row.spawn((
                    Button,
                    Hovered::default(),
                    MenuButtonLook,
                    KitButton(kit),
                    Node {
                        width: px(124.0),
                        height: px(36.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(KIT_BUTTON_IDLE),
                ))
                .with_child(text(kit.name(), 20.0, colour));
            }
        });
    board.spawn(text(kit_line(picked, file), 18.0, HINT_GREY));
}

/// What the line under the picker says: the picked Kit's gift.
fn kit_line(picked: Kit, file: &GauntletFile) -> String {
    let locked: Vec<String> = Kit::ALL
        .iter()
        .filter(|kit| !kit.is_earned(file))
        .filter_map(|kit| {
            let commendation = find_commendation(kit.commendation()?)?;
            Some(format!("{}: {}", kit.name(), commendation.name))
        })
        .collect();
    if locked.is_empty() {
        picked.blurb().to_string()
    } else {
        format!("{} Locked Kits are earned by a Commendation, e.g. {}.", picked.blurb(), locked[0])
    }
}

/// A click on a Kit button picks it for the next Run, if it's earned, and saves it with the file.
pub fn pick_kit(activate: On<Activate>, buttons: Query<&KitButton>, mut file: ResMut<GauntletFile>) {
    let Ok(&KitButton(kit)) = buttons.get(activate.entity) else {
        return;
    };
    if !kit.is_earned(&file) || chosen_kit(&file) == kit {
        return;
    }
    file.kit = Some(kit.id().to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::weapon::Loadout;
    use crate::modifier::resolve;
    use crate::weapons::arsenal::WeaponId;
    use crate::weapons::equipment::FragLimits;

    #[test]
    fn standard_starts_as_classic_does() {
        assert_eq!(Kit::Standard.starting_loadout(), StartingLoadout::default());
        assert!(Kit::Standard.entries(false).is_empty());
        assert!(Kit::Standard.is_earned(&GauntletFile::default()));
        assert_eq!(Kit::Standard.family(), None);
    }

    #[test]
    fn the_lead_kit_swaps_in_an_m14_with_one_spare_magazine() {
        let start = Kit::Lead.starting_loadout();
        assert_eq!(start.weapon, WeaponId::M14);
        assert_eq!(start.reserve, Some(8));
        let loadout = start.loadout();
        let held = loadout.held();
        assert_eq!((held.id, held.magazine, held.reserve), (WeaponId::M14, 8, 8));
        assert!(Kit::Lead.entries(false).is_empty());
    }

    #[test]
    fn the_blade_kit_starts_with_the_bowie_and_its_first_offer_has_a_blade_card() {
        let start = Kit::Blade.starting_loadout();
        assert!(start.bowie && start.knife().bowie);
        assert_eq!(start.loadout().held().id, WeaponId::M1911);
        assert_eq!(first_offer_boost(Kit::Blade, true).families, [Family::Blade]);
        assert!(first_offer_boost(Kit::Blade, false).is_none(), "only the first Offer");
        assert!(first_offer_boost(Kit::Lead, true).is_none());
    }

    #[test]
    fn the_spark_kit_discounts_the_first_pack_a_punch_only() {
        let before = Kit::Spark.entries(false);
        assert_eq!(resolve(Stat::PackAPunchPrice, 5000.0, &before), 2500.0);
        assert!(Kit::Spark.entries(true).is_empty());
        assert_eq!(Kit::Spark.starting_loadout(), StartingLoadout::default());
    }

    #[test]
    fn the_ember_kit_starts_with_four_lethals_and_restocks_to_the_cap() {
        let start = Kit::Ember.starting_loadout();
        assert_eq!(start.lethals, 4);
        assert_eq!(start.equipment().frags, 4);
        let limits = FragLimits::resolve(&Kit::Ember.entries(false));
        assert_eq!(limits.per_round, FRAG_PER_ROUND + KIT_EMBER_RESTOCK_BONUS);
        // From any low count, one Round's restock fills the Lethals to the 4 cap.
        assert_eq!(crate::weapons::equipment::frags_after_round(2, limits), FRAG_MAX);
        assert_eq!(Kit::Ember.entries(true).len(), 1, "it never lapses");
    }

    #[test]
    fn the_gold_kit_frees_and_teddy_proofs_the_first_spin_only() {
        let before = Kit::Gold.entries(false);
        assert!(crate::weapons::mystery_box::free_spin(&before, 0.99));
        assert_eq!(resolve(Stat::TeddyChance, 1.0, &before), 0.0);
        assert!(Kit::Gold.entries(true).is_empty());
    }

    #[test]
    fn the_grit_kit_gives_a_self_revive() {
        assert_eq!(resolve(Stat::SelfRevives, 0.0, &Kit::Grit.entries(false)), 1.0);
        assert_eq!(resolve(Stat::SelfRevives, 0.0, &Kit::Grit.entries(true)), 1.0);
    }

    #[test]
    fn a_family_kit_counts_as_a_boon_of_its_family_and_is_earned_by_its_commendation() {
        let mut file = GauntletFile::default();
        for family in Family::ALL {
            let kit = Kit::ALL.into_iter().find(|kit| kit.family() == Some(family)).expect("a Kit per Family");
            assert_eq!(kit.commendation(), Some(format!("family_{}", family.name().to_lowercase()).as_str()));
        }
        assert!(!Kit::Gold.is_earned(&file));
        file.kit = Some("gold".into());
        assert_eq!(chosen_kit(&file), Kit::Standard, "an unearned pick falls back");
        file.earn("family_gold", 100);
        assert!(Kit::Gold.is_earned(&file));
        assert_eq!(chosen_kit(&file), Kit::Gold);
        assert_eq!(chosen_kit(&GauntletFile { kit: Some("retired".into()), ..default() }), Kit::Standard);
    }

    #[test]
    fn kit_entries_are_never_stat_bumps_and_are_recognised_as_a_kits_own() {
        for kit in Kit::ALL {
            for entry in kit.entries(false) {
                assert!(is_kit_source(entry.source));
                assert!(
                    !matches!(entry.stat, Stat::Damage | Stat::MoveSpeed | Stat::MaxHealth),
                    "{kit:?} bumps a stat"
                );
            }
        }
        assert!(!is_kit_source(Source("Track: Move speed")));
    }
    /// A Player with `kit`, a Pack-a-Punch machine and a Mystery Box, ready for buys.
    fn buying(kit: Kit) -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        app.add_message::<Purchased>().add_systems(Update, spend_first_buys);
        let player = app.world_mut().spawn((Player, PlayerKit::new(kit), Loadout::new(WeaponId::M1911))).id();
        let machine = app.world_mut().spawn(PackAPunch::default()).id();
        let mystery_box = app.world_mut().spawn(MysteryBox { location: 0, price: 950, teddy: true, state: crate::weapons::mystery_box::BoxState::Idle }).id();
        (app, player, machine, mystery_box)
    }

    fn state(app: &App, player: Entity) -> PlayerKit {
        *app.world().get::<PlayerKit>(player).unwrap()
    }

    #[test]
    fn the_sparks_discount_lapses_a_frame_after_the_first_pack_a_punch_is_bought() {
        let (mut app, player, machine, mystery_box) = buying(Kit::Spark);
        // Other buys don't touch it.
        app.world_mut().write_message(Purchased { player, buyable: mystery_box });
        app.update();
        app.update();
        assert!(!state(&app, player).spent);
        app.world_mut().write_message(Purchased { player, buyable: machine });
        app.update();
        assert!(!state(&app, player).spent, "the buy itself still reads the old price");
        app.update();
        assert!(state(&app, player).spent);
        assert!(Kit::Spark.entries(state(&app, player).spent).is_empty());
    }

    #[test]
    fn only_the_first_pack_a_punch_tap_lapses_the_discount_whatever_the_loadout_says() {
        let (mut app, player, machine, _) = buying(Kit::Spark);
        // `use_machine` may already have taken the Weapon, or an Upgraded one may be held.
        app.world_mut().get_mut::<Loadout>(player).unwrap().held_mut().upgraded = true;
        app.world_mut().write_message(Purchased { player, buyable: machine });
        app.update();
        app.update();
        assert!(state(&app, player).spent, "the first tap is the first buy, not the Loadout's say");
        let (mut again, player, machine, _) = buying(Kit::Spark);
        again.world_mut().write_message(Purchased { player, buyable: machine });
        again.update();
        again.world_mut().write_message(Purchased { player, buyable: machine });
        again.update();
        assert_eq!(state(&again, player).pap_taps, 2);
    }

    #[test]
    fn the_golds_free_spin_lapses_after_the_first_spin_and_only_for_a_gold_kit() {
        let (mut app, player, _, mystery_box) = buying(Kit::Gold);
        app.world_mut().write_message(Purchased { player, buyable: mystery_box });
        app.update();
        app.update();
        assert!(state(&app, player).spent);
        let (mut other, player, _, mystery_box) = buying(Kit::Standard);
        other.world_mut().write_message(Purchased { player, buyable: mystery_box });
        other.update();
        other.update();
        assert!(!state(&other, player).spent);
    }
}
