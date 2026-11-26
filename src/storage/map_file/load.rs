//! Reading and writing a Map File's folder, and versioning `map.ron`.
//!
//! A Map File is untrusted data (a Player's own, or one received from a Host), so every size is
//! checked before it is read or kept, and no name in the file ever becomes a path except through
//! [`AssetRef::file_name`], which is made of a validated hash and an allowed extension.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use super::hash::MapHash;
use super::model::{AssetKind, AssetRef, FORMAT, MapFile};

/// `map.ron` itself is at most this big.
pub const MAX_MAP_RON_BYTES: u64 = 16 * 1024 * 1024;
/// A Map's assets together are at most this big (ADR 0011: 64 MB).
pub const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
/// A Map names at most this many assets.
pub const MAX_ASSETS: usize = 1024;
/// No section of a Map has more entries than this; it bounds what a file can make us allocate.
pub const MAX_ENTRIES: usize = 100_000;
const MAX_LINEAGE: usize = 64;
const MAX_NAME: usize = 100;
const MAX_DESCRIPTION: usize = 4000;

/// The file a Map's data is in.
pub const MAP_RON: &str = "map.ron";
/// The folder its assets are in.
pub const ASSETS_DIR: &str = "assets";

/// Where a loaded Map came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapSource {
    /// `assets/maps/`: comes with the game.
    Shipped,
    /// `~/.zombies/maps/<map-id>/`: the Player's own.
    Player,
    /// `~/.zombies/cache/<hash>/`: received in Co-op.
    Cache,
}

