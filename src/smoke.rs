//! The release smoke check: `ZOMBIES_SMOKE=1` starts the game as normal, loads one file of each
//! kind from each asset folder through the game's own asset server, and quits with exit code 0
//! once all of them have loaded, or 1 if one is missing, fails or is still loading after
//! `TIMEOUT_SECS`. The release workflow runs it from the unzipped package, so it proves the
//! packaged exe starts and finds the `assets/` beside it. It doesn't prove every file is sound: a
//! sound, for one, is only read, not decoded, until it plays.

use bevy::asset::{LoadState, LoadedUntypedAsset};
use bevy::asset::io::file::FileAssetReader;
use bevy::prelude::*;

/// How long the samples may take to load before the check gives up.
const TIMEOUT_SECS: f32 = 120.0;

/// Frames the game runs before the check reports, so it has really started (and an exit on the
/// first frame, which can leave the process hanging, is never asked for).
const MIN_FRAMES: u32 = 60;

/// The folders under `assets/` a release must carry.
const FOLDERS: [&str; 9] = ["audio", "decals", "enemies", "materials", "models", "props", "skies", "sprites", "weapons"];

/// The file kinds the game loads.
const KINDS: [&str; 4] = ["ktx2", "ogg", "gltf", "glb"];

/// From every file under `assets/` (relative, `/`-separated), the first of each loadable kind in
/// each of `FOLDERS`; or the folders that have none.
fn pick_samples(files: &[String]) -> Result<Vec<String>, Vec<&'static str>> {
    let mut sorted: Vec<&String> = files.iter().collect();
    sorted.sort();
    let mut picked = Vec::new();
    let mut empty = Vec::new();
    for folder in FOLDERS {
        let before = picked.len();
        for kind in KINDS {
            let first = sorted
                .iter()
                .find(|file| file.strip_prefix(folder).is_some_and(|rest| rest.starts_with('/')) && file.ends_with(&format!(".{kind}")));
            picked.extend(first.map(|file| file.to_string()));
        }
        if picked.len() == before {
            empty.push(folder);
        }
    }
    if empty.is_empty() { Ok(picked) } else { Err(empty) }
}

/// How far one sample has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Load {
    Loading,
    Loaded,
    Failed,
}

/// Where the check stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    Wait,
    Pass,
    /// Why it failed.
    Fail(String),
}

/// Where the check stands, given each sample's load and the seconds since the game started.
fn verdict(loads: &[(String, Load)], elapsed_secs: f32) -> Verdict {
    let named = |wanted: Load| loads.iter().filter(|(_, load)| *load == wanted).map(|(file, _)| file.as_str()).collect::<Vec<_>>().join(", ");
    let failed = named(Load::Failed);
    let loading = named(Load::Loading);
    if !failed.is_empty() {
        Verdict::Fail(format!("failed to load: {failed}"))
    } else if loading.is_empty() {
        Verdict::Pass
    } else if elapsed_secs > TIMEOUT_SECS {
        Verdict::Fail(format!("still loading after {TIMEOUT_SECS} s: {loading}"))
    } else {
        Verdict::Wait
    }
}

/// Turned on by `ZOMBIES_SMOKE`.
pub struct SmokePlugin;

impl SmokePlugin {
    /// The smoke check asked for by `ZOMBIES_SMOKE`, if any.
    pub fn from_env() -> Option<Self> {
        std::env::var("ZOMBIES_SMOKE").is_ok_and(|value| !value.is_empty() && value != "0").then_some(Self)
    }
}

impl Plugin for SmokePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Samples(Err("the smoke check never started".into()))).add_systems(Startup, start).add_systems(Update, check);
    }
}

/// The samples being loaded, or why there are none. An exit asked for in `Startup` is lost, so
/// `check` reports both.
#[derive(Resource)]
struct Samples(Result<Vec<(String, Handle<LoadedUntypedAsset>)>, String>);

/// Every file under `root`, relative to it and `/`-separated.
fn files_under(root: &std::path::Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                files.push(relative.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
            }
        }
    }
    files
}

fn finish(exit: &mut MessageWriter<AppExit>, verdict: Result<String, String>) {
    match verdict {
        Ok(message) => {
            println!("smoke: ok, {message}");
            exit.write(AppExit::Success);
        }
        Err(message) => {
            println!("smoke: FAILED, {message}");
            exit.write(AppExit::from_code(1));
        }
    }
}

