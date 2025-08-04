use std::fmt;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use tempfile::NamedTempFile;

use crate::in_game_time::InGameTime;
use crate::splits::{Splits, splits::Split};

/// Current version of splits file. Increment on breaking change and create migration.
const SPLITS_FILE_VERSION: u32 = 1;

/// Used for version detection. Any JSON containing a top-level "version" field will deserialize properly into this struct.
#[derive(Debug, Deserialize)]
struct DetectVersion {
    version: u32,
}

fn detect_splits_version(json: &str) -> Result<DetectVersion> {
    serde_json::from_str(json).map_err(|e| anyhow::anyhow!("Failed to parse splits version: {}", e))
}

// Wrapper around std::time::Duration that adds serialization / deserialization into a human-readable format.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct HmsDuration(pub Duration);

impl fmt::Display for HmsDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secs = self.0.as_secs();
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        write!(f, "{:01}:{:02}:{:02}", h, m, s)
    }
}

impl FromStr for HmsDuration {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_hms_duration(s).map(HmsDuration)
    }
}

fn parse_hms_duration(s: &str) -> Result<Duration, String> {
    let parts: Vec<_> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("Invalid format (expected H:MM:SS): '{}'", s));
    }

    let h = parts[0]
        .parse::<u64>()
        .map_err(|e| format!("Invalid hours '{}': {}", parts[0], e))?;
    let m = parts[1]
        .parse::<u64>()
        .map_err(|e| format!("Invalid minutes '{}': {}", parts[1], e))?;
    let s = parts[2]
        .parse::<u64>()
        .map_err(|e| format!("Invalid seconds '{}': {}", parts[2], e))?;

    if m >= 60 || s >= 60 {
        return Err(format!("Minutes or seconds out of range: '{}'", s));
    }

    Ok(Duration::from_secs(h * 3600 + m * 60 + s))
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct SplitsFileV1 {
    pub version: u32,
    pub splits: SplitsV1,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct SplitsV1 {
    pub splits: Vec<SplitV1>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct SplitV1 {
    pub name: String,
    pub percent: u32,

    #[serde_as(as = "DisplayFromStr")]
    pub duration: HmsDuration,
}

impl From<&Splits> for SplitsFileV1 {
    fn from(splits: &Splits) -> Self {
        SplitsFileV1 {
            version: SPLITS_FILE_VERSION,
            splits: SplitsV1 {
                splits: splits
                    .splits
                    .iter()
                    .map(|s| SplitV1 {
                        name: s.name.clone(),
                        percent: s.time.percent,
                        duration: HmsDuration(s.time.duration),
                    })
                    .collect(),
            },
        }
    }
}

fn from_v1(file_v1: SplitsFileV1, path: &Path) -> Splits {
    let splits = file_v1
        .splits
        .splits
        .into_iter()
        .map(|s| Split {
            name: s.name,
            time: InGameTime {
                percent: s.percent,
                duration: s.duration.0,
            },
        })
        .collect();

    Splits {
        splits,
        path: Some(path.to_path_buf()),
    }
}

pub fn load_from_file(path: &Path) -> Result<Splits> {
    let contents = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", path.display(), e))?;

    let version_info = detect_splits_version(&contents)?;

    match version_info.version {
        1 => {
            let file_v1: SplitsFileV1 = serde_json::from_str(&contents)?;
            Ok(from_v1(file_v1, path))
        }
        v => bail!("Unsupported version: {}", v),
    }
}

pub fn save_to_file(splits: &Splits, path: &Path) -> Result<()> {
    // Convert Splits → SplitsFileV1
    let file_v1 = SplitsFileV1::from(splits);

    // Serialize to pretty JSON
    let json = serde_json::to_string_pretty(&file_v1)?;

    // Create temp file in same directory
    let mut temp_file = NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid path: no parent directory"))?,
    )?;

    // Write JSON
    temp_file.write_all(json.as_bytes())?;
    temp_file.flush()?;

    // Persist atomically
    temp_file.persist(path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn detects_version_successfully() {
        let json = r#"{ "version": 1 }"#;

        let parsed: DetectVersion =
            serde_json::from_str(json).expect("Failed to deserialize version");
        assert_eq!(parsed.version, 1);
    }

    #[test]
    fn detect_fails_on_missing_version() {
        let json = r#"{}"#; // no version field present

        let result: Result<DetectVersion, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Deserialization should fail without version"
        );
    }

    #[test]
    fn load_rejects_maximum_version() {
        let json = format!(r#"{{ "version": {} }}"#, u32::MAX);
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), &json).unwrap();

        let result = load_from_file(temp_file.path());
        assert!(result.is_err(), "Should reject max version as too new");
    }

    #[test]
    fn round_trip_splits_file_v1_serialization() {
        let original = SplitsFileV1 {
            version: 1,
            splits: SplitsV1 {
                splits: vec![
                    SplitV1 {
                        name: "Buzz".to_string(),
                        percent: 18,
                        duration: HmsDuration(Duration::from_secs(25 * 60 + 43)), // 00:25:43
                    },
                    SplitV1 {
                        name: "Fireworks Factory 1".to_string(),
                        percent: 56,
                        duration: HmsDuration(Duration::from_secs(1 * 3600 + 37 * 60 + 48)), // 01:37:48
                    },
                ],
            },
        };

        let json = serde_json::to_string_pretty(&original).expect("Serialize failed");
        let parsed: SplitsFileV1 = serde_json::from_str(&json).expect("Deserialize failed");

        assert_eq!(parsed, original);
    }

    #[test]
    fn deserialize_malformed_duration_fails() {
        let bad_inputs = [
            r#"{
            "version": 1,
            "splits": {
                "splits": [
                    { "name": "Invalid", "percent": 10, "duration": "1h5m" }
                ]
            }
        }"#,
            r#"{
            "version": 1,
            "splits": {
                "splits": [
                    { "name": "Invalid", "percent": 20, "duration": "1:65:90" }
                ]
            }
        }"#,
            r#"{
            "version": 1,
            "splits": {
                "splits": [
                    { "name": "Invalid", "percent": 30, "duration": "" }
                ]
            }
        }"#,
        ];

        for (i, input) in bad_inputs.iter().enumerate() {
            let result = serde_json::from_str::<SplitsFileV1>(input);
            assert!(
                result.is_err(),
                "Malformed duration input {} should fail, but succeeded",
                i
            );
        }
    }

    #[test]
    fn deserialize_missing_or_null_duration_fails() {
        let bad_inputs = [
            // Missing duration
            r#"{
            "version": 1,
            "splits": {
                "splits": [
                    { "name": "NoDuration", "percent": 10 }
                ]
            }
        }"#,
            // Null duration
            r#"{
            "version": 1,
            "splits": {
                "splits": [
                    { "name": "NullDuration", "percent": 20, "duration": null }
                ]
            }
        }"#,
        ];

        for (i, input) in bad_inputs.iter().enumerate() {
            let result = serde_json::from_str::<SplitsFileV1>(input);
            assert!(
                result.is_err(),
                "Input {} with missing/null duration should fail, but succeeded",
                i
            );
        }
    }

    #[test]
    fn empty_splits_list_is_valid() {
        let json = r#"{
        "version": 1,
        "splits": {
            "splits": []
        }
    }"#;

        let parsed: Result<SplitsFileV1, _> = serde_json::from_str(json);
        assert!(parsed.is_ok(), "Empty splits list should be valid");
    }

    #[test]
    fn load_from_file_with_valid_v1_file() -> Result<()> {
        use std::fs::write;
        use tempfile::tempdir;

        let dir = tempdir()?;
        let file_path = dir.path().join("splits.json");

        let json = r#"{
        "version": 1,
        "splits": {
            "splits": [
                { "name": "Level 1", "percent": 10, "duration": "0:10:00" },
                { "name": "Boss Fight", "percent": 50, "duration": "1:00:00" }
            ]
        }
    }"#;

        write(&file_path, json)?;

        let splits = load_from_file(&file_path)?;

        assert_eq!(splits.splits.len(), 2);
        assert_eq!(splits.splits[0].name, "Level 1");
        assert_eq!(splits.splits[0].time.percent, 10);
        assert_eq!(splits.splits[0].time.duration, Duration::from_secs(10 * 60));
        assert_eq!(splits.splits[1].name, "Boss Fight");
        assert_eq!(splits.splits[1].time.percent, 50);
        assert_eq!(
            splits.splits[1].time.duration,
            Duration::from_secs(1 * 3600)
        );
        assert_eq!(splits.path.as_ref(), Some(&file_path));

        Ok(())
    }

    #[test]
    fn save_to_file_writes_valid_v1_splits() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("splits.json");

        let mut splits = Splits::new();
        splits.add_split(
            "Start".to_string(),
            InGameTime {
                percent: 25,
                duration: Duration::from_secs(5),
            },
        );
        splits.add_split(
            "End".to_string(),
            InGameTime {
                percent: 100,
                duration: Duration::from_secs(5 * 60),
            },
        );
        splits.path = Some(file_path.clone());

        splits.save_to_file()?;

        // Check that file exists and contains expected JSON
        let contents = fs::read_to_string(&file_path)?;
        assert!(contents.contains("\"version\": 1"));
        assert!(contents.contains("\"Start\""));
        assert!(contents.contains("\"End\""));

        Ok(())
    }

    #[test]
    fn save_then_load_round_trip() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("roundtrip_splits.json");

        let mut original_splits = Splits::new();
        original_splits.add_split(
            "Split 1".to_string(),
            InGameTime {
                percent: 25,
                duration: Duration::from_secs(600),
            },
        );
        original_splits.add_split(
            "Split 2".to_string(),
            InGameTime {
                percent: 75,
                duration: Duration::from_secs(1800),
            },
        );
        original_splits.path = Some(file_path.clone());

        // Save to file
        original_splits.save_to_file()?;

        // Load from file
        let loaded_splits = load_from_file(&file_path)?;

        // Assert they match (except path which is set during load)
        assert_eq!(loaded_splits.splits.len(), original_splits.splits.len());
        for (orig, loaded) in original_splits
            .splits
            .iter()
            .zip(loaded_splits.splits.iter())
        {
            assert_eq!(orig.name, loaded.name);
            assert_eq!(orig.time.percent, loaded.time.percent);
            assert_eq!(orig.time.duration, loaded.time.duration);
        }

        Ok(())
    }
}
