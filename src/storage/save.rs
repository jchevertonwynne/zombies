//! The save-file layer: versioned RON files in the per-OS save directory.
//!
//! Loading never crashes. A missing file means defaults; a corrupt one (unreadable, unparseable or
//! of an unknown version) means defaults too, with the bad file kept beside it as `<name>.bak`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bevy::log::{info, warn};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A file the save layer can load and store.
pub trait SaveFile: Serialize + DeserializeOwned + Default {
    /// Its name inside the save directory, e.g. `settings.ron`.
    const FILE_NAME: &'static str;
    /// The `version` this build writes. A file with any other version is treated as corrupt.
    const VERSION: u32;

    /// The `version` field read from the file.
    fn version(&self) -> u32;

    /// The file brought up to this build's version from an older one's text, or `None` if the text
    /// isn't one this file knows how to migrate. The default migrates nothing.
    fn migrate(_text: &str) -> Option<Self> {
        None
    }
}

/// `~/.zombies/` on Linux and macOS, `%APPDATA%\zombies\` on Windows.
/// `None` if the environment doesn't say where home is; the game then runs without saving.
pub fn save_dir() -> Option<PathBuf> {
    save_dir_from(
        cfg!(windows),
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("APPDATA").map(PathBuf::from),
    )
}

fn save_dir_from(
    windows: bool,
    home: Option<PathBuf>,
    appdata: Option<PathBuf>,
) -> Option<PathBuf> {
    if windows {
        appdata.map(|dir| dir.join("zombies"))
    } else {
        home.map(|dir| dir.join(".zombies"))
    }
}

/// Parses a save file's text. An older version is migrated if the file knows how; `None` if it
/// doesn't parse or has a version nothing handles.
pub fn parse<T: SaveFile>(text: &str) -> Option<T> {
    ron::from_str::<T>(text)
        .ok()
        .filter(|value| value.version() == T::VERSION)
        .or_else(|| T::migrate(text))
}

/// Loads `T` from `dir`, falling back to defaults. A corrupt file is renamed to `.bak`.
pub fn load<T: SaveFile>(dir: &Path) -> T {
    let path = dir.join(T::FILE_NAME);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return T::default(),
        Err(err) => {
            // Can't read it at all (permissions, a directory in the way): leave it be.
            warn!("Couldn't read {}: {err}. Using defaults.", path.display());
            return T::default();
        }
    };
    if let Some(value) = std::str::from_utf8(&bytes).ok().and_then(parse) {
        return value;
    }
    let backup = backup_path(&path);
    match fs::rename(&path, &backup) {
        Ok(()) => warn!(
            "{} was corrupt; kept it as {}. Using defaults.",
            path.display(),
            backup.display()
        ),
        Err(err) => warn!(
            "{} was corrupt and couldn't be kept as .bak ({err}). Using defaults.",
            path.display()
        ),
    }
    T::default()
}

/// Writes `value` to `dir`, creating the directory if needed. The write goes to a temporary file
/// first and is renamed over the old one, so a crash mid-write can't leave a half-written file.
pub fn store<T: SaveFile>(dir: &Path, value: &T) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())
        .map_err(io::Error::other)?;
    let path = dir.join(T::FILE_NAME);
    let temp = path.with_extension("ron.tmp");
    fs::write(&temp, text)?;
    fs::rename(&temp, &path)?;
    info!("Saved {}", path.display());
    Ok(())
}

/// `settings.ron` → `settings.ron.bak`.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

#[cfg(test)]
pub(crate) mod tests {
    use serde::Deserialize;

    use super::*;

    /// A fresh, empty directory under the system temp dir, removed when dropped.
    pub(crate) struct TempDir(pub PathBuf);

    impl TempDir {
        pub(crate) fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("zombies-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Example {
        version: u32,
        value: i32,
    }

    impl Default for Example {
        fn default() -> Self {
            Self {
                version: Self::VERSION,
                value: 7,
            }
        }
    }

    impl SaveFile for Example {
        const FILE_NAME: &'static str = "example.ron";
        const VERSION: u32 = 2;

        fn version(&self) -> u32 {
            self.version
        }
    }

    #[test]
    fn a_missing_file_means_defaults_and_no_backup() {
        let dir = TempDir::new("missing");
        assert_eq!(load::<Example>(&dir.0), Example::default());
        assert!(!dir.0.join("example.ron.bak").exists());
    }

    #[test]
    fn a_missing_directory_means_defaults() {
        let dir = TempDir::new("no-dir");
        assert_eq!(load::<Example>(&dir.0.join("nowhere")), Example::default());
    }

    #[test]
    fn a_corrupt_file_means_defaults_and_is_kept_as_bak() {
        let dir = TempDir::new("corrupt");
        let path = dir.0.join("example.ron");
        fs::write(&path, "(version: 2, value: oops").unwrap();

        assert_eq!(load::<Example>(&dir.0), Example::default());
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(dir.0.join("example.ron.bak")).unwrap(),
            "(version: 2, value: oops"
        );
    }

    #[test]
    fn a_file_that_is_not_text_is_corrupt() {
        let dir = TempDir::new("binary");
        fs::write(dir.0.join("example.ron"), [0xff, 0xfe, 0x00, 0x9c]).unwrap();

        assert_eq!(load::<Example>(&dir.0), Example::default());
        assert!(dir.0.join("example.ron.bak").exists());
    }

    #[test]
    fn an_unknown_version_is_corrupt() {
        let dir = TempDir::new("version");
        fs::write(dir.0.join("example.ron"), "(version: 99, value: 1)").unwrap();

        assert_eq!(load::<Example>(&dir.0), Example::default());
        assert!(dir.0.join("example.ron.bak").exists());
    }

    #[test]
    fn a_new_corrupt_file_replaces_an_old_backup() {
        let dir = TempDir::new("rebackup");
        fs::write(dir.0.join("example.ron.bak"), "old").unwrap();
        fs::write(dir.0.join("example.ron"), "new").unwrap();

        load::<Example>(&dir.0);
        assert_eq!(
            fs::read_to_string(dir.0.join("example.ron.bak")).unwrap(),
            "new"
        );
    }

    #[test]
    fn a_stored_file_loads_back_unchanged() {
        let dir = TempDir::new("round-trip");
        let saved = Example {
            version: Example::VERSION,
            value: -42,
        };
        store(&dir.0.join("nested"), &saved).unwrap();

        assert_eq!(load::<Example>(&dir.0.join("nested")), saved);
        assert!(!dir.0.join("nested").join("example.ron.tmp").exists());
    }

    #[test]
    fn stored_files_carry_their_version() {
        let dir = TempDir::new("has-version");
        store(&dir.0, &Example::default()).unwrap();
        let text = fs::read_to_string(dir.0.join("example.ron")).unwrap();
        assert!(text.contains("version: 2"), "{text}");
    }

    #[test]
    fn the_save_directory_is_per_os() {
        let home = Some(PathBuf::from("/home/p"));
        let appdata = Some(PathBuf::from(r"C:\Users\p\AppData\Roaming"));
        assert_eq!(
            save_dir_from(false, home.clone(), appdata.clone()),
            Some(PathBuf::from("/home/p/.zombies"))
        );
        assert_eq!(
            save_dir_from(true, home, appdata),
            Some(PathBuf::from(r"C:\Users\p\AppData\Roaming").join("zombies"))
        );
        assert_eq!(save_dir_from(false, None, None), None);
    }

    #[test]
    fn the_backup_sits_beside_the_file() {
        assert_eq!(
            backup_path(Path::new("/x/settings.ron")),
            PathBuf::from("/x/settings.ron.bak")
        );
    }
}
