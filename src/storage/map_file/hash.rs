//! Content hashes: how a Map File is known on the wire and in the cache.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// A SHA-256 digest. For one asset it is the hash of the file; for a Map it is [`MapHash::of`].
///
/// It is 32 bytes, `Copy` and `Eq`, so the Room listing and `Hello` carry it as `[u8; 32]`
/// ([`MapHash::as_bytes`], [`MapHash::from_bytes`]); in text (`map.ron`, the cache folder's name,
/// logs) it is 64 lowercase hex digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MapHash([u8; 32]);

/// Why a hash's text wasn't one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a hash is 64 lowercase hex digits")]
pub struct BadHash;

impl MapHash {
    /// The hash of a file's bytes: an asset's hash.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// A Map's hash: the hash of `map.ron` plus the hashes of its assets. Domain-tagged and
    /// length-prefixed so no `map.ron` can be made to look like another's hash input; the asset
    /// hashes are sorted and deduplicated, so the order `map.ron` lists them in doesn't matter
    /// beyond the bytes of `map.ron` itself.
    pub fn of(map_ron: &[u8], assets: impl IntoIterator<Item = MapHash>) -> Self {
        let mut assets: Vec<MapHash> = assets.into_iter().collect();
        assets.sort();
        assets.dedup();
        let mut hasher = Sha256::new();
        hasher.update(b"zombies-map-file\0v1\0");
        hasher.update((map_ron.len() as u64).to_le_bytes());
        hasher.update(map_ron);
        hasher.update((assets.len() as u64).to_le_bytes());
        for asset in assets {
            hasher.update(asset.0);
        }
        Self(hasher.finalize().into())
    }

    /// For `Hello` and the Room.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Whether `text` is shaped like a hash: 64 lowercase hex digits. Names built from a hash (an
    /// asset's file, a cache folder) are only made from text that passes this.
    pub fn is_hex(text: &str) -> bool {
        text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }
}

impl fmt::Display for MapHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for MapHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MapHash({self})")
    }
}

impl FromStr for MapHash {
    type Err = BadHash;

    fn from_str(text: &str) -> Result<Self, BadHash> {
        if !Self::is_hex(text) {
            return Err(BadHash);
        }
        let mut out = [0u8; 32];
        for (byte, pair) in out.iter_mut().zip(text.as_bytes().chunks(2)) {
            let digit = |c: u8| if c <= b'9' { c - b'0' } else { c - b'a' + 10 };
            *byte = digit(pair[0]) << 4 | digit(pair[1]);
        }
        Ok(Self(out))
    }
}

impl Serialize for MapHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for MapHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_round_trips_through_text_and_bytes() {
        let hash = MapHash::of_bytes(b"hello");
        assert_eq!(hash.to_string().parse::<MapHash>(), Ok(hash));
        assert_eq!(MapHash::from_bytes(*hash.as_bytes()), hash);
        assert_eq!(
            hash.to_string(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn text_that_is_not_a_hash_is_refused() {
        for bad in ["", "abc", "../../etc/passwd", &"A".repeat(64), &"g".repeat(64), &"a".repeat(65)] {
            assert_eq!(bad.parse::<MapHash>(), Err(BadHash), "{bad}");
        }
    }

    #[test]
    fn the_map_hash_depends_on_both_parts_and_not_on_listing_order() {
        let (a, b) = (MapHash::of_bytes(b"a"), MapHash::of_bytes(b"b"));
        let base = MapHash::of(b"map", [a, b]);
        assert_eq!(base, MapHash::of(b"map", [b, a, a]));
        assert_ne!(base, MapHash::of(b"map!", [a, b]));
        assert_ne!(base, MapHash::of(b"map", [a]));
        assert_ne!(base, MapHash::of(b"map", [a, MapHash::of_bytes(b"c")]));
    }
}
