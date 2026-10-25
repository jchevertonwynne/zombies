//! The Player's Settings: loaded from `settings.ron` at launch, applied immediately and saved on change.

use std::path::PathBuf;

use bevy::audio::{GlobalVolume, Volume};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::mode::Mode;
use crate::save::{self, SaveFile};
use crate::tuning::*;

#[derive(Resource, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Settings {
    pub version: u32,
    /// 1–10.
    pub sensitivity: u8,
    /// 0–100%.
    pub master_volume: u8,
    /// Vertical field of view in degrees, 65–110.
    pub fov: u8,
    pub invert_y: bool,
    /// The Mode last picked, so the Mode screen can start on it.
    pub last_mode: Mode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            sensitivity: SENSITIVITY_DEFAULT,
            master_volume: MASTER_VOLUME_DEFAULT,
            fov: FOV_DEFAULT,
            invert_y: false,
            last_mode: Mode::default(),
        }
    }
}

impl SaveFile for Settings {
    const FILE_NAME: &'static str = "settings.ron";
    const VERSION: u32 = 1;

    fn version(&self) -> u32 {
        self.version
    }
}

/// One adjustable line of the Settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Sensitivity,
    MasterVolume,
    Fov,
    InvertY,
}

impl Setting {
    pub const ALL: [Setting; 4] = [
        Setting::Sensitivity,
        Setting::MasterVolume,
        Setting::Fov,
        Setting::InvertY,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Setting::Sensitivity => "Sensitivity",
            Setting::MasterVolume => "Master Volume",
            Setting::Fov => "Field of View",
            Setting::InvertY => "Invert Y",
        }
    }
}

impl Settings {
    /// Pulls every value back into its range, so a hand-edited file can't break the game.
    pub fn clamped(mut self) -> Self {
        self.sensitivity = self.sensitivity.clamp(SENSITIVITY_MIN, SENSITIVITY_MAX);
        self.master_volume = self.master_volume.min(100);
        self.fov = self.fov.clamp(FOV_MIN, FOV_MAX);
        self
    }

    /// Steps a Setting up (`up`) or down one notch, stopping at the ends. Invert-Y flips either way.
    pub fn adjust(&mut self, setting: Setting, up: bool) {
        let step = |value: u8, step: u8, min: u8, max: u8| {
            if up {
                value.saturating_add(step).min(max)
            } else {
                value.saturating_sub(step).max(min)
            }
        };
        match setting {
            Setting::Sensitivity => {
                self.sensitivity = step(self.sensitivity, 1, SENSITIVITY_MIN, SENSITIVITY_MAX)
            }
            Setting::MasterVolume => {
                self.master_volume = step(self.master_volume, MASTER_VOLUME_STEP, 0, 100)
            }
            Setting::Fov => self.fov = step(self.fov, FOV_STEP, FOV_MIN, FOV_MAX),
            Setting::InvertY => self.invert_y = !self.invert_y,
        }
    }

    /// The value as the Settings screen shows it.
    pub fn display(&self, setting: Setting) -> String {
        match setting {
            Setting::Sensitivity => self.sensitivity.to_string(),
            Setting::MasterVolume => format!("{}%", self.master_volume),
            Setting::Fov => self.fov.to_string(),
            Setting::InvertY => if self.invert_y { "On" } else { "Off" }.to_string(),
        }
    }

    /// Turns raw mouse motion (pixels) into (yaw, pitch) radians to turn by.
    pub fn look_delta(&self, mouse: Vec2) -> Vec2 {
        let scale = MOUSE_SENSITIVITY * self.sensitivity as f32 / SENSITIVITY_DEFAULT as f32;
        let delta = mouse * scale;
        if self.invert_y {
            Vec2::new(delta.x, -delta.y)
        } else {
            delta
        }
    }

    pub fn fov_radians(&self) -> f32 {
        (self.fov as f32).to_radians()
    }

    pub fn volume(&self) -> Volume {
        Volume::Linear(self.master_volume as f32 / 100.0)
    }
}

/// Where save files go, or `None` to run without saving.
#[derive(Resource)]
pub struct SaveDir(pub Option<PathBuf>);

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let dir = save::save_dir();
        let settings = match &dir {
            Some(dir) => save::load::<Settings>(dir).clamped(),
            None => {
                warn!("No home directory found; Settings won't be saved.");
                Settings::default()
            }
        };
        app.insert_resource(SaveDir(dir))
            .insert_resource(GlobalVolume::new(settings.volume()))
            .insert_resource(settings)
            .add_systems(
                Update,
                (apply_volume, save_settings).run_if(resource_changed::<Settings>),
            );
    }
}

