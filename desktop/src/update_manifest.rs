use super::Version;
use std::collections::BTreeMap;

pub const MAX_MANIFEST: u64 = 256 * 1024;
pub const MAX_PACKAGE: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
    pub es: Vec<String>,
    pub en: Vec<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub published_at: String,
    pub notes: Notes,
    pub release_url: String,
    #[serde(deserialize_with = "unique_assets")]
    pub assets: BTreeMap<String, Asset>,
}

fn unique_assets<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Asset>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, Asset>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("unique release targets")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut assets = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, Asset>()? {
                if assets.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("Duplicate release target"));
                }
            }
            Ok(assets)
        }
    }
    deserializer.deserialize_map(Visitor)
}

pub const TARGETS: &[(&str, &str)] = &[
    ("windows-x86_64", "PepoMote.exe"),
    ("linux-x86_64", "PepoMote-linux-x86_64"),
    ("linux-x86_64-appimage", "PepoMote-x86_64.AppImage"),
    ("macos-aarch64", "PepoMote-macOS.zip"),
    ("mobile-linux-aarch64", "PepoMote-Mobile-aarch64"),
    (
        "mobile-linux-aarch64-appimage",
        "PepoMote-Mobile-aarch64.AppImage",
    ),
    ("mobile-linux-aarch64-musl", "PepoMote-Mobile-aarch64-musl"),
    ("android-universal", "PepoMote.apk"),
    ("ios", "PepoMote.ipa"),
];

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err("Manifest too large".into());
        }
        let m: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let version = Version::parse(&m.version).ok_or("Invalid stable version")?;
        if m.schema != 1
            || version.to_string() != m.version
            || !utc_timestamp(&m.published_at)
            || m.release_url != super::release_url(&version)
            || m.assets.is_empty()
        {
            return Err("Invalid release metadata".into());
        }
        for notes in [&m.notes.es, &m.notes.en] {
            if notes.is_empty()
                || notes.len() > 8
                || notes.iter().any(|s| {
                    s.trim().is_empty()
                        || s.chars().count() > 280
                        || s.chars().any(char::is_control)
                })
            {
                return Err("Invalid release notes".into());
            }
        }
        for (target, asset) in &m.assets {
            let expected = TARGETS
                .iter()
                .find(|(key, _)| *key == target)
                .ok_or("Unknown release target")?
                .1;
            if asset.name != expected
                || asset.size == 0
                || asset.size > MAX_PACKAGE
                || asset.sha256.len() != 64
                || !asset.sha256.bytes().all(|c| c.is_ascii_hexdigit())
                || asset.url
                    != format!(
                        "https://github.com/{}/releases/download/v{version}/{expected}",
                        super::REPO
                    )
            {
                return Err("Invalid release asset".into());
            }
        }
        Ok(m)
    }
    pub fn version(&self) -> Version {
        Version::parse(&self.version).unwrap()
    }
}

fn utc_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return false;
    }
    let number = |a, z| {
        std::str::from_utf8(&b[a..z])
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
    };
    let (Some(y), Some(m), Some(d), Some(h), Some(min), Some(sec)) = (
        number(0, 4),
        number(5, 7),
        number(8, 10),
        number(11, 13),
        number(14, 16),
        number(17, 19),
    ) else {
        return false;
    };
    let days = match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    y >= 2020 && d >= 1 && d <= days && h < 24 && min < 60 && sec < 60
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn fixture() -> serde_json::Value {
        serde_json::json!({
            "schema": 1, "version": "1.11.0", "published_at": "2026-09-19T12:00:00Z",
            "notes": {"es": ["Mejora del mando."], "en": ["Controller improvements."]},
            "release_url": "https://github.com/pepitolas13/PepoMote/releases/tag/v1.11.0",
            "assets": {"windows-x86_64": {"name":"PepoMote.exe", "size":3,
                "sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "url":"https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.exe"}}
        })
    }
    fn parse(v: &serde_json::Value) -> Result<Manifest, String> {
        Manifest::parse(&serde_json::to_vec(v).unwrap())
    }
    #[test]
    fn accepts_the_release_contract() {
        assert_eq!(parse(&fixture()).unwrap().version(), Version([1, 11, 0]));
    }
    #[test]
    fn refuses_untrusted_or_mismatched_downloads() {
        for bad in [
            "https://evil.example/PepoMote.exe",
            "https://github.com/pepitolas13/PepoMote/releases/download/v1.10.0/PepoMote.exe",
            "https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.exe?x=1",
        ] {
            let mut v = fixture();
            v["assets"]["windows-x86_64"]["url"] = bad.into();
            assert!(parse(&v).is_err());
        }
    }
    #[test]
    fn rejects_invalid_versions_sizes_hashes_and_notes() {
        for version in ["v1.11.0", "1.11", "01.11.0", "1.11.0-beta", "1.11.0+1"] {
            let mut v = fixture();
            v["version"] = version.into();
            assert!(parse(&v).is_err());
        }
        for size in [0, MAX_PACKAGE + 1] {
            let mut v = fixture();
            v["assets"]["windows-x86_64"]["size"] = size.into();
            assert!(parse(&v).is_err());
        }
        let mut v = fixture();
        v["assets"]["windows-x86_64"]["sha256"] = "bad".into();
        assert!(parse(&v).is_err());
        let mut v = fixture();
        v["notes"]["es"] = serde_json::json!(["x".repeat(281)]);
        assert!(parse(&v).is_err());
        let mut v = fixture();
        v["notes"]["es"] = serde_json::json!(["embedded\nnewline"]);
        assert!(parse(&v).is_err());
        let mut v = fixture();
        v["published_at"] = "2026-02-31T12:00:00Z".into();
        assert!(parse(&v).is_err());
        let mut v = fixture();
        v["schema"] = 2.into();
        assert!(parse(&v).is_err());
        assert!(Manifest::parse(&vec![b' '; MAX_MANIFEST as usize + 1]).is_err());
    }
}