#[derive(Debug, thiserror::Error)]
pub enum MapFileError {
    #[error("couldn't read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{what} is {size} bytes, over the {limit} byte limit")]
    TooLarge { what: String, size: u64, limit: u64 },
    #[error("map.ron isn't a Map File: {0}")]
    Parse(String),
    #[error("this Map is format {found} but this game reads up to format {supported}: update the game")]
    NewerFormat { found: u32, supported: u32 },
    #[error("map.ron says format {0}, which no version of the game has read")]
    UnknownFormat(u32),
    #[error("migrating from format {from} failed: {reason}")]
    Migration { from: u32, reason: String },
    #[error("bad asset: {0}")]
    BadAsset(String),
    #[error("the Map names asset {0} but it isn't in assets/")]
    MissingAsset(String),
    #[error("asset {name} doesn't hash to its name (it is {actual})")]
    AssetHashMismatch { name: String, actual: MapHash },
    #[error("invalid Map File: {0}")]
    Invalid(String),
    #[error("the folder is named {folder} but the Map is {expected}")]
    WrongFolder { folder: String, expected: String },
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> MapFileError + '_ {
    move |source| MapFileError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A Map File loaded and checked, with its hash. Assets stay on disk under `dir`.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedMap {
    pub map: MapFile,
    pub hash: MapHash,
    pub dir: PathBuf,
    pub source: MapSource,
}

impl LoadedMap {
    /// Where one of this Map's assets is.
    pub fn asset_path(&self, asset: &AssetRef) -> PathBuf {
        self.dir.join(ASSETS_DIR).join(asset.file_name())
    }
}

// ---- versioning ----

/// Brings `map.ron` text of one format up to the next.
type Step = fn(&str) -> Result<String, String>;

/// `STEPS[n - 1]` migrates format `n` to `n + 1`. Format 1 is the first, so there are none yet. A
/// format bump adds its step here and a frozen sample of the format before it under
/// `tests/map_files/`.
const STEPS: &[Step] = &[];

#[derive(Deserialize)]
struct Header {
    format: u32,
}

/// Parses `map.ron` text of any format this build knows into a [`MapFile`], migrating one step at
/// a time. A newer format is refused.
pub fn parse(text: &str) -> Result<MapFile, MapFileError> {
    let map: MapFile = parse_with(text, FORMAT, STEPS)?;
    check(&map)?;
    Ok(map)
}

/// [`parse`] for any `current` format and its migration `steps`, so the stepping is testable
/// before a real second format exists.
fn parse_with<T: DeserializeOwned>(text: &str, current: u32, steps: &[Step]) -> Result<T, MapFileError> {
    let header: Header = ron::from_str(text).map_err(|e| MapFileError::Parse(e.to_string()))?;
    let mut format = header.format;
    if format > current {
        return Err(MapFileError::NewerFormat { found: format, supported: current });
    }
    if format == 0 {
        return Err(MapFileError::UnknownFormat(format));
    }
    let mut text = std::borrow::Cow::Borrowed(text);
    while format < current {
        let step = steps
            .get(format as usize - 1)
            .ok_or(MapFileError::UnknownFormat(format))?;
        let migrated = step(&text).map_err(|reason| MapFileError::Migration { from: format, reason })?;
        format += 1;
        let after: Header =
            ron::from_str(&migrated).map_err(|e| MapFileError::Migration { from: format - 1, reason: e.to_string() })?;
        if after.format != format {
            return Err(MapFileError::Migration {
                from: format - 1,
                reason: format!("the step left format {} instead of {format}", after.format),
            });
        }
        text = std::borrow::Cow::Owned(migrated);
    }
    ron::from_str(&text).map_err(|e| MapFileError::Parse(e.to_string()))
}

// ---- checks on a parsed file ----

fn too_long(what: &str, len: usize, limit: usize) -> MapFileError {
    MapFileError::Invalid(format!("{what} has {len} entries or characters, over the limit of {limit}"))
}

/// Whether a name is a name: not empty, within `limit`, no control characters.
fn check_text(what: &str, text: &str, limit: usize, allow_empty: bool) -> Result<(), MapFileError> {
    if text.chars().count() > limit {
        return Err(too_long(what, text.chars().count(), limit));
    }
    if text.trim().is_empty() && !allow_empty {
        return Err(MapFileError::Invalid(format!("{what} is empty")));
    }
    if text.chars().any(|c| c.is_control() && c != '\n') {
        return Err(MapFileError::Invalid(format!("{what} has control characters")));
    }
    Ok(())
}

fn check_asset(asset: &AssetRef) -> Result<(), MapFileError> {
    if !MapHash::is_hex(&asset.hash) {
        return Err(MapFileError::BadAsset(format!(
            "{:?} isn't a content hash (64 lowercase hex digits)",
            asset.hash.chars().take(80).collect::<String>()
        )));
    }
    if !asset.kind.extensions().contains(&asset.ext.as_str()) {
        return Err(MapFileError::BadAsset(format!(
            "{} is not a {:?} file type",
            asset.ext.chars().take(16).collect::<String>(),
            asset.kind
        )));
    }
    Ok(())
}

/// The checks every Map File passes however it arrived: sizes and names. Whether the Map can be
/// played is the validator's (#550), not this.
pub fn check(map: &MapFile) -> Result<(), MapFileError> {
    if map.format != FORMAT {
        return Err(MapFileError::UnknownFormat(map.format));
    }
    check_text("name", &map.name, MAX_NAME, false)?;
    check_text("creator", &map.creator, MAX_NAME, false)?;
    check_text("description", &map.description, MAX_DESCRIPTION, true)?;
    if map.lineage.len() > MAX_LINEAGE {
        return Err(too_long("lineage", map.lineage.len(), MAX_LINEAGE));
    }
    for entry in &map.lineage {
        check_text("a lineage creator", &entry.creator, MAX_NAME, false)?;
        check_text("a lineage name", &entry.name, MAX_NAME, false)?;
    }
    if map.assets.len() > MAX_ENTRIES.min(MAX_ASSETS) {
        return Err(too_long("assets", map.assets.len(), MAX_ASSETS));
    }
    for asset in &map.assets {
        check_asset(asset)?;
    }
    if let Some(thumbnail) = &map.thumbnail
        && !map.assets.iter().any(|a| a.kind == AssetKind::Texture && &a.hash == thumbnail)
    {
        return Err(MapFileError::BadAsset("the thumbnail isn't one of the Map's texture assets".into()));
    }
    let sections = [
        ("zones", map.zones.len()),
        ("walls", map.walls.len()),
        ("windows", map.windows.len()),
        ("doors", map.doors.len()),
        ("openings", map.openings.len()),
        ("spawn points", map.spawn_points.len()),
        ("seats", map.seats.len()),
        ("props", map.props.len()),
        ("lamps", map.lamps.len()),
    ];
    for (what, len) in sections {
        if len > MAX_ENTRIES {
            return Err(too_long(what, len, MAX_ENTRIES));
        }
    }
    for zone in &map.zones {
        let vertices = zone.floor.vertices.len() + zone.infills.iter().map(|f| f.vertices.len()).sum::<usize>();
        let triangles = zone.floor.triangles.len() + zone.infills.iter().map(|f| f.triangles.len()).sum::<usize>();
        if vertices > MAX_ENTRIES || triangles > MAX_ENTRIES {
            return Err(too_long("a Zone's Footprint", vertices.max(triangles), MAX_ENTRIES));
        }
        for footprint in std::iter::once(&zone.floor).chain(&zone.infills) {
            let count = footprint.vertices.len();
            if footprint.triangles.iter().flatten().any(|&i| i as usize >= count) {
                return Err(MapFileError::Invalid("a Footprint triangle names a vertex it doesn't have".into()));
            }
        }
    }
    Ok(())
}

// ---- reading a folder ----

/// Reads `path` as UTF-8 text, refusing more than `limit` bytes without reading them.
fn read_limited(path: &Path, what: &str, limit: u64) -> Result<Vec<u8>, MapFileError> {
    let file = File::open(path).map_err(io_err(path))?;
    let size = file.metadata().map_err(io_err(path))?.len();
    if size > limit {
        return Err(MapFileError::TooLarge { what: what.to_string(), size, limit });
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(limit + 1).read_to_end(&mut bytes).map_err(io_err(path))?;
    if bytes.len() as u64 > limit {
        return Err(MapFileError::TooLarge { what: what.to_string(), size: bytes.len() as u64, limit });
    }
    Ok(bytes)
}

/// Hashes a file in chunks, so a 64 MB asset isn't held whole. Symlinks are refused: an asset is a
/// plain file inside the folder.
fn hash_asset(path: &Path, remaining: &mut u64) -> Result<MapHash, MapFileError> {
    use sha2::{Digest, Sha256};
    let meta = fs::symlink_metadata(path).map_err(io_err(path))?;
    if !meta.is_file() {
        return Err(MapFileError::BadAsset(format!("{} isn't a plain file", path.display())));
    }
    if meta.len() > *remaining {
        return Err(MapFileError::TooLarge {
            what: "the Map's assets".to_string(),
            size: meta.len(),
            limit: *remaining,
        });
    }
    let mut file = File::open(path).map_err(io_err(path))?.take(*remaining + 1);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(io_err(path))?;
        if n == 0 {
            break;
        }
        read += n as u64;
        hasher.update(&buf[..n]);
    }
    if read > *remaining {
        return Err(MapFileError::TooLarge { what: "the Map's assets".to_string(), size: read, limit: *remaining });
    }
    *remaining -= read;
    Ok(MapHash::from_bytes(hasher.finalize().into()))
}

/// Loads the Map File in `dir`: `map.ron`, and every asset it names checked against its hash.
pub fn load_dir(dir: &Path, source: MapSource) -> Result<LoadedMap, MapFileError> {
    let ron_bytes = read_limited(&dir.join(MAP_RON), MAP_RON, MAX_MAP_RON_BYTES)?;
    let text = std::str::from_utf8(&ron_bytes).map_err(|_| MapFileError::Parse("not UTF-8 text".into()))?;
    let map = parse(text)?;

    // The sizes first, so a Map over the cap is refused before any of it is read.
    let mut total = 0u64;
    for asset in &map.assets {
        let path = dir.join(ASSETS_DIR).join(asset.file_name());
        total += fs::symlink_metadata(&path).map_or(0, |meta| meta.len());
        if total > MAX_ASSET_BYTES {
            return Err(MapFileError::TooLarge { what: "the Map's assets".to_string(), size: total, limit: MAX_ASSET_BYTES });
        }
    }
    let mut remaining = MAX_ASSET_BYTES;
    let mut hashes = Vec::with_capacity(map.assets.len());
    for asset in &map.assets {
        let name = asset.file_name();
        let path = dir.join(ASSETS_DIR).join(&name);
        let actual = hash_asset(&path, &mut remaining).map_err(|e| match e {
            MapFileError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound => {
                MapFileError::MissingAsset(name.clone())
            }
            other => other,
        })?;
        if actual.to_string() != asset.hash {
            return Err(MapFileError::AssetHashMismatch { name, actual });
        }
        hashes.push(actual);
    }
    Ok(LoadedMap { hash: MapHash::of(&ron_bytes, hashes), map, dir: dir.to_path_buf(), source })
}

/// The folders under `root` that hold Maps, each loaded or refused with its reason. A folder
/// without a `map.ron` is skipped. Sorted by folder name, so the order is stable.
pub fn load_all(root: &Path, source: MapSource) -> Vec<(PathBuf, Result<LoadedMap, MapFileError>)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join(MAP_RON).exists())
        .collect();
    dirs.sort();
    dirs.into_iter()
        .map(|dir| {
            let loaded = load_dir(&dir, source);
            (dir, loaded)
        })
        .collect()
}