fn apply_volume(settings: Res<Settings>, mut global: ResMut<GlobalVolume>) {
    global.volume = settings.volume();
}

fn save_settings(settings: Res<Settings>, dir: Res<SaveDir>) {
    // Freshly loaded Settings are already on disk (or defaults); only changes get written.
    if settings.is_added() {
        return;
    }
    if let Some(dir) = &dir.0
        && let Err(err) = save::store(dir, &*settings)
    {
        warn!("Couldn't save Settings: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::tests::TempDir;

    #[test]
    fn defaults_match_the_spec() {
        let settings = Settings::default();
        assert_eq!(settings.sensitivity, 5);
        assert_eq!(settings.master_volume, 80);
        assert_eq!(settings.fov, 80);
        assert!(!settings.invert_y);
        assert_eq!(settings.last_mode, Mode::Classic);
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let dir = TempDir::new("settings-round-trip");
        let saved = Settings {
            sensitivity: 9,
            master_volume: 35,
            fov: 105,
            invert_y: true,
            last_mode: Mode::Gauntlet,
            ..default()
        };
        save::store(&dir.0, &saved).unwrap();
        assert_eq!(save::load::<Settings>(&dir.0), saved);
    }

    #[test]
    fn corrupt_settings_fall_back_to_defaults_and_are_kept() {
        let dir = TempDir::new("settings-corrupt");
        std::fs::write(dir.0.join("settings.ron"), "(version: 1, fov: \"wide\")").unwrap();
        assert_eq!(save::load::<Settings>(&dir.0), Settings::default());
        assert!(dir.0.join("settings.ron.bak").exists());
    }

    #[test]
    fn default_sensitivity_is_todays_feel() {
        let delta = Settings::default().look_delta(Vec2::new(10.0, -4.0));
        assert_eq!(delta, Vec2::new(10.0, -4.0) * MOUSE_SENSITIVITY);
    }

    #[test]
    fn sensitivity_scales_the_look() {
        let settings = Settings {
            sensitivity: 10,
            ..default()
        };
        assert_eq!(
            settings.look_delta(Vec2::X),
            Vec2::X * MOUSE_SENSITIVITY * 2.0
        );
    }

    #[test]
    fn invert_y_flips_only_the_pitch() {
        let settings = Settings {
            invert_y: true,
            ..default()
        };
        assert_eq!(
            settings.look_delta(Vec2::new(1.0, 1.0)),
            Vec2::new(1.0, -1.0) * MOUSE_SENSITIVITY
        );
    }

    #[test]
    fn adjusting_stops_at_the_ends() {
        let mut settings = Settings {
            sensitivity: 10,
            master_volume: 100,
            fov: 110,
            ..default()
        };
        for setting in [Setting::Sensitivity, Setting::MasterVolume, Setting::Fov] {
            settings.adjust(setting, true);
        }
        assert_eq!(
            (settings.sensitivity, settings.master_volume, settings.fov),
            (10, 100, 110)
        );

        let mut settings = Settings {
            sensitivity: 1,
            master_volume: 0,
            fov: 65,
            ..default()
        };
        for setting in [Setting::Sensitivity, Setting::MasterVolume, Setting::Fov] {
            settings.adjust(setting, false);
        }
        assert_eq!(
            (settings.sensitivity, settings.master_volume, settings.fov),
            (1, 0, 65)
        );
    }

    #[test]
    fn adjusting_steps_one_notch() {
        let mut settings = Settings::default();
        settings.adjust(Setting::Sensitivity, true);
        settings.adjust(Setting::MasterVolume, false);
        settings.adjust(Setting::Fov, true);
        settings.adjust(Setting::InvertY, false);
        assert_eq!(
            (settings.sensitivity, settings.master_volume, settings.fov),
            (6, 75, 85)
        );
        assert!(settings.invert_y);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let settings = Settings {
            sensitivity: 0,
            master_volume: 250,
            fov: 200,
            ..default()
        }
        .clamped();
        assert_eq!(
            (settings.sensitivity, settings.master_volume, settings.fov),
            (1, 100, 110)
        );
    }
}
