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
    // Rounds
    RoundStart => "round_start", Root, Flat, 0.9;
    HellhoundRoundStart => "hellhound_round_start", Root, Flat, 0.9;

    // Player
    GunshotPistol => "gunshot_pistol", Root, Flat, 0.6;
    Hit => "hit", Root, Flat, 0.5;
    Kill => "kill", Root, Flat, 0.8;

    // Buying and the Map
    Buy => "buy", Root, Flat, 0.7;
    CantAfford => "cant_afford", Root, Flat, 0.6;
    PlankTear => "plank_tear", Root, Spatial, 0.9;
    PlankRebuild => "plank_rebuild", Root, Flat, 0.6;

    // Hellhounds
    HoundSpawn => "hound_spawn", Root, Spatial, 1.0;
    HoundBite => "hound_bite", Root, Flat, 0.8;
    HoundDeath => "hound_death", Root, Spatial, 0.8;

    // Nova Crawlers, and the Shellshock their gas (and later the Napalm and the Teleporter) gives
    Shellshock => "shellshock", Root, Flat, 0.8;
    NovaVent => "nova_vent", Root, Spatial, 1.0;
    NovaAttack => "nova_attack", Root, Flat, 0.7;
    NovaBurst => "nova_burst", Root, Spatial, 1.0;
    NovaGas => "nova_gas", Root, Spatial, 0.6;

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
    fn announcer_lines_live_in_their_own_folder() {
        assert_eq!(Cue::AnnouncerNuke.stem(), "announcer/announcer_nuke");
        assert_eq!(Cue::Hit.stem(), "hit");
    }
}
