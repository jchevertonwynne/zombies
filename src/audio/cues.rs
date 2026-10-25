//! Every sound the game can play, as a named slot in the audio assets.
//!
//! Kept free of Bevy so the `gen-sounds` binary can share the list.

/// The audio folder inside the assets folder.
pub const AUDIO_DIR: &str = "audio";

/// Where a cue's files live, relative to the audio assets folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    Root,
    #[expect(
        dead_code,
        reason = "the first announcer lines arrive with the Appendix C cues"
    )]
    Announcer,
}

/// How a cue is heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mix {
    /// UI, the Player's own sounds, stingers and the announcer.
    Flat,
    /// A source out in the world, heard from where it is.
    #[expect(
        dead_code,
        reason = "the first world sources arrive with the Appendix C cues"
    )]
    Spatial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    RoundStart,
    GunshotPistol,
    Hit,
    Kill,
}

impl Cue {
    pub const ALL: [Cue; 4] = [Cue::RoundStart, Cue::GunshotPistol, Cue::Hit, Cue::Kill];

    /// The slot name: files are `<name>.ogg` or `<name>_1.ogg`, `<name>_2.ogg`…
    pub fn name(self) -> &'static str {
        match self {
            Cue::RoundStart => "round_start",
            Cue::GunshotPistol => "gunshot_pistol",
            Cue::Hit => "hit",
            Cue::Kill => "kill",
        }
    }

    pub fn folder(self) -> Folder {
        match self {
            Cue::RoundStart | Cue::GunshotPistol | Cue::Hit | Cue::Kill => Folder::Root,
        }
    }

    pub fn mix(self) -> Mix {
        match self {
            Cue::RoundStart | Cue::GunshotPistol | Cue::Hit | Cue::Kill => Mix::Flat,
        }
    }

    /// Linear playback volume.
    pub fn gain(self) -> f32 {
        match self {
            Cue::RoundStart => 0.9,
            Cue::GunshotPistol => 0.6,
            Cue::Hit => 0.5,
            Cue::Kill => 0.8,
        }
    }

    /// The slot's path without extension, relative to the audio assets folder.
    pub fn stem(self) -> String {
        match self.folder() {
            Folder::Root => self.name().to_string(),
            Folder::Announcer => format!("announcer/{}", self.name()),
        }
    }
}