// ---- where Maps live ----

/// `~/.zombies/maps/`.
pub fn player_maps_dir(save_dir: &Path) -> PathBuf {
    save_dir.join("maps")
}

/// `~/.zombies/maps/<map-id>/`.
pub fn player_map_dir(save_dir: &Path, id: Uuid) -> PathBuf {
    player_maps_dir(save_dir).join(id.hyphenated().to_string())
}

/// `~/.zombies/cache/<hash>/`.
pub fn cache_map_dir(save_dir: &Path, hash: MapHash) -> PathBuf {
    save_dir.join("cache").join(hash.to_string())
}

/// The Shipped Maps in `assets_root/maps/` (the game's `assets/` folder).
pub fn load_shipped(assets_root: &Path) -> Vec<(PathBuf, Result<LoadedMap, MapFileError>)> {
    load_all(&assets_root.join("maps"), MapSource::Shipped)
}

/// The Player's own Maps. Each must sit in the folder named for its id, so a Map can't hide as
/// another's.
pub fn load_player_maps(save_dir: &Path) -> Vec<(PathBuf, Result<LoadedMap, MapFileError>)> {
    load_all(&player_maps_dir(save_dir), MapSource::Player)
        .into_iter()
        .map(|(dir, loaded)| {
            let loaded = loaded.and_then(|loaded| {
                let expected = loaded.map.id.hyphenated().to_string();
                let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if folder == expected {
                    Ok(loaded)
                } else {
                    Err(MapFileError::WrongFolder { folder, expected })
                }
            });
            (dir, loaded)
        })
        .collect()
}