fn start(mut samples: ResMut<Samples>, asset_server: Res<AssetServer>) {
    let root = FileAssetReader::get_base_path().join("assets");
    println!("smoke: build {}, looking for assets in {}", crate::net::protocol::BUILD, root.display());
    samples.0 = match pick_samples(&files_under(&root)) {
        Ok(picked) => Ok(picked.into_iter().map(|file| (file.clone(), asset_server.load_builder().load_untyped(file))).collect()),
        Err(empty) => Err(format!("nothing to load in {}", empty.join(", "))),
    };
}

fn check(
    samples: Res<Samples>,
    asset_server: Res<AssetServer>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
    mut frames: Local<u32>,
    mut done: Local<bool>,
) {
    *frames += 1;
    if *done || *frames < MIN_FRAMES {
        return;
    }
    let samples = match &samples.0 {
        Ok(samples) => samples,
        Err(why) => {
            *done = true;
            return finish(&mut exit, Err(why.clone()));
        }
    };
    let loads: Vec<(String, Load)> = samples
        .iter()
        .map(|(file, handle)| {
            let load = match asset_server.load_state(handle) {
                LoadState::Failed(_) => Load::Failed,
                LoadState::Loaded if asset_server.is_loaded_with_dependencies(handle) => Load::Loaded,
                _ => Load::Loading,
            };
            (file.clone(), load)
        })
        .collect();
    let elapsed = time.elapsed_secs();
    let result = match verdict(&loads, elapsed) {
        Verdict::Wait => return,
        Verdict::Pass => Ok(format!("{} samples loaded, {} frames in {elapsed:.1} s", loads.len(), *frames)),
        Verdict::Fail(why) => Err(why),
    };
    *done = true;
    finish(&mut exit, result);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_folder_with(file: &str) -> Vec<String> {
        FOLDERS.iter().map(|folder| format!("{folder}/{file}")).collect()
    }

    #[test]
    fn takes_the_first_file_of_each_kind_in_each_folder() {
        let mut files = every_folder_with("a/color.ktx2");
        files.extend(["models/b/model.gltf", "models/b/model.bin", "audio/zz.ogg", "audio/aa.ogg", "models/a/color.ktx2"].map(String::from));
        let picked = pick_samples(&files).unwrap();
        assert!(picked.contains(&"audio/aa.ogg".to_string()));
        assert!(!picked.contains(&"audio/zz.ogg".to_string()));
        assert!(picked.contains(&"models/b/model.gltf".to_string()));
        assert!(picked.contains(&"materials/a/color.ktx2".to_string()));
        assert!(!picked.iter().any(|file| file.ends_with(".bin")));
        assert_eq!(picked.len(), FOLDERS.len() + 2);
    }

    #[test]
    fn names_the_folders_with_nothing_to_load() {
        let mut files: Vec<String> = every_folder_with("a/color.ktx2").into_iter().filter(|file| !file.starts_with("skies/") && !file.starts_with("audio/")).collect();
        files.extend(["audio/readme.md", "audiobook/x.ogg"].map(String::from));
        assert_eq!(pick_samples(&files), Err(vec!["audio", "skies"]));
    }

    fn loads(each: &[(&str, Load)]) -> Vec<(String, Load)> {
        each.iter().map(|&(file, load)| (file.to_string(), load)).collect()
    }

    #[test]
    fn passes_once_every_sample_has_loaded() {
        let all = loads(&[("audio/a.ogg", Load::Loaded), ("skies/a.ktx2", Load::Loaded)]);
        assert_eq!(verdict(&all, 3.0), Verdict::Pass);
    }

    #[test]
    fn waits_while_a_sample_is_loading_within_the_timeout() {
        let some = loads(&[("audio/a.ogg", Load::Loaded), ("skies/a.ktx2", Load::Loading)]);
        assert_eq!(verdict(&some, 3.0), Verdict::Wait);
    }

    #[test]
    fn fails_naming_a_sample_that_failed() {
        let some = loads(&[("audio/a.ogg", Load::Failed), ("skies/a.ktx2", Load::Loading)]);
        assert_eq!(verdict(&some, 3.0), Verdict::Fail("failed to load: audio/a.ogg".into()));
    }

    #[test]
    fn fails_naming_the_samples_still_loading_at_the_timeout() {
        let some = loads(&[("audio/a.ogg", Load::Loaded), ("skies/a.ktx2", Load::Loading), ("models/b.glb", Load::Loading)]);
        assert_eq!(verdict(&some, 121.0), Verdict::Fail("still loading after 120 s: skies/a.ktx2, models/b.glb".into()));
    }
}
