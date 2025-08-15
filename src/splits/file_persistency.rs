use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::splits::{Splits, splits::HistoricalSplit, splits::RunSummary, splits::Split};

/// Current version of splits file. Increment on breaking change and create migration.
const SPLITS_FILE_VERSION_V1: u32 = 1;
const SPLITS_FILE_VERSION_V2: u32 = 2;

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
struct HmsDuration(pub Duration);

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
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_hms_duration(s).map(HmsDuration)
    }
}

fn parse_hms_duration(s: &str) -> Result<Duration> {
    let parts: Vec<_> = s.split(':').collect();
    if parts.len() != 3 {
        bail!(format!("Invalid format (expected H:MM:SS): '{}'", s));
    }

    let h = parts[0]
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("Invalid hours '{}': {}", parts[0], e))?;
    let m = parts[1]
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("Invalid minutes '{}': {}", parts[1], e))?;
    let s = parts[2]
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("Invalid seconds '{}': {}", parts[2], e))?;

    if m >= 60 || s >= 60 {
        bail!(
            "Minutes or seconds out of range: {}:{}, expected 0..59",
            m,
            s
        );
    }

    Ok(Duration::from_secs(h * 3600 + m * 60 + s))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SplitsFileV1 {
    pub version: u32,
    pub splits: SplitsV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SplitsV1 {
    pub splits: Vec<SplitV1>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SplitV1 {
    pub name: String,
    pub percent: u32,

    #[serde_as(as = "Option<DisplayFromStr>")]
    pub duration: Option<HmsDuration>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct SplitsFileV2 {
    pub version: u32,
    pub splits: SplitsV2,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct SplitsV2 {
    pub personal_best: Option<RunSummaryV2>,
    pub runs: Vec<RunSummaryV2>,
    pub splits: Vec<SplitV2>,
}

impl From<SplitsV1> for SplitsV2 {
    fn from(v1: SplitsV1) -> Self {
        SplitsV2 {
            personal_best: None,
            runs: Vec::new(),
            splits: v1.splits.into_iter().map(|split| split.into()).collect(),
        }
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct SplitV2 {
    pub name: String,
    pub percent: u32,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub time: Option<HmsDuration>,
    pub history: Vec<HistoricalSplitV2>,
}

impl From<SplitV1> for SplitV2 {
    fn from(v1: SplitV1) -> Self {
        SplitV2 {
            name: v1.name,
            percent: v1.percent,
            time: v1.duration,
            history: Vec::new(),
        }
    }
}

impl From<&Split> for SplitV2 {
    fn from(s: &Split) -> Self {
        SplitV2 {
            name: s.name.clone(),
            percent: s.percent,
            time: s.time.map(HmsDuration),
            history: s.history.iter().map(|h| h.into()).collect(),
        }
    }
}

impl From<&SplitV2> for Split {
    fn from(sv2: &SplitV2) -> Self {
        Split {
            name: sv2.name.clone(),
            percent: sv2.percent,
            time: sv2.time.map(|h| h.0),
            history: sv2.history.iter().map(|h| h.into()).collect(),
        }
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct RunSummaryV2 {
    pub id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub final_time: Option<HmsDuration>,
}

impl From<&RunSummary> for RunSummaryV2 {
    fn from(run: &RunSummary) -> Self {
        RunSummaryV2 {
            id: run.id,
            start_time: run.start_time,
            end_time: run.end_time,
            final_time: run.final_time.map(HmsDuration),
        }
    }
}

impl From<&RunSummaryV2> for RunSummary {
    fn from(run_v2: &RunSummaryV2) -> Self {
        RunSummary {
            id: run_v2.id,
            start_time: run_v2.start_time,
            end_time: run_v2.end_time,
            final_time: run_v2.final_time.map(|h| h.0),
        }
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct HistoricalSplitV2 {
    pub run_id: Uuid,
    #[serde_as(as = "DisplayFromStr")]
    pub duration: HmsDuration,
}

impl From<&HistoricalSplit> for HistoricalSplitV2 {
    fn from(h: &HistoricalSplit) -> Self {
        HistoricalSplitV2 {
            run_id: h.run_id,
            duration: HmsDuration(h.duration),
        }
    }
}

impl From<&HistoricalSplitV2> for HistoricalSplit {
    fn from(hv2: &HistoricalSplitV2) -> Self {
        HistoricalSplit {
            run_id: hv2.run_id,
            duration: hv2.duration.0,
        }
    }
}

impl From<SplitsFileV1> for SplitsFileV2 {
    fn from(v1: SplitsFileV1) -> Self {
        SplitsFileV2 {
            version: SPLITS_FILE_VERSION_V2,
            splits: v1.splits.into(),
        }
    }
}

impl From<&Splits> for SplitsFileV2 {
    fn from(splits: &Splits) -> Self {
        SplitsFileV2 {
            version: SPLITS_FILE_VERSION_V2,
            splits: SplitsV2 {
                personal_best: splits.personal_best().map(|pb| pb.into()),
                runs: splits.runs().iter().map(|run| run.into()).collect(),
                splits: splits.splits().iter().map(|split| split.into()).collect(),
            },
        }
    }
}

fn from_v2(file_v2: SplitsFileV2, path: &Path) -> Splits {
    let personal_best = file_v2.splits.personal_best.map(|pb| (&pb).into());
    let runs = file_v2.splits.runs.iter().map(|run| run.into()).collect();
    let splits = file_v2
        .splits
        .splits
        .iter()
        .map(|split| split.into())
        .collect();
    Splits::create_with_history(path.to_path_buf(), personal_best, runs, splits)
}

pub fn load_from_file(path: &Path) -> Result<Splits> {
    let contents = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", path.display(), e))?;

    let version_info = detect_splits_version(&contents)?;

    match version_info.version {
        SPLITS_FILE_VERSION_V1 => {
            let file_v1: SplitsFileV1 = serde_json::from_str(&contents)?;
            Ok(from_v2(file_v1.into(), path))
        }
        SPLITS_FILE_VERSION_V2 => {
            let file_v2: SplitsFileV2 = serde_json::from_str(&contents)?;
            Ok(from_v2(file_v2, path))
        }
        v => bail!("Unsupported version: {}", v),
    }
}

pub fn save_to_file(splits: &Splits, path: &Path) -> Result<()> {
    // Convert Splits → SplitsFileV2
    let file_v2 = SplitsFileV2::from(splits);

    // Create temp file in same directory
    let temp_file = NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid path: no parent directory"))?,
    )?;

    // Serialize to pretty JSON
    serde_json::to_writer_pretty(&temp_file, &file_v2)?;
    temp_file.as_file().sync_all()?;

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
                        duration: Some(HmsDuration(Duration::from_secs(25 * 60 + 43))), // 00:25:43
                    },
                    SplitV1 {
                        name: "Fireworks Factory 1".to_string(),
                        percent: 56,
                        duration: Some(HmsDuration(Duration::from_secs(1 * 3600 + 37 * 60 + 48))), // 01:37:48
                    },
                ],
            },
        };

        let json = serde_json::to_string_pretty(&original).expect("Serialize failed");
        let parsed: SplitsFileV1 = serde_json::from_str(&json).expect("Deserialize failed");

        assert_eq!(parsed, original);
    }

    #[test]
    fn run_summary_to_v2_and_back() {
        let run = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(100),
            end_time: Some(Utc::now()),
            final_time: Some(Duration::from_secs(50)),
        };

        let v2: RunSummaryV2 = (&run).into();
        let restored: RunSummary = (&v2).into();

        assert_eq!(restored, run);
    }

    #[test]
    fn historical_split_to_v2_and_back() {
        let hist = HistoricalSplit {
            run_id: Uuid::new_v4(),
            duration: Duration::from_secs(123),
        };

        let v2: HistoricalSplitV2 = (&hist).into();
        let restored: HistoricalSplit = (&v2).into();

        assert_eq!(restored, hist);
    }

    #[test]
    fn split_to_v2_and_back() {
        let split = Split {
            name: "Test".to_string(),
            percent: 75,
            time: Some(Duration::from_secs(200)),
            history: vec![HistoricalSplit {
                run_id: Uuid::new_v4(),
                duration: Duration::from_secs(150),
            }],
        };

        let v2: SplitV2 = (&split).into();
        let restored: Split = (&v2).into();

        assert_eq!(restored, split);
    }

    #[test]
    fn runtime_to_v2_and_back_preserves_data() {
        // Create a sample runtime Splits with all persistable fields filled
        let run_id = Uuid::new_v4();
        let run_summary = RunSummary {
            id: run_id,
            start_time: Utc::now() - Duration::from_secs(1234),
            end_time: Some(Utc::now()),
            final_time: Some(Duration::from_secs(567)),
        };

        let history = vec![
            HistoricalSplit {
                run_id: Uuid::new_v4(),
                duration: Duration::from_secs(678),
            },
            HistoricalSplit {
                run_id,
                duration: Duration::from_secs(567),
            },
        ];

        let splits = Splits::create_with_history(
            std::path::PathBuf::from("/tmp/fake.json"),
            Some(run_summary.clone()),
            vec![run_summary.clone()],
            vec![Split {
                name: "Test Split".to_string(),
                percent: 50,
                time: Some(Duration::from_secs(567)),
                history,
            }],
        );

        // Round-trip
        let file_v2: SplitsFileV2 = (&splits).into();
        let restored = from_v2(file_v2, std::path::Path::new("/tmp/fake.json"));

        // Check equality
        assert_eq!(restored.personal_best(), splits.personal_best());
        assert_eq!(restored.runs(), splits.runs());
        assert_eq!(restored.splits(), splits.splits());
    }

    #[test]
    fn test_migrate_v1_to_v2_basic() {
        // Setup a minimal V1 splits file
        let v1 = SplitsFileV1 {
            version: 1,
            splits: SplitsV1 {
                splits: vec![
                    SplitV1 {
                        name: "Split 1".to_string(),
                        percent: 50,
                        duration: Some(HmsDuration(Duration::from_secs(30))),
                    },
                    SplitV1 {
                        name: "Split 2".to_string(),
                        percent: 100,
                        duration: Some(HmsDuration(Duration::from_secs(60))),
                    },
                ],
            },
        };

        // Perform migration
        let v2: SplitsFileV2 = v1.clone().into();

        // Assertions
        assert_eq!(v2.version, SPLITS_FILE_VERSION_V2, "v2 version should be 2");
        assert!(
            v2.splits.personal_best.is_none(),
            "personal_best should be None"
        );
        assert!(v2.splits.runs.is_empty(), "runs should be empty");
        assert_eq!(
            v2.splits.splits.len(),
            v1.splits.splits.len(),
            "v1 and v2 splits should have equal length"
        );

        for (split_v2, split_v1) in v2.splits.splits.iter().zip(v1.splits.splits) {
            assert_eq!(split_v2.name, split_v1.name);
            assert_eq!(split_v2.percent, split_v1.percent);
            assert_eq!(split_v2.time, split_v1.duration);
            assert!(split_v2.history.is_empty(), "history should be empty");
        }
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
    fn deserialize_missing_or_null_duration_succeeds() {
        let input = r#"{
            "version": 1,
            "splits": {
                "splits": [
                    { "name": "NoDuration", "percent": 10 },
                    { "name": "NullDuration", "percent": 20, "duration": null }
                ]
            }
        }"#;

        let parsed: Result<SplitsFileV1, _> = serde_json::from_str(input);
        assert!(
            parsed.is_ok(),
            "Input with missing/null duration should succeed, but failed"
        );
        let parsed = parsed.unwrap();
        let splits = parsed.splits.splits;

        assert_eq!(splits.len(), 2);
        assert_eq!(splits[0].name, "NoDuration");
        assert_eq!(splits[0].percent, 10);
        assert_eq!(splits[0].duration, None);
        assert_eq!(splits[1].name, "NullDuration");
        assert_eq!(splits[1].percent, 20);
        assert_eq!(splits[1].duration, None);
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
    fn load_from_file_with_valid_v2_file() -> Result<()> {
        use std::fs::write;
        use tempfile::tempdir;

        let dir = tempdir()?;
        let file_path = dir.path().join("splits.json");

        let json = format!(
            r#"{{
        "version": 2,
        "splits": {{
            "personal_best": {{
                "id": "{pb_id}",
                "start_time": "2025-08-15T12:00:00Z",
                "end_time": "2025-08-15T12:30:00Z",
                "final_time": "0:30:00"
            }},
            "runs": [
                {{
                    "id": "{run_id}",
                    "start_time": "2025-08-14T15:00:00Z",
                    "end_time": "2025-08-14T15:35:00Z",
                    "final_time": "0:35:00"
                }}
            ],
            "splits": [
                {{
                    "name": "Level 1",
                    "percent": 10,
                    "time": "0:10:00",
                    "history": [
                        {{
                            "run_id": "{run_id}",
                            "duration": "0:11:00"
                        }}
                    ]
                }},
                {{
                    "name": "Boss Fight",
                    "percent": 50,
                    "time": "1:00:00",
                    "history": []
                }}
            ]
        }}
    }}"#,
            pb_id = Uuid::new_v4(),
            run_id = Uuid::new_v4()
        );

        write(&file_path, json)?;

        let splits = load_from_file(&file_path)?;

        assert_eq!(splits.splits().len(), 2);

        let split1 = &splits.splits()[0];
        assert_eq!(split1.name, "Level 1");
        assert_eq!(split1.percent, 10);
        assert_eq!(split1.time, Some(Duration::from_secs(10 * 60)));
        assert_eq!(split1.history.len(), 1);
        assert_eq!(split1.history[0].duration, Duration::from_secs(11 * 60));

        let split2 = &splits.splits()[1];
        assert_eq!(split2.name, "Boss Fight");
        assert_eq!(split2.percent, 50);
        assert_eq!(split2.time, Some(Duration::from_secs(3600)));
        assert!(split2.history.is_empty());

        let pb = splits.personal_best().unwrap();
        assert_eq!(pb.final_time, Some(Duration::from_secs(30 * 60)));

        let runs = splits.runs();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].final_time, Some(Duration::from_secs(35 * 60)));
        assert_eq!(runs[0].id, split1.history[0].run_id);

        assert_eq!(splits.path(), Some(&file_path));
        assert_eq!(splits.active_run(), None);

        Ok(())
    }

    #[test]
    fn load_from_file_with_valid_v1_file_migrates_to_v2() -> Result<()> {
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

        assert_eq!(splits.splits().len(), 2);
        assert_eq!(splits.splits()[0].name, "Level 1");
        assert_eq!(splits.splits()[0].percent, 10);
        assert_eq!(splits.splits()[0].time, Some(Duration::from_secs(10 * 60)));
        assert!(splits.splits()[0].history.is_empty());
        assert_eq!(splits.splits()[1].name, "Boss Fight");
        assert_eq!(splits.splits()[1].percent, 50);
        assert_eq!(splits.splits()[1].time, Some(Duration::from_secs(1 * 3600)));
        assert!(splits.splits()[1].history.is_empty());
        assert_eq!(splits.path(), Some(&file_path));
        assert_eq!(splits.active_run(), None);
        assert_eq!(splits.personal_best(), None);
        assert!(splits.runs().is_empty());

        Ok(())
    }

    #[test]
    fn save_to_file_writes_valid_v2_splits() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("splits.json");

        let splits = Splits::create(
            file_path.clone(),
            vec![
                Split {
                    name: "Start".to_string(),
                    percent: 25,
                    time: Some(Duration::from_secs(5)),
                    history: Vec::new(),
                },
                Split {
                    name: "End".to_string(),
                    percent: 100,
                    time: Some(Duration::from_secs(5 * 60)),
                    history: Vec::new(),
                },
            ],
        );

        splits.save_to_file()?;

        // Check that file exists and contains expected JSON
        let contents = fs::read_to_string(&file_path)?;
        assert!(contents.contains("\"version\": 2"));
        assert!(contents.contains("\"Start\""));
        assert!(contents.contains("\"End\""));

        Ok(())
    }

    #[test]
    fn save_then_load_round_trip() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("roundtrip_splits.json");

        let run_id = uuid::Uuid::new_v4();
        let pb = RunSummary {
            id: run_id,
            start_time: chrono::Utc::now(),
            end_time: Some(chrono::Utc::now() + chrono::Duration::seconds(2400)),
            final_time: Some(Duration::from_secs(1750)),
        };
        let original_splits = Splits::create_with_history(
            file_path.clone(),
            Some(pb.clone()),
            vec![pb.clone()],
            vec![
                Split {
                    name: "Split 1".to_string(),
                    percent: 25,
                    time: Some(Duration::from_secs(600)),
                    history: vec![HistoricalSplit {
                        run_id,
                        duration: Duration::from_secs(590),
                    }],
                },
                Split {
                    name: "Split 2".to_string(),
                    percent: 75,
                    time: Some(Duration::from_secs(1800)),
                    history: vec![HistoricalSplit {
                        run_id,
                        duration: Duration::from_secs(1750),
                    }],
                },
            ],
        );

        // Save to file
        original_splits.save_to_file()?;

        // Load from file
        let loaded_splits = load_from_file(&file_path)?;

        // Assert splits match
        assert_eq!(loaded_splits.splits().len(), original_splits.splits().len());
        for (orig, loaded) in original_splits
            .splits()
            .iter()
            .zip(loaded_splits.splits().iter())
        {
            assert_eq!(orig.name, loaded.name);
            assert_eq!(orig.percent, loaded.percent);
            assert_eq!(orig.time, loaded.time);
            assert_eq!(orig.history.len(), loaded.history.len());
        }

        assert_eq!(loaded_splits.runs().len(), original_splits.runs().len());
        assert_eq!(
            loaded_splits.personal_best().unwrap().id,
            original_splits.personal_best().unwrap().id
        );

        Ok(())
    }
}