/// A received Map by hash: it must hash to the name it is cached under.
pub fn load_cached(save_dir: &Path, hash: MapHash) -> Result<LoadedMap, MapFileError> {
    let loaded = load_dir(&cache_map_dir(save_dir, hash), MapSource::Cache)?;
    if loaded.hash != hash {
        return Err(MapFileError::WrongFolder { folder: hash.to_string(), expected: loaded.hash.to_string() });
    }
    Ok(loaded)
}

// ---- writing a folder ----

/// Writes `bytes` to `path` through a temporary file, so a crash can't leave half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_extension("tmp");
    let mut file = File::create(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temp, path)
}

/// Writes `map` as `dir/map.ron`, creating `dir`. Returns the Map's hash as saved, over the
/// assets the map names (which must already be in `dir/assets`; see [`store_asset`]). Saving is
/// always allowed: nothing here asks whether the Map can be played.
pub fn store(dir: &Path, map: &MapFile) -> Result<MapHash, MapFileError> {
    check(map)?;
    let text = ron::ser::to_string_pretty(map, ron::ser::PrettyConfig::default())
        .map_err(|e| MapFileError::Invalid(e.to_string()))?;
    fs::create_dir_all(dir.join(ASSETS_DIR)).map_err(io_err(dir))?;
    write_atomic(&dir.join(MAP_RON), text.as_bytes()).map_err(io_err(dir))?;
    let mut hashes = Vec::with_capacity(map.assets.len());
    for asset in &map.assets {
        hashes.push(asset.hash.parse().map_err(|_| MapFileError::BadAsset(asset.hash.clone()))?);
    }
    Ok(MapHash::of(text.as_bytes(), hashes))
}

