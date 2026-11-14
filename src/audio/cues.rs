//! Every sound the game can play, as a named slot in the audio assets.
//!
//! Kept free of Bevy so the `gen-sounds` binary can share the list.

/// The audio folder inside the assets folder.
pub const AUDIO_DIR: &str = "audio";

/// Where a cue's files live, relative to the audio assets folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    Root,
    /// Lines spoken by the announcer, rendered by espeak-ng.
    Announcer,
}

/// How a cue is heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mix {
    /// UI, the Player's own sounds, stingers and the announcer.
    Flat,
    /// A source out in the world, heard from where it is.
    Spatial,
}

/// What the slot table knows about one cue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    /// The slot name: files are `<name>.ogg` or `<name>_1.ogg`, `<name>_2.ogg`…
    pub name: &'static str,
    pub folder: Folder,
    pub mix: Mix,
    /// Linear playback volume.
    pub gain: f32,
}

/// Declares the `Cue` enum, `Cue::ALL` and each cue's `Spec` from one table, so adding a
/// cue is one line here (plus its recipe in `gen-sounds` and its placeholder file).
macro_rules! cues {
    ($($cue:ident => $name:literal, $folder:ident, $mix:ident, $gain:literal;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Cue {
            $($cue,)*
        }

        impl Cue {
            /// Every cue, in declaration order.
            pub const ALL: &[Cue] = &[$(Cue::$cue,)*];

            pub const fn spec(self) -> Spec {
                match self {
                    $(Cue::$cue => Spec {
                        name: $name,
                        folder: Folder::$folder,
                        mix: Mix::$mix,
                        gain: $gain,
                    },)*
                }
            }
        }
    };
}

cues! {
    // Rounds and Gauntlet
    RoundStart => "round_start", Root, Flat, 0.9;
    RoundEnd => "round_end", Root, Flat, 0.9;
    HellhoundRoundStart => "hellhound_round_start", Root, Flat, 0.9;
    BoonOffer => "boon_offer", Root, Flat, 0.8;
    PactOffer => "pact_offer", Root, Flat, 0.8;
    OfferPick => "offer_pick", Root, Flat, 0.6;
    GameOver => "game_over", Root, Flat, 0.9;

    // Player. Each Weapon plays its class's gunshot at its own pitch.
    GunshotPistol => "gunshot_pistol", Root, Flat, 0.6;
    GunshotSmg => "gunshot_smg", Root, Flat, 0.55;
    GunshotRifle => "gunshot_rifle", Root, Flat, 0.6;
    GunshotShotgun => "gunshot_shotgun", Root, Flat, 0.7;
    GunshotLmg => "gunshot_lmg", Root, Flat, 0.6;
    GunshotSniper => "gunshot_sniper", Root, Flat, 0.75;
    GunshotLauncher => "gunshot_launcher", Root, Flat, 0.7;
    PapShotLayer => "pap_shot_layer", Root, Flat, 0.35;
    RayGun => "ray_gun", Root, Flat, 0.6;
    Thundergun => "thundergun", Root, Flat, 0.9;
    Flamethrower => "flamethrower", Root, Flat, 0.5;
    Explosion => "explosion", Root, Spatial, 1.0;
    Reload => "reload", Root, Flat, 0.5;
    EmptyClick => "empty_click", Root, Flat, 0.5;
    WeaponSwap => "weapon_swap", Root, Flat, 0.4;
    KnifeSwing => "knife_swing", Root, Flat, 0.5;
    KnifeHit => "knife_hit", Root, Flat, 0.7;
    Hit => "hit", Root, Flat, 0.5;
    Headshot => "headshot", Root, Flat, 0.6;
    Kill => "kill", Root, Flat, 0.8;
    PlayerHurt => "player_hurt", Root, Flat, 0.7;
    LowHealth => "low_health", Root, Flat, 0.5;
    Down => "down", Root, Flat, 0.9;
    SelfRevive => "self_revive", Root, Flat, 0.8;
    MonkeyBomb => "monkey_bomb", Root, Spatial, 0.8;
    ClaymorePlant => "claymore_plant", Root, Flat, 0.6;
    // Only teammates make footsteps; the Player's own are silent.
    Footstep => "footstep", Root, Spatial, 0.4;
    // One rustle for every stance change, both ways; a Dive's landing thuds instead.
    StanceRustle => "stance_rustle", Root, Flat, 0.35;
    DiveLand => "dive_land", Root, Flat, 0.6;
    // Only teammates drag along a Crawl; the Player's own, like their footsteps, is silent.
    CrawlDrag => "crawl_drag", Root, Spatial, 0.4;

    // Buying and the Map
    Buy => "buy", Root, Flat, 0.7;
    CantAfford => "cant_afford", Root, Flat, 0.6;
    DoorOpen => "door_open", Root, Spatial, 0.9;
    PlankTear => "plank_tear", Root, Spatial, 0.9;
    PlankRebuild => "plank_rebuild", Root, Spatial, 0.6;
    PowerOn => "power_on", Root, Flat, 0.9;
    PerkJingleJuggernog => "perk_jingle_juggernog", Root, Spatial, 0.7;
    PerkJingleSpeedCola => "perk_jingle_speed_cola", Root, Spatial, 0.7;
    PerkJingleDoubleTap => "perk_jingle_double_tap", Root, Spatial, 0.7;
    PerkJingleQuickRevive => "perk_jingle_quick_revive", Root, Spatial, 0.7;
    PerkDrink => "perk_drink", Root, Flat, 0.7;
    PerkMachineLeave => "perk_machine_leave", Root, Spatial, 0.9;
    BoxSpin => "box_spin", Root, Spatial, 0.8;
    BoxTeddy => "box_teddy", Root, Spatial, 0.9;
    BoxMove => "box_move", Root, Spatial, 0.9;
    FireSale => "fire_sale", Root, Spatial, 0.7;
    PapProcess => "pap_process", Root, Spatial, 0.8;
    PapReady => "pap_ready", Root, Spatial, 0.8;
    PowerupHum => "powerup_hum", Root, Spatial, 0.5;
    PowerupPickup => "powerup_pickup", Root, Flat, 0.7;
    NukeBlast => "nuke_blast", Root, Flat, 1.0;
    TrapActive => "trap_active", Root, Spatial, 0.6;
    TrapKill => "trap_kill", Root, Spatial, 0.8;
    TeleporterCharge => "teleporter_charge", Root, Spatial, 0.9;
    TeleporterFire => "teleporter_fire", Root, Spatial, 1.0;
    TeleporterArrive => "teleporter_arrive", Root, Spatial, 1.0;
    MainframeLink => "mainframe_link", Root, Spatial, 0.8;
    AmmoModBlastFurnace => "ammo_mod_blast_furnace", Root, Spatial, 0.9;
    AmmoModDeadWire => "ammo_mod_dead_wire", Root, Spatial, 0.9;
    AmmoModFireworks => "ammo_mod_fireworks", Root, Spatial, 0.9;
    AmmoModThunderWall => "ammo_mod_thunder_wall", Root, Spatial, 0.9;
    AmmoModTurned => "ammo_mod_turned", Root, Spatial, 0.9;
    // Gauntlet's own Proc Effects (spec #182, Appendix C6).
    ProcFrost => "proc_frost", Root, Spatial, 0.8;
    ProcBlast => "proc_blast", Root, Spatial, 0.9;
    // A Set Bonus tier reached, rising with the tier, shared by every Family.
    SetBonusTier2 => "set_bonus_tier_two", Root, Flat, 0.7;
    SetBonusTier4 => "set_bonus_tier_four", Root, Flat, 0.75;
    SetBonusTier6 => "set_bonus_tier_six", Root, Flat, 0.8;
    DuoTaken => "duo_taken", Root, Flat, 0.85;
    PowerDown => "power_down", Root, Spatial, 0.9;
    PowerRestore => "power_restore", Root, Flat, 0.8;
    // Heard Map-wide, wherever the Generator is.
    GeneratorHeld => "generator_held", Root, Flat, 0.9;
    GeneratorLost => "generator_lost", Root, Flat, 0.9;

    // Zombies. The idle vocals share a cap of voices, nearest first; attacks and deaths never wait.
    ZombieWalkerIdle => "zombie_walker_idle", Root, Spatial, 0.6;
    ZombieJoggerIdle => "zombie_jogger_idle", Root, Spatial, 0.6;
    ZombieSprinterIdle => "zombie_sprinter_idle", Root, Spatial, 0.6;
    ZombieAttack => "zombie_attack", Root, Spatial, 0.7;
    ZombieDeath => "zombie_death", Root, Spatial, 0.7;
    ZombieRiser => "zombie_riser", Root, Spatial, 0.8;
    CrawlerGib => "crawler_gib", Root, Spatial, 0.9;
    CrawlerVocal => "crawler_vocal", Root, Spatial, 0.6;
    CrawlerDrag => "crawler_drag", Root, Spatial, 0.4;

    // Hellhounds
    HoundIdle => "hound_idle", Root, Spatial, 0.6;
    HoundSpawn => "hound_spawn", Root, Spatial, 1.0;
    HoundBite => "hound_bite", Root, Spatial, 0.8;
    HoundDeath => "hound_death", Root, Spatial, 0.8;

    // Nova Crawlers, and the Shellshock their gas (and the Napalm and the Teleporter) gives
    Shellshock => "shellshock", Root, Flat, 0.8;
    NovaVocal => "nova_vocal", Root, Spatial, 0.6;
    NovaVent => "nova_vent", Root, Spatial, 1.0;
    NovaAttack => "nova_attack", Root, Spatial, 0.7;
    NovaBurst => "nova_burst", Root, Spatial, 1.0;
    NovaGas => "nova_gas", Root, Spatial, 0.6;

    // Napalm and Shrieker
    NapalmRise => "napalm_rise", Root, Spatial, 1.0;
    NapalmIdle => "napalm_idle", Root, Spatial, 0.6;
    NapalmHeat => "napalm_heat", Root, Flat, 0.5;
    NapalmWindup => "napalm_windup", Root, Spatial, 1.0;
    NapalmExplode => "napalm_explode", Root, Spatial, 1.0;
    FirePool => "fire_pool", Root, Spatial, 0.5;
    FireBurn => "fire_burn", Root, Flat, 0.6;
    ShriekerRise => "shrieker_rise", Root, Spatial, 0.9;
    ShriekerIdle => "shrieker_idle", Root, Spatial, 0.6;
    ShriekerWindup => "shrieker_windup", Root, Spatial, 1.0;
    ShriekerScream => "shrieker_scream", Root, Spatial, 1.0;
    ScreamHit => "scream_hit", Root, Flat, 0.5;
    ShriekerDeath => "shrieker_death", Root, Spatial, 1.0;
    ScreamPop => "scream_pop", Root, Spatial, 0.8;

    // The Sapper. Its idle and Sabotage loop for as long as they last.
    SapperSmash => "sapper_smash", Root, Spatial, 1.0;
    SapperIdle => "sapper_idle", Root, Spatial, 0.6;
    SapperStep => "sapper_step", Root, Spatial, 0.8;
    SapperSabotage => "sapper_sabotage", Root, Spatial, 0.9;
    SapperWindup => "sapper_windup", Root, Spatial, 1.0;
    SapperSlam => "sapper_slam", Root, Spatial, 1.0;
    HelmetOff => "helmet_off", Root, Spatial, 1.0;
    SapperStun => "sapper_stun", Root, Spatial, 0.9;
    SapperDeath => "sapper_death", Root, Spatial, 1.0;

    // Announcer. The Power-up lines play on pickup and the Hellhound line as its Round starts;
    // the Sapper's starts playing when the Sapper exists.
    AnnouncerMaxAmmo => "announcer_max_ammo", Announcer, Flat, 1.0;
    AnnouncerInstaKill => "announcer_insta_kill", Announcer, Flat, 1.0;
    AnnouncerDoublePoints => "announcer_double_points", Announcer, Flat, 1.0;
    AnnouncerNuke => "announcer_nuke", Announcer, Flat, 1.0;
    AnnouncerCarpenter => "announcer_carpenter", Announcer, Flat, 1.0;
    AnnouncerFireSale => "announcer_fire_sale", Announcer, Flat, 1.0;
    AnnouncerHellhoundRound => "announcer_hellhound_round", Announcer, Flat, 1.0;
    SapperWarning => "sapper_warning", Announcer, Flat, 1.0;
}

impl Cue {
    /// The cue's position in `Cue::ALL`.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The slot's path without extension, relative to the audio assets folder.
    pub fn stem(self) -> String {
        let spec = self.spec();
        match spec.folder {
            Folder::Root => spec.name.to_string(),
            Folder::Announcer => format!("announcer/{}", spec.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cue_sits_at_its_index() {
        for (i, cue) in Cue::ALL.iter().enumerate() {
            assert_eq!(cue.index(), i);
        }
    }

    #[test]
    fn slot_names_are_unique_and_lowercase() {
        let mut names: Vec<_> = Cue::ALL.iter().map(|cue| cue.spec().name).collect();
        assert!(
            names
                .iter()
                .all(|n| n.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        );
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Cue::ALL.len());
    }

    #[test]
    fn every_appendix_c_slot_is_declared() {
        const APPENDIX_C: &[&str] = &[
            "round_start", "round_end", "hellhound_round_start", "boon_offer", "pact_offer", "offer_pick", "game_over",
            "announcer_max_ammo", "announcer_insta_kill", "announcer_double_points", "announcer_nuke",
            "announcer_carpenter", "announcer_fire_sale", "announcer_hellhound_round", "sapper_warning",
            "gunshot_pistol", "gunshot_smg", "gunshot_rifle", "gunshot_shotgun", "gunshot_lmg", "gunshot_sniper",
            "gunshot_launcher", "pap_shot_layer", "ray_gun", "thundergun", "flamethrower", "explosion", "reload",
            "empty_click", "weapon_swap", "knife_swing", "knife_hit", "hit", "headshot", "kill", "player_hurt",
            "low_health", "down", "self_revive", "monkey_bomb", "claymore_plant", "shellshock",
            "buy", "cant_afford", "door_open", "plank_tear", "plank_rebuild", "power_on", "perk_jingle_juggernog",
            "perk_jingle_speed_cola", "perk_jingle_double_tap", "perk_jingle_quick_revive", "perk_drink",
            "perk_machine_leave", "box_spin", "box_teddy", "box_move", "fire_sale", "pap_process", "pap_ready",
            "powerup_hum", "powerup_pickup", "nuke_blast", "trap_active", "trap_kill", "teleporter_charge",
            "teleporter_fire", "teleporter_arrive", "mainframe_link", "ammo_mod_blast_furnace", "ammo_mod_dead_wire",
            "ammo_mod_fireworks", "ammo_mod_thunder_wall", "ammo_mod_turned", "power_down", "power_restore",
            "generator_held", "generator_lost",
            "zombie_walker_idle", "zombie_jogger_idle", "zombie_sprinter_idle", "zombie_attack", "zombie_death",
            "zombie_riser", "hound_idle", "hound_bite", "hound_death", "hound_spawn", "crawler_gib", "crawler_vocal",
            "crawler_drag", "nova_vocal", "nova_vent", "nova_attack", "nova_burst", "nova_gas", "napalm_rise",
            "napalm_idle", "napalm_heat", "napalm_windup", "napalm_explode", "fire_pool", "fire_burn", "shrieker_rise",
            "shrieker_idle", "shrieker_windup", "shrieker_scream", "scream_hit", "shrieker_death", "scream_pop",
            "sapper_smash", "sapper_idle", "sapper_step", "sapper_sabotage", "sapper_windup", "sapper_slam",
            "helmet_off", "sapper_stun", "sapper_death",
            // Not in the appendix: a teammate's footstep and Crawl drag, for Co-op, and the stance cues.
            "footstep", "stance_rustle", "dive_land", "crawl_drag",
            // Gauntlet v2's Proc cues (spec #182, Appendix C6).
            "proc_frost", "proc_blast",
            "set_bonus_tier_two", "set_bonus_tier_four", "set_bonus_tier_six", "duo_taken",
        ];
        let declared: Vec<_> = Cue::ALL.iter().map(|cue| cue.spec().name).collect();
        for slot in APPENDIX_C {
            assert!(declared.contains(slot), "{slot} isn't declared");
        }
        assert_eq!(declared.len(), APPENDIX_C.len(), "and nothing beyond the appendix");
    }

    #[test]
    fn the_zombies_are_all_heard_from_where_they_are() {
        // The appendix's spatial and flat split, spot-checked.
        for cue in [Cue::ZombieAttack, Cue::HoundBite, Cue::NovaAttack, Cue::PlankRebuild, Cue::DoorOpen, Cue::FirePool] {
            assert_eq!(cue.spec().mix, Mix::Spatial, "{cue:?}");
        }
        for cue in [Cue::NapalmHeat, Cue::ScreamHit, Cue::FireBurn, Cue::LowHealth, Cue::PowerRestore] {
            assert_eq!(cue.spec().mix, Mix::Flat, "{cue:?}");
        }
    }

    #[test]
    fn announcer_lines_live_in_their_own_folder() {
        assert_eq!(Cue::AnnouncerNuke.stem(), "announcer/announcer_nuke");
        assert_eq!(Cue::Hit.stem(), "hit");
    }
}
