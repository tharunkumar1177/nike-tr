//! The published index this build of the app downloads. `pinned.json` is written only by the
//! library workflow's pull request.

use serde::Deserialize;

const PINNED_JSON: &str = include_str!("../pinned.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PinnedIndex {
    pub kicad_tag: String,
    pub release: String,
    /// The gzip-compressed index file.
    pub url: String,
    /// Lowercase hex SHA-256 of the compressed file.
    pub sha256: String,
    /// Size of the compressed file in bytes.
    pub size: u64,
}

impl PinnedIndex {
    /// The index pinned into this build.
    pub fn current() -> PinnedIndex {
        PinnedIndex::parse(PINNED_JSON).expect("crates/library/pinned.json is a valid pin")
    }

    pub fn parse(json: &str) -> Result<PinnedIndex, String> {
        let pin: PinnedIndex = serde_json::from_str(json).map_err(|err| err.to_string())?;
        if !pin.url.starts_with("https://") {
            return Err(format!("the pinned URL {} is not HTTPS", pin.url));
        }
        if pin.sha256.len() != 64
            || !pin
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(format!(
                "the pinned SHA-256 {} is not 64 lowercase hex digits",
                pin.sha256
            ));
        }
        if pin.size == 0 {
            return Err("the pinned size is zero".to_owned());
        }
        Ok(pin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_pin_is_valid() {
        let pin = PinnedIndex::parse(PINNED_JSON).unwrap();
        assert!(pin.url.ends_with("/library.db.gz"));
        assert!(pin.url.contains(&pin.release));
    }

    #[test]
    fn a_pin_with_a_bad_checksum_is_rejected() {
        let json = r#"{"kicad_tag":"10.0.7","release":"r","url":"https://example.org/library.db.gz","sha256":"ABC","size":1}"#;
        assert!(PinnedIndex::parse(json).is_err());
    }

    #[test]
    fn a_plain_http_pin_is_rejected() {
        let sha = "0".repeat(64);
        let json = format!(
            r#"{{"kicad_tag":"10.0.7","release":"r","url":"http://example.org/library.db.gz","sha256":"{sha}","size":1}}"#
        );
        assert!(PinnedIndex::parse(&json).is_err());
    }
}