/// Writes one asset into `dir/assets` under its content hash and returns the entry for `map.ron`.
pub fn store_asset(dir: &Path, kind: AssetKind, ext: &str, bytes: &[u8]) -> Result<AssetRef, MapFileError> {
    let asset = AssetRef { hash: MapHash::of_bytes(bytes).to_string(), kind, ext: ext.to_string() };
    check_asset(&asset)?;
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(MapFileError::TooLarge { what: "an asset".into(), size: bytes.len() as u64, limit: MAX_ASSET_BYTES });
    }
    let folder = dir.join(ASSETS_DIR);
    fs::create_dir_all(&folder).map_err(io_err(&folder))?;
    write_atomic(&folder.join(asset.file_name()), bytes).map_err(io_err(&folder))?;
    Ok(asset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::save::tests::TempDir;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn sample() -> MapFile {
        let mut map = MapFile::new(id(1), "Ada", "Test Yard");
        map.description = "A yard.".into();
        map.zones.push(super::super::model::Zone {
            name: "Yard".into(),
            floor: super::super::model::Footprint {
                vertices: vec![[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 4.0, 0.5]],
                triangles: vec![[0, 1, 2]],
            },
            top: 3.0,
            ..Default::default()
        });
        map
    }

    #[test]
    fn a_map_file_round_trips_through_save_and_load_unchanged() {
        let temp = TempDir::new("mapfile-roundtrip");
        let dir = temp.0.join("m");
        let mut map = sample();
        map.lineage.push(super::super::model::LineageEntry { id: id(9), creator: "Bo".into(), name: "Old Yard".into() });
        let texture = store_asset(&dir, AssetKind::Texture, "png", b"png bytes").unwrap();
        map.thumbnail = Some(texture.hash.clone());
        map.assets.push(texture);
        let saved = store(&dir, &map).unwrap();
        let loaded = load_dir(&dir, MapSource::Player).unwrap();
        assert_eq!(loaded.map, map);
        assert_eq!(loaded.hash, saved);
        // Saving what was loaded changes nothing, down to the bytes.
        let again = store(&dir, &loaded.map).unwrap();
        assert_eq!(again, saved);
    }

    #[test]
    fn the_hash_changes_when_map_ron_or_an_asset_changes_and_not_otherwise() {
        let temp = TempDir::new("mapfile-hash");
        let mut map = sample();
        let first = temp.0.join("a");
        let asset = store_asset(&first, AssetKind::Sound, "ogg", b"one").unwrap();
        map.assets.push(asset);
        let base = store(&first, &map).unwrap();

        // The same Map in another folder, saved again: the same hash.
        let copy = temp.0.join("b");
        store_asset(&copy, AssetKind::Sound, "ogg", b"one").unwrap();
        assert_eq!(store(&copy, &map).unwrap(), base);
        assert_eq!(store(&copy, &map).unwrap(), base);

        // A change to map.ron.
        let mut renamed = map.clone();
        renamed.name = "Other".into();
        assert_ne!(store(&copy, &renamed).unwrap(), base);

        // A change to an asset (a different file under a different name).
        let mut swapped = map.clone();
        swapped.assets = vec![store_asset(&copy, AssetKind::Sound, "ogg", b"two").unwrap()];
        assert_ne!(store(&copy, &swapped).unwrap(), base);
        assert_eq!(load_dir(&copy, MapSource::Player).unwrap().hash, store(&copy, &swapped).unwrap());
    }

    #[test]
    fn an_asset_changed_on_disk_is_refused() {
        let temp = TempDir::new("mapfile-tamper");
        let dir = temp.0.join("m");
        let mut map = sample();
        let asset = store_asset(&dir, AssetKind::Sound, "ogg", b"one").unwrap();
        let path = dir.join(ASSETS_DIR).join(asset.file_name());
        map.assets.push(asset);
        store(&dir, &map).unwrap();
        fs::write(&path, b"tampered").unwrap();
        assert!(matches!(load_dir(&dir, MapSource::Player), Err(MapFileError::AssetHashMismatch { .. })));
        fs::remove_file(&path).unwrap();
        assert!(matches!(load_dir(&dir, MapSource::Player), Err(MapFileError::MissingAsset(_))));
    }

    fn with_assets(names: &[(&str, AssetKind, &str)]) -> String {
        let mut map = sample();
        map.assets = names
            .iter()
            .map(|(hash, kind, ext)| AssetRef { hash: hash.to_string(), kind: *kind, ext: ext.to_string() })
            .collect();
        ron::ser::to_string(&map).unwrap()
    }

    #[test]
    fn asset_names_that_could_leave_the_assets_folder_are_refused() {
        let good = "a".repeat(64);
        for hash in ["../../outside", "/etc/passwd", "..\\x", "a/b", "", &format!("{good}/../x"), "A".repeat(64).as_str()] {
            let text = with_assets(&[(hash, AssetKind::Model, "glb")]);
            assert!(matches!(parse(&text), Err(MapFileError::BadAsset(_))), "{hash}");
        }
        for ext in ["../glb", "exe", "glb/x", "", "png"] {
            let text = with_assets(&[(&good, AssetKind::Model, ext)]);
            assert!(matches!(parse(&text), Err(MapFileError::BadAsset(_))), "{ext}");
        }
        assert!(parse(&with_assets(&[(&good, AssetKind::Model, "glb")])).is_ok());
    }

    #[test]
    fn a_symlinked_asset_is_refused() {
        #[cfg(unix)]
        {
            let temp = TempDir::new("mapfile-symlink");
            let dir = temp.0.join("m");
            let mut map = sample();
            let asset = store_asset(&dir, AssetKind::Sound, "ogg", b"one").unwrap();
            let path = dir.join(ASSETS_DIR).join(asset.file_name());
            map.assets.push(asset);
            store(&dir, &map).unwrap();
            let elsewhere = temp.0.join("elsewhere");
            fs::rename(&path, &elsewhere).unwrap();
            std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
            assert!(matches!(load_dir(&dir, MapSource::Player), Err(MapFileError::BadAsset(_))));
        }
    }

    #[test]
    fn sizes_are_checked_before_they_are_trusted() {
        let temp = TempDir::new("mapfile-sizes");
        let dir = temp.0.join("m");
        store(&dir, &sample()).unwrap();
        let big = File::create(dir.join(MAP_RON)).unwrap();
        big.set_len(MAX_MAP_RON_BYTES + 1).unwrap();
        assert!(matches!(load_dir(&dir, MapSource::Player), Err(MapFileError::TooLarge { .. })));

        let mut map = sample();
        map.name = "x".repeat(MAX_NAME + 1);
        assert!(matches!(check(&map), Err(MapFileError::Invalid(_))));
        let mut map = sample();
        map.zones[0].floor.triangles.push([0, 1, 7]);
        assert!(matches!(check(&map), Err(MapFileError::Invalid(_))));
    }

    #[test]
    fn assets_over_the_cap_together_are_refused() {
        let temp = TempDir::new("mapfile-cap");
        let dir = temp.0.join("m");
        let mut map = sample();
        // Sparse files: 40 MB each, two of them, so over 64 MB without writing it.
        for (n, ext) in [(1u8, "ogg"), (2, "ogg")] {
            let folder = dir.join(ASSETS_DIR);
            fs::create_dir_all(&folder).unwrap();
            let hash = format!("{:0>64}", n);
            File::create(folder.join(format!("{hash}.{ext}"))).unwrap().set_len(40 * 1024 * 1024).unwrap();
            map.assets.push(AssetRef { hash, kind: AssetKind::Sound, ext: ext.into() });
        }
        store(&dir, &map).unwrap();
        assert!(matches!(load_dir(&dir, MapSource::Player), Err(MapFileError::TooLarge { .. })));
    }

    // ---- versioning ----

    #[derive(Deserialize, Debug, PartialEq)]
    struct Two {
        format: u32,
        title: String,
    }

    /// A made-up format 1 → 2 step: `name` became `title`.
    fn rename_name(text: &str) -> Result<String, String> {
        Ok(text.replace("format: 1", "format: 2").replace("name:", "title:"))
    }

    fn broken(_: &str) -> Result<String, String> {
        Err("nope".into())
    }

    fn stuck(text: &str) -> Result<String, String> {
        Ok(text.to_string())
    }

    #[test]
    fn a_file_one_format_older_is_migrated() {
        let old = r#"(format: 1, name: "Yard")"#;
        let two: Two = parse_with(old, 2, &[rename_name]).unwrap();
        assert_eq!(two, Two { format: 2, title: "Yard".into() });
        // And the current format passes through untouched.
        let two: Two = parse_with(r#"(format: 2, title: "Yard")"#, 2, &[rename_name]).unwrap();
        assert_eq!(two.title, "Yard");
    }

    #[test]
    fn steps_chain_one_at_a_time() {
        fn bump(text: &str) -> Result<String, String> {
            let n: u32 = text.trim_start_matches("(format: ").split(',').next().unwrap().parse().unwrap();
            Ok(text.replace(&format!("format: {n}"), &format!("format: {}", n + 1)))
        }
        let three: Two = parse_with(r#"(format: 1, title: "t")"#, 3, &[bump, bump]).unwrap();
        assert_eq!(three.format, 3);
        let err = parse_with::<Two>(r#"(format: 1, title: "t")"#, 3, &[bump]).unwrap_err();
        assert!(matches!(err, MapFileError::UnknownFormat(2)));
    }

    #[test]
    fn a_failing_or_stuck_step_is_an_error_not_a_loop() {
        let old = r#"(format: 1, title: "t")"#;
        assert!(matches!(parse_with::<Two>(old, 2, &[broken]), Err(MapFileError::Migration { from: 1, .. })));
        assert!(matches!(parse_with::<Two>(old, 2, &[stuck]), Err(MapFileError::Migration { from: 1, .. })));
    }

    #[test]
    fn a_newer_format_is_refused_and_says_to_update() {
        let text = format!("(format: {}, name: \"x\", id: \"00000000-0000-0000-0000-000000000001\", creator: \"a\")", FORMAT + 1);
        let err = parse(&text).unwrap_err();
        assert!(matches!(err, MapFileError::NewerFormat { found, supported } if found == FORMAT + 1 && supported == FORMAT));
        assert!(err.to_string().contains("update the game"), "{err}");
        // Even a file whose other fields this build can't read says so first.
        assert!(matches!(parse("(format: 99, shapes: [1])"), Err(MapFileError::NewerFormat { .. })));
    }

    #[test]
    fn format_zero_and_garbage_are_refused() {
        assert!(matches!(parse("(format: 0)"), Err(MapFileError::UnknownFormat(0))));
        assert!(matches!(parse("not ron"), Err(MapFileError::Parse(_))));
        assert!(matches!(parse("()"), Err(MapFileError::Parse(_))));
    }

    // ---- frozen samples ----

    /// Every `tests/map_files/format_<n>/` is a Map File as that format wrote it, frozen. When a
    /// bump makes it an old format, it stays here and must keep loading.
    fn sample_dir(n: u32) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/map_files").join(format!("format_{n}"))
    }

    #[test]
    fn every_frozen_format_still_loads() {
        for n in 1..=FORMAT {
            let loaded = load_dir(&sample_dir(n), MapSource::Shipped).unwrap_or_else(|e| panic!("format {n}: {e}"));
            assert_eq!(loaded.map.format, FORMAT, "format {n} migrates to the current format");
            assert_eq!(loaded.map.name, "Frozen Yard");
        }
    }

    #[test]
    fn the_frozen_format_one_sample_keeps_its_hash() {
        // Pins the hash scheme: changing how a Map is hashed breaks every cache and Room.
        let loaded = load_dir(&sample_dir(1), MapSource::Shipped).unwrap();
        assert_eq!(loaded.map.zones.len(), 2);
        assert_eq!(loaded.map.assets.len(), 1);
        assert_eq!(loaded.hash.to_string(), FROZEN_FORMAT_1_HASH);
    }

    const FROZEN_FORMAT_1_HASH: &str = "4876142dd0f4afc9aea59af778e22b0b45145ed4101fc106457e446d8a482591";

    // ---- locations ----

    #[test]
    fn player_maps_load_from_their_id_folder_and_received_ones_from_their_hash() {
        let temp = TempDir::new("mapfile-places");
        let save = temp.0.as_path();
        let map = sample();
        let hash = store(&player_map_dir(save, map.id), &map).unwrap();
        let found = load_player_maps(save);
        assert_eq!(found.len(), 1);
        let loaded = found.into_iter().next().unwrap().1.unwrap();
        assert_eq!((loaded.hash, loaded.source), (hash, MapSource::Player));

        // A Map filed under another's id is refused.
        store(&player_maps_dir(save).join("not-its-id"), &map).unwrap();
        assert_eq!(load_player_maps(save).iter().filter(|(_, r)| matches!(r, Err(MapFileError::WrongFolder { .. }))).count(), 1);

        // The cache holds a Map under its hash, and nothing under a hash it doesn't have.
        assert!(load_cached(save, hash).is_err());
        store(&cache_map_dir(save, hash), &map).unwrap();
        assert_eq!(load_cached(save, hash).unwrap().source, MapSource::Cache);
        let other = MapHash::of_bytes(b"other");
        store(&cache_map_dir(save, other), &map).unwrap();
        assert!(matches!(load_cached(save, other), Err(MapFileError::WrongFolder { .. })));
    }

    #[test]
    fn shipped_maps_load_from_assets_maps() {
        let temp = TempDir::new("mapfile-shipped");
        assert!(load_shipped(&temp.0).is_empty());
        store(&temp.0.join("maps").join("yard"), &sample()).unwrap();
        let found = load_shipped(&temp.0);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1.as_ref().unwrap().source, MapSource::Shipped);
    }
}
