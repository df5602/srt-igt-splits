use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

const SPLITS_FILE_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct DetectVersion {
    version: u32,
}

fn detect_splits_version(json: &str) -> Result<DetectVersion> {
    let version_info: DetectVersion = serde_json::from_str(json)
        .map_err(|e| anyhow::anyhow!("Failed to parse splits version: {}", e))?;

    if version_info.version > SPLITS_FILE_VERSION {
        bail!(
            "Splits version {} is newer than supported version {}",
            version_info.version,
            SPLITS_FILE_VERSION
        );
    }

    Ok(version_info)
}

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

#[cfg(test)]
mod tests {
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
    fn detect_rejects_maximum_version() {
        let json = format!(r#"{{ "version": {} }}"#, u32::MAX);
        let result = detect_splits_version(&json);
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
}
