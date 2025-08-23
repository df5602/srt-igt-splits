use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::bail;
use chrono::{DateTime, Utc};
use colored::Colorize;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use crate::in_game_time::InGameTime;

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveRun {
    pub id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub latest_split: InGameTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub final_time: Option<Duration>, // if finished
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalSplit {
    pub run_id: Uuid,
    pub duration: Duration,
}

#[derive(Debug, PartialEq)]
pub struct Split {
    pub name: String,
    pub percent: u32,
    pub time: Option<Duration>,
    pub history: Vec<HistoricalSplit>,
}

#[derive(Debug, PartialEq)]
pub struct Splits {
    path: Option<PathBuf>,
    active_run: Option<ActiveRun>,
    personal_best: Option<RunSummary>,
    runs: Vec<RunSummary>,
    splits: Vec<Split>,
}

impl Splits {
    /// Constructs empty `Splits`.
    pub fn new() -> Self {
        Splits {
            path: None,
            active_run: None,
            personal_best: None,
            runs: Vec::new(),
            splits: Vec::new(),
        }
    }

    /// Currently only used for tests and deserialization
    pub fn create(path: PathBuf, splits: Vec<Split>) -> anyhow::Result<Self> {
        let mut splits = Splits {
            path: Some(path),
            active_run: None,
            personal_best: None,
            runs: Vec::new(),
            splits,
        };
        splits.validate()?;
        Ok(splits)
    }

    /// Currently only used for tests and deserialization
    pub fn create_with_history(
        path: PathBuf,
        personal_best: Option<RunSummary>,
        runs: Vec<RunSummary>,
        splits: Vec<Split>,
    ) -> anyhow::Result<Self> {
        let mut splits = Splits {
            path: Some(path),
            active_run: None,
            personal_best,
            runs,
            splits,
        };
        splits.validate()?;
        Ok(splits)
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn active_run(&self) -> Option<&ActiveRun> {
        self.active_run.as_ref()
    }

    // This is a hack, might be one more argument for proper LiveSplit integration
    pub fn initialize_active_run(&mut self, time: &InGameTime) {
        if self.active_run.is_none() {
            self.active_run = Some(ActiveRun {
                id: Uuid::new_v4(),
                start_time: Utc::now(),
                end_time: Some(Utc::now()),
                latest_split: *time,
            });
        }
    }

    pub fn personal_best(&self) -> Option<&RunSummary> {
        self.personal_best.as_ref()
    }

    pub fn runs(&self) -> &Vec<RunSummary> {
        &self.runs
    }

    pub fn splits(&self) -> &Vec<Split> {
        &self.splits
    }

    /// Loads splits from a file
    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let mut splits = crate::splits::file_persistency::load_from_file(path)?;
        splits.validate()?;
        Ok(splits)
    }

    /// Save splits to file
    pub fn save_to_file(&self) -> anyhow::Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No file path to save to"))?;

        crate::splits::file_persistency::save_to_file(self, path)
    }

    fn validate(&mut self) -> anyhow::Result<()> {
        // Splits must be sorted by percent
        self.splits.sort_by(|a, b| a.percent.cmp(&b.percent));

        // Splits can't contain duplicate entries
        // We just sorted above, so we can rely on that.
        if self.splits.windows(2).any(|s| s[0].percent == s[1].percent) {
            bail!("Splits contain duplicate entries (percentages)");
        }

        // Runs should be sorted according to start time
        self.runs.sort_by(|a, b| a.start_time.cmp(&b.start_time));

        // Runs can't have duplicate IDs (also build a map from run ID -> sort order for later)
        let mut run_indices: HashMap<Uuid, usize> = HashMap::new();
        for (idx, run) in self.runs().iter().enumerate() {
            if !run_indices.insert(run.id, idx).is_none() {
                bail!("Runs have duplicate IDs");
            }
        }

        // If `personal_best` is set, it should reference a run in `runs`.
        // Add if missing
        if let Some(pb) = self.personal_best() {
            if self.runs().iter().find(|&run| run.id == pb.id).is_none() {
                self.runs.push(pb.clone());
            }
        }

        // Splits history should be sorted according to run order
        // Splits history should not contain duplicates, take "last one" (largest duration)
        for split in &mut self.splits {
            split.history.sort_by(|a, b| {
                let idxa = run_indices.get(&a.run_id);
                let idxb = run_indices.get(&b.run_id);
                match idxa.cmp(&idxb) {
                    ord @ std::cmp::Ordering::Less | ord @ std::cmp::Ordering::Greater => ord,
                    std::cmp::Ordering::Equal => {
                        // Reverse comparison so that larger duration comes first (and is kept during the dedup() step)
                        b.duration.cmp(&a.duration)
                    }
                }
            });
            split.history.dedup_by_key(|hs| hs.run_id);
        }

        // Final split should match final time in run summary (if present)
        if let Some(final_split) = self.splits.last_mut() {
            for run in &self.runs {
                if let Some(final_time) = run.final_time {
                    match final_split
                        .history
                        .iter_mut()
                        .find(|hs| hs.run_id == run.id)
                    {
                        Some(hs) => hs.duration = final_time,
                        None => final_split.history.push(HistoricalSplit {
                            run_id: run.id,
                            duration: final_time,
                        }),
                    }
                }
            }

            // re-sort
            final_split.history.sort_by(|a, b| {
                let idxa = run_indices.get(&a.run_id);
                let idxb = run_indices.get(&b.run_id);
                idxa.cmp(&idxb)
            });
        }

        // PB times should match personal best
        for split in &mut self.splits {
            match &self.personal_best {
                Some(pb) => {
                    split.time = split
                        .history
                        .iter()
                        .find(|&hs| hs.run_id == pb.id)
                        .map(|hs| hs.duration)
                }
                None => split.time = None,
            }
        }

        Ok(())
    }

    /// Returns the split matching the given percent, if found.
    fn find_by_percent(&self, time: &InGameTime) -> Option<&Split> {
        self.splits.iter().find(|s| s.percent == time.percent)
    }

    fn find_by_percent_mut(&mut self, time: &InGameTime) -> Option<&mut Split> {
        self.splits.iter_mut().find(|s| s.percent == time.percent)
    }

    fn is_final_split(&self, time: &InGameTime) -> bool {
        self.splits
            .last()
            .map_or(false, |s| s.percent == time.percent)
    }

    fn compare(&self, current: &InGameTime) -> Option<(i64, &Split)> {
        if let Some(split) = self.find_by_percent(current) {
            match split.time {
                Some(duration) => {
                    let delta = current.duration.as_secs() as i64 - duration.as_secs() as i64;
                    Some((delta, split))
                }
                None => None,
            }
        } else {
            None
        }
    }

    fn start_new_run_at(&mut self, current: &InGameTime, now: DateTime<Utc>) -> Uuid {
        let run_id = Uuid::new_v4();
        self.active_run = Some(ActiveRun {
            id: run_id,
            start_time: now,
            end_time: None,
            latest_split: *current,
        });
        run_id
    }

    fn finalize_run_at(&mut self, run_id: Uuid, current: &InGameTime, now: DateTime<Utc>) {
        if let Some(active_run) = &mut self.active_run {
            active_run.end_time = Some(now);
        }

        let is_pb = current.duration
            < self
                .personal_best
                .as_ref()
                .and_then(|pb| pb.final_time)
                .unwrap_or(Duration::MAX);

        if let Some(run) = self.runs.iter_mut().find(|run| run.id == run_id) {
            run.end_time = Some(now);
            run.final_time = Some(current.duration);

            if is_pb {
                self.personal_best = Some(run.clone());
            }
        }

        if is_pb {
            for split in &mut self.splits {
                let pb = split.history.last();
                if let Some(pb) = pb
                    && pb.run_id == run_id
                {
                    split.time = Some(pb.duration);
                } else {
                    split.time = None;
                }
            }
        }
    }

    fn record_split_time(&mut self, run_id: Uuid, current: &InGameTime) {
        if let Some(current_split) = self.find_by_percent_mut(current) {
            let existing = current_split
                .history
                .last_mut()
                .filter(|hs| hs.run_id == run_id);

            match existing {
                Some(entry) => entry.duration = current.duration,
                None => current_split.history.push(HistoricalSplit {
                    run_id,
                    duration: current.duration,
                }),
            }
        }
    }

    pub fn update_with_igt(&mut self, current: &InGameTime) {
        let now = Utc::now();

        // Check if current percent corresponds to a known split
        if self.find_by_percent(current).is_none() {
            // Unknown percent -> no-op
            return;
        }

        let run_id: Option<Uuid> = match &mut self.active_run {
            Some(active_run) => {
                if current.percent < active_run.latest_split.percent {
                    // IGT has regressed, treat it as reset
                    None
                } else if active_run.end_time.is_some() {
                    // If the active run is already finished, ignore updates
                    return;
                } else {
                    active_run.latest_split = *current;
                    Some(active_run.id)
                }
            }
            None => None,
        };

        let run_id = match run_id {
            Some(run_id) => run_id,
            None => {
                let run_id = self.start_new_run_at(current, now);

                self.runs.push(RunSummary {
                    id: run_id,
                    start_time: now,
                    end_time: None,
                    final_time: None,
                });
                run_id
            }
        };

        self.record_split_time(run_id, current);

        if self.is_final_split(current) {
            self.finalize_run_at(run_id, current, now);
        }

        self.save_to_file();
    }

    pub fn compare_and_print(&self, current: &InGameTime) {
        // TODO: handle `None` case (print something like '-', check what LiveSplit does)
        if let Some((delta, split)) = self.compare(current) {
            let name_width = self.compute_name_width();
            let display_name = Self::truncate_name(&split.name, name_width);
            let colored_delta = if delta >= 0 {
                let delta_str = format!("+{:02}:{:02}", delta / 60, delta % 60);
                delta_str.red()
            } else {
                let delta_str = format!("-{:02}:{:02}", delta.abs() / 60, delta.abs() % 60);
                delta_str.green()
            };

            let current_str = Self::format_time(Some(current.duration));
            println!(
                "{} {:>8} {:>8}",
                Self::pad_str(&display_name, name_width),
                colored_delta,
                current_str
            );
        }
    }

    /// Prints all the splits in order, without time comparison.
    pub fn print_splits(&self) {
        let name_width = self.compute_name_width();

        for split in &self.splits {
            Self::print_split(name_width, split);
        }
    }

    pub fn print_split(name_width: usize, split: &Split) {
        let display_name = Self::truncate_name(&split.name, name_width);
        let duration_str = Self::format_time(split.time);
        println!(
            "{} {:>8} {:>8}",
            Self::pad_str(&display_name, name_width),
            " ",
            duration_str
        );
    }

    pub fn compute_name_width(&self) -> usize {
        const MAX_NAME_WIDTH: usize = 25;

        self.splits
            .iter()
            .map(|s| s.name.len())
            .max()
            .map(|len| len.min(MAX_NAME_WIDTH))
            .unwrap_or(0)
    }

    /// Truncates a string to a max display width. If truncation is needed, adds "..".
    pub fn truncate_name(name: &str, max_display_width: usize) -> String {
        if UnicodeWidthStr::width(name) <= max_display_width {
            return name.to_string();
        }

        let mut result = String::new();
        let mut width = 0;

        for g in UnicodeSegmentation::graphemes(name, true) {
            let g_width = UnicodeWidthStr::width(g);
            if width + g_width > max_display_width - 2 {
                break;
            }
            result.push_str(g);
            width += g_width;
        }

        if width < UnicodeWidthStr::width(name) {
            result.push_str("..");
        }

        result
    }

    /// Pads a string on the right with spaces to match the target display width.
    pub fn pad_str(s: &str, target_width: usize) -> String {
        let width = UnicodeWidthStr::width(s);
        if width >= target_width {
            s.to_string()
        } else {
            let pad = " ".repeat(target_width - width);
            format!("{}{}", s, pad)
        }
    }

    pub fn format_time(duration: Option<Duration>) -> String {
        match duration {
            Some(duration) => {
                let secs = duration.as_secs();
                let hours = secs / 3600;
                let minutes = (secs % 3600) / 60;
                let seconds = secs % 60;
                format!("{:01}:{:02}:{:02}", hours, minutes, seconds)
            }
            None => {
                format!("-:--:--")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_ingame_time(percent: u32, hours: u64, minutes: u64, secs: u64) -> InGameTime {
        InGameTime {
            percent,
            duration: Duration::from_secs(hours * 3600 + minutes * 60 + secs),
        }
    }

    #[test]
    fn new_splits_is_empty() {
        let splits = Splits::new();
        assert!(splits.splits.is_empty());
    }

    #[test]
    fn validate_splits_are_sorted_by_percentage() {
        let split1 = Split {
            name: "C".to_string(),
            percent: 75,
            time: None,
            history: vec![],
        };
        let split2 = Split {
            name: "A".to_string(),
            percent: 25,
            time: None,
            history: vec![],
        };
        let split3 = Split {
            name: "B".to_string(),
            percent: 50,
            time: None,
            history: vec![],
        };

        let splits = Splits::create(PathBuf::from("dummy_path"), vec![split1, split2, split3])
            .expect("splits should be valid");

        let percents: Vec<u32> = splits.splits.iter().map(|s| s.percent).collect();

        assert_eq!(percents, vec![25, 50, 75]);
    }

    #[test]
    fn validate_fails_on_duplicate_percents() {
        let path = PathBuf::from("dummy_path");

        // Two splits with the same percent (50)
        let splits = vec![
            Split {
                name: "First".to_string(),
                percent: 50,
                time: None,
                history: Vec::new(),
            },
            Split {
                name: "Second".to_string(),
                percent: 50,
                time: None,
                history: Vec::new(),
            },
        ];

        let result = Splits::create(path, splits);
        assert!(
            result.is_err(),
            "Expected create() to fail due to duplicate percents"
        );
    }

    #[test]
    fn validate_fails_on_duplicate_run_ids() {
        use uuid::Uuid;

        let duplicate_id = Uuid::new_v4();

        let runs = vec![
            RunSummary {
                id: duplicate_id,
                start_time: Utc::now() - Duration::from_secs(60 * 60),
                end_time: None,
                final_time: None,
            },
            RunSummary {
                id: duplicate_id, // duplicate
                start_time: Utc::now(),
                end_time: None,
                final_time: None,
            },
        ];

        let result =
            Splits::create_with_history(PathBuf::from("dummy_path"), None, runs, Vec::new());
        assert!(
            result.is_err(),
            "Expected create() to fail due to duplicate run IDs"
        );
    }

    #[test]
    fn validate_adds_missing_personal_best_run() {
        let pb_run = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(60 * 60),
            end_time: Some(Utc::now()),
            final_time: Some(Duration::from_secs(45 * 60 + 15)),
        };

        // `runs` does not contain the PB run yet
        let runs = vec![];

        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            Some(pb_run.clone()), // PB is set
            runs,
            Vec::new(),
        )
        .expect("create() should succeed by fixing up missing PB run");

        // The PB run must now exist in runs
        let found = splits
            .runs()
            .iter()
            .find(|r| r.id == pb_run.id)
            .expect("Expected PB run to be added");

        // It should be exactly the same as the PB run
        assert_eq!(
            found, &pb_run,
            "PB run in runs must match personal_best exactly"
        );
    }

    #[test]
    fn validate_sorts_history_entries_by_run_order() {
        use uuid::Uuid;

        let run_a = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(300),
            end_time: Some(Utc::now() - Duration::from_secs(200)),
            final_time: Some(Duration::from_secs(105)),
        };
        let run_b = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(200),
            end_time: Some(Utc::now() - Duration::from_secs(100)),
            final_time: Some(Duration::from_secs(100)),
        };
        let run_c = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(100),
            end_time: Some(Utc::now()),
            final_time: Some(Duration::from_secs(95)),
        };

        let runs = vec![run_a.clone(), run_b.clone(), run_c.clone()];

        // History deliberately out of order
        let split = Split {
            name: "Split".to_string(),
            percent: 50,
            time: None,
            history: vec![
                HistoricalSplit {
                    run_id: run_c.id,
                    duration: Duration::from_secs(95),
                },
                HistoricalSplit {
                    run_id: run_a.id,
                    duration: Duration::from_secs(105),
                },
                HistoricalSplit {
                    run_id: run_b.id,
                    duration: Duration::from_secs(100),
                },
            ],
        };

        let splits =
            Splits::create_with_history(PathBuf::from("dummy_path"), None, runs, vec![split])
                .expect("create() should succeed");

        let sorted_history_ids: Vec<Uuid> =
            splits.splits[0].history.iter().map(|h| h.run_id).collect();

        assert_eq!(
            sorted_history_ids,
            vec![run_a.id, run_b.id, run_c.id],
            "History entries must be sorted according to run order"
        );
    }

    #[test]
    fn validate_deduplicates_history_last_one_wins() {
        let run = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(100),
            end_time: Some(Utc::now()),
            final_time: Some(Duration::from_secs(100)),
        };

        // Split with multiple entries for the same run
        let splits = vec![
            Split {
                name: "First Split".to_string(),
                percent: 50,
                time: None,
                history: vec![
                    HistoricalSplit {
                        run_id: run.id,
                        duration: Duration::from_secs(10),
                    },
                    HistoricalSplit {
                        run_id: run.id,
                        duration: Duration::from_secs(20),
                    },
                    HistoricalSplit {
                        run_id: run.id,
                        duration: Duration::from_secs(15),
                    },
                ],
            },
            Split {
                name: "Final Split".to_string(),
                percent: 100,
                time: None,
                history: vec![],
            },
        ];

        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            None,
            vec![run.clone()],
            splits,
        )
        .expect("create_with_history() should succeed");

        // After deduplication, only the "last one" (largest duration) should remain
        let remaining = &splits.splits[0].history;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].run_id, run.id);
        assert_eq!(remaining[0].duration, Duration::from_secs(20));
    }

    #[test]
    fn validate_final_split_only_updates_last_split() {
        let run = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(120),
            end_time: Some(Utc::now()),
            final_time: Some(Duration::from_secs(120)),
        };

        // Early split that should remain untouched
        let early_split = Split {
            name: "Early".to_string(),
            percent: 50,
            time: None,
            history: vec![HistoricalSplit {
                run_id: run.id,
                duration: Duration::from_secs(60),
            }],
        };

        // Final split with a wrong duration
        let final_split = Split {
            name: "Final".to_string(),
            percent: 100,
            time: None,
            history: vec![HistoricalSplit {
                run_id: run.id,
                duration: Duration::from_secs(90),
            }],
        };

        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            None,
            vec![run.clone()],
            vec![early_split, final_split],
        )
        .expect("create_with_history() should succeed");

        // Check final split is fixed
        let last = &splits.splits[1].history.last().unwrap();
        assert_eq!(
            last.duration,
            run.final_time.unwrap(),
            "Final split duration must match run final_time"
        );

        // Check early split is untouched
        let early = &splits.splits[0].history.last().unwrap();
        assert_eq!(
            early.duration,
            Duration::from_secs(60),
            "Early split duration should remain unchanged"
        );
    }

    #[test]
    fn validate_sorts_unknown_run_ids_to_start() {
        let known_run = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now(),
            end_time: Some(Utc::now() + Duration::from_secs(5)),
            final_time: Some(Duration::from_secs(5)),
        };

        // HistoricalSplit with an unknown run_id
        let unknown_run_id = Uuid::new_v4();

        let split = Split {
            name: "Split".to_string(),
            percent: 50,
            time: None,
            history: vec![
                HistoricalSplit {
                    run_id: known_run.id,
                    duration: Duration::from_secs(5),
                },
                HistoricalSplit {
                    run_id: unknown_run_id,
                    duration: Duration::from_secs(99),
                },
            ],
        };

        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            None,
            vec![known_run.clone()],
            vec![split],
        )
        .expect("create_with_history() should succeed");

        // After validate, unknown IDs should come first
        let history = &splits.splits[0].history;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].run_id, unknown_run_id);
        assert_eq!(history[1].run_id, known_run.id);
    }

    #[test]
    fn validate_final_split_inserts_missing_historical_split() {
        let run = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(60),
            end_time: Some(Utc::now()),
            final_time: Some(Duration::from_secs(45)),
        };

        // Final split is missing history
        let final_split = Split {
            name: "Final".to_string(),
            percent: 100,
            time: None,
            history: vec![],
        };

        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            None,
            vec![run.clone()],
            vec![final_split],
        )
        .expect("create_with_history() should succeed");

        let history = &splits.splits[0].history;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].run_id, run.id);
        assert_eq!(history[0].duration, run.final_time.unwrap());
    }

    #[test]
    fn validate_sorts_runs_by_start_time() {
        let run1 = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(60),
            end_time: None,
            final_time: None,
        };
        let run2 = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now(),
            end_time: None,
            final_time: None,
        };

        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            None,
            vec![run2.clone(), run1.clone()],
            Vec::new(),
        )
        .expect("create_with_history() should succeed");

        // Runs should be sorted by start_time ascending
        let sorted_runs: Vec<_> = splits.runs().iter().map(|r| r.id).collect();
        assert_eq!(sorted_runs, vec![run1.id, run2.id]);
    }

    #[test]
    fn validate_sets_split_times_from_pb() {
        let pb_run = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::from_secs(60),
            end_time: Some(Utc::now()),
            final_time: Some(Duration::from_secs(60)),
        };

        let split1 = Split {
            name: "50%".into(),
            percent: 50,
            time: None,
            history: vec![HistoricalSplit {
                run_id: pb_run.id,
                duration: Duration::from_secs(30),
            }],
        };
        let split2 = Split {
            name: "100%".into(),
            percent: 100,
            time: None,
            history: vec![HistoricalSplit {
                run_id: pb_run.id,
                duration: Duration::from_secs(60),
            }],
        };

        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            Some(pb_run),
            vec![],
            vec![split1, split2],
        )
        .expect("create_with_history() should succeed");

        // After validate, each split.time should match PB’s duration
        assert_eq!(splits.splits()[0].time, Some(Duration::from_secs(30)));
        assert_eq!(splits.splits()[1].time, Some(Duration::from_secs(60)));
    }

    #[test]
    fn validate_sets_split_time_to_none_if_no_personal_best() {
        let run = RunSummary {
            id: Uuid::new_v4(),
            start_time: Utc::now(),
            end_time: None,
            final_time: None,
        };

        let split = Split {
            name: "Split 1".to_string(),
            percent: 50,
            time: Some(Duration::from_secs(30)),
            history: vec![HistoricalSplit {
                run_id: run.id,
                duration: Duration::from_secs(30),
            }],
        };

        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            None, // no personal best
            vec![run],
            vec![split],
        )
        .expect("create_with_history() should succeed");

        // Assert that split.time is None because there is no PB
        assert_eq!(splits.splits()[0].time, None);
    }

    #[test]
    fn find_by_percent_finds_correct_split() {
        let split1 = Split {
            name: "Alpha".to_string(),
            percent: 10,
            time: Some(Duration::from_secs(60)),
            history: vec![],
        };
        let split2 = Split {
            name: "Beta".to_string(),
            percent: 20,
            time: Some(Duration::from_secs(2 * 60)),
            history: vec![],
        };
        let splits = Splits::create(PathBuf::from("dummy_path"), vec![split1, split2])
            .expect("splits should be valid");

        let result = splits.find_by_percent(&make_ingame_time(20, 0, 0, 0));
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Beta");
    }

    #[test]
    fn find_by_percent_returns_none_for_unknown_percent() {
        let split1 = Split {
            name: "One".to_string(),
            percent: 30,
            time: Some(Duration::from_secs(3 * 60)),
            history: vec![],
        };
        let splits = Splits::create(PathBuf::from("dummy_path"), vec![split1])
            .expect("splits should be valid");

        let result = splits.find_by_percent(&make_ingame_time(40, 0, 0, 0));
        assert!(result.is_none());
    }

    #[test]
    fn compare_returns_correct_positive_delta() {
        let id = Uuid::new_v4();
        let split1 = Split {
            name: "One".to_string(),
            percent: 50,
            time: Some(Duration::from_secs(8 * 60 + 30)),
            history: vec![HistoricalSplit {
                run_id: id,
                duration: Duration::from_secs(8 * 60 + 30),
            }],
        };
        let split2 = Split {
            name: "Two".to_string(),
            percent: 60,
            time: Some(Duration::from_secs(10 * 60)),
            history: vec![HistoricalSplit {
                run_id: id,
                duration: Duration::from_secs(10 * 60),
            }],
        };
        let personal_best = RunSummary {
            id,
            start_time: Utc::now(),
            end_time: None,
            final_time: None,
        };
        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            Some(personal_best),
            vec![],
            vec![split1, split2],
        )
        .expect("splits should be valid");

        let current = make_ingame_time(60, 0, 11, 0); // +60s
        let result = splits.compare(&current);

        assert!(result.is_some());
        let (delta, split) = result.unwrap();
        assert_eq!(delta, 60);
        assert_eq!(split.name, "Two");
    }

    #[test]
    fn compare_returns_correct_negative_delta() {
        let id = Uuid::new_v4();
        let split1 = Split {
            name: "One".to_string(),
            percent: 70,
            time: Some(Duration::from_secs(15 * 60)),
            history: vec![HistoricalSplit {
                run_id: id,
                duration: Duration::from_secs(15 * 60),
            }],
        };
        let personal_best = RunSummary {
            id,
            start_time: Utc::now(),
            end_time: None,
            final_time: None,
        };
        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            Some(personal_best),
            vec![],
            vec![split1],
        )
        .expect("splits should be valid");

        let current = make_ingame_time(70, 0, 14, 30); // -30s
        let result = splits.compare(&current);

        assert!(result.is_some());
        let (delta, split) = result.unwrap();
        assert_eq!(delta, -30);
        assert_eq!(split.name, "One");
    }

    #[test]
    fn compare_returns_zero_delta() {
        let id = Uuid::new_v4();
        let time = make_ingame_time(40, 0, 5, 30);
        let split1 = Split {
            name: "One".to_string(),
            percent: time.percent,
            time: Some(time.duration),
            history: vec![HistoricalSplit {
                run_id: id,
                duration: time.duration,
            }],
        };
        let personal_best = RunSummary {
            id,
            start_time: Utc::now(),
            end_time: None,
            final_time: None,
        };
        let splits = Splits::create_with_history(
            PathBuf::from("dummy_path"),
            Some(personal_best),
            vec![],
            vec![split1],
        )
        .expect("splits should be valid");

        let result = splits.compare(&time);
        assert!(result.is_some());

        let (delta, split) = result.unwrap();
        assert_eq!(delta, 0);
        assert_eq!(split.name, "One");
    }

    #[test]
    fn compare_returns_none_for_nonexistent_split() {
        let split1 = Split {
            name: "Unrelated".to_string(),
            percent: 10,
            time: Some(Duration::from_secs(1 * 60)),
            history: vec![],
        };
        let splits = Splits::create(PathBuf::from("dummy_path"), vec![split1])
            .expect("splits should be valid");

        let result = splits.compare(&make_ingame_time(99, 0, 2, 0));
        assert!(result.is_none());
    }

    #[test]
    fn start_new_run_creates_active_run() {
        let split = Split {
            name: "First Split".into(),
            percent: 10,
            time: Some(Duration::from_secs(20)),
            history: vec![],
        };

        let mut splits = Splits::create(PathBuf::from("fake/path"), vec![split])
            .expect("splits should be valid");

        let igt = InGameTime {
            percent: 10,
            duration: Duration::from_secs(30),
        };

        // Given: no active run
        assert!(splits.active_run().is_none());

        // When: update with new IGT
        let now = Utc::now();
        splits.update_with_igt(&igt);

        // Then: active run was created
        let active_run = splits
            .active_run()
            .expect("Expected active run to be created");
        assert_eq!(active_run.latest_split, igt);
        assert!(active_run.start_time >= now - Duration::from_secs(1));
        assert!(active_run.start_time <= now + Duration::from_secs(1));
    }

    #[test]
    fn start_new_run_appends_to_existing_history() {
        let existing_run_id = Uuid::new_v4();
        let existing_entry = HistoricalSplit {
            run_id: existing_run_id,
            duration: Duration::from_secs(25),
        };

        let split = Split {
            name: "First Split".into(),
            percent: 10,
            time: Some(Duration::from_secs(20)),
            history: vec![existing_entry.clone()],
        };

        let mut splits = Splits::create(PathBuf::from("fake/path"), vec![split])
            .expect("splits should be valid");

        let igt = InGameTime {
            percent: 10,
            duration: Duration::from_secs(30),
        };

        splits.update_with_igt(&igt);

        let current_split = splits.splits().first().unwrap();
        assert_eq!(current_split.history.len(), 2);
        assert_eq!(current_split.history[0], existing_entry);
        assert_eq!(current_split.history[1].duration, igt.duration);
    }

    #[test]
    fn start_new_run_pushes_run_summary() {
        let split_1 = Split {
            name: "First Split".into(),
            percent: 10,
            time: Some(Duration::from_secs(20)),
            history: vec![],
        };
        let split_2 = Split {
            name: "Second Split".into(),
            percent: 20,
            time: Some(Duration::from_secs(40)),
            history: vec![],
        };

        let mut splits = Splits::create(PathBuf::from("fake/path"), vec![split_1, split_2])
            .expect("splits should be valid");

        let igt = InGameTime {
            percent: 10,
            duration: Duration::from_secs(30),
        };

        // When: update with IGT
        let now = Utc::now();
        splits.update_with_igt(&igt);

        // Then: a new run is added to the runs list
        let run_summary = splits
            .runs()
            .last()
            .expect("Expected a RunSummary to be added");

        let active_run = splits.active_run().expect("Expected active run");

        assert_eq!(run_summary.id, active_run.id);
        assert!(run_summary.start_time >= now - Duration::from_secs(1));
        assert!(run_summary.start_time <= now + Duration::from_secs(1));
        assert_eq!(run_summary.end_time, None);
        assert_eq!(run_summary.final_time, None);
    }

    #[test]
    fn update_existing_split_overwrites_duration_for_same_run() {
        let run_id = Uuid::new_v4();

        let original_duration = Duration::from_secs(25);
        let updated_duration = Duration::from_secs(30);

        let split = Split {
            name: "20% Split".into(),
            percent: 20,
            time: Some(original_duration),
            history: vec![HistoricalSplit {
                run_id,
                duration: original_duration,
            }],
        };

        let mut splits = Splits::create(PathBuf::from("fake/path"), vec![split])
            .expect("splits should be valid");

        // Pre-existing active run
        splits.active_run = Some(ActiveRun {
            id: run_id,
            start_time: Utc::now(),
            end_time: None,
            latest_split: InGameTime {
                percent: 20,
                duration: original_duration,
            },
        });

        // Incoming update with same percent, updated duration
        let current = InGameTime {
            percent: 20,
            duration: updated_duration,
        };

        // Act
        splits.update_with_igt(&current);

        // Assert: history was updated
        let updated_split = &splits.splits()[0];
        let hist = &updated_split.history[0];
        assert_eq!(hist.run_id, run_id);
        assert_eq!(hist.duration, updated_duration);

        // Also: active run was not reset
        let active_run = splits.active_run().expect("active run should exist");
        assert_eq!(active_run.id, run_id);
        assert_eq!(active_run.latest_split, current);
    }

    #[test]
    fn advance_to_next_split_adds_new_history_entry() {
        let split_10 = Split {
            name: "First Split".into(),
            percent: 10,
            time: Some(Duration::from_secs(20)),
            history: vec![],
        };

        let split_20 = Split {
            name: "Second Split".into(),
            percent: 20,
            time: Some(Duration::from_secs(40)),
            history: vec![],
        };

        let mut splits = Splits::create(PathBuf::from("fake/path"), vec![split_10, split_20])
            .expect("splits should be valid");

        // Simulate first update at 10% to create the run
        let run_start_igt = InGameTime {
            percent: 10,
            duration: Duration::from_secs(25),
        };
        splits.update_with_igt(&run_start_igt);

        // Store active run ID
        let run_id = splits.active_run().expect("Expected active run").id;

        // Advance to next split (20%)
        let next_igt = InGameTime {
            percent: 20,
            duration: Duration::from_secs(55),
        };
        splits.update_with_igt(&next_igt);

        // active_run.latest_split should now be at 20%
        let active_run = splits.active_run().expect("Expected active run");
        assert_eq!(active_run.latest_split.percent, 20);
        assert_eq!(active_run.latest_split.duration, Duration::from_secs(55));

        // The 10% split history should contain only the initial entry
        let first_split = &splits.splits()[0];
        assert_eq!(first_split.history.len(), 1);
        assert_eq!(first_split.history[0].run_id, run_id);
        assert_eq!(first_split.history[0].duration, Duration::from_secs(25));

        // The 20% split history should now contain a new entry
        let second_split = &splits.splits()[1];
        assert_eq!(second_split.history.len(), 1);
        assert_eq!(second_split.history[0].run_id, run_id);
        assert_eq!(second_split.history[0].duration, Duration::from_secs(55));

        // No new run summary should have been added
        assert_eq!(splits.runs().len(), 1);
    }

    #[test]
    fn reset_triggers_new_run_on_percent_regression() {
        let split_5 = Split {
            name: "Intro".into(),
            percent: 5,
            time: Some(Duration::from_secs(10)),
            history: vec![],
        };

        let split_40 = Split {
            name: "Mid Game".into(),
            percent: 40,
            time: Some(Duration::from_secs(80)),
            history: vec![],
        };

        let mut splits = Splits::create(PathBuf::from("fake/path"), vec![split_5, split_40])
            .expect("splits should be valid");

        // Step 1: Simulate reaching 40%
        splits.update_with_igt(&InGameTime {
            percent: 5,
            duration: Duration::from_secs(8),
        });

        let first_igt = InGameTime {
            percent: 40,
            duration: Duration::from_secs(90),
        };
        splits.update_with_igt(&first_igt);

        let first_run = splits.active_run().expect("Expected active run");
        let first_run_id = first_run.id;

        // Step 2: Reset to 5% — should start a new run
        let reset_igt = InGameTime {
            percent: 5,
            duration: Duration::from_secs(8),
        };
        splits.update_with_igt(&reset_igt);

        let second_run = splits.active_run().expect("Expected new active run");
        let second_run_id = second_run.id;

        // A new run ID was generated
        assert_ne!(first_run_id, second_run_id);

        // New run has updated latest_split
        assert_eq!(second_run.latest_split, reset_igt);

        // Historical split at 40% is untouched (only first run)
        let split_40 = &splits.splits()[1];
        assert_eq!(split_40.history.len(), 1);
        assert_eq!(split_40.history[0].run_id, first_run_id);

        // Historical split at 5% contains entry for new run only
        let split_5 = &splits.splits()[0];
        assert_eq!(split_5.history.len(), 2);
        assert_eq!(split_5.history[1].run_id, second_run_id);
        assert_eq!(split_5.history[1].duration, Duration::from_secs(8));

        // Two RunSummary entries exist
        assert_eq!(splits.runs().len(), 2);
    }

    #[test]
    fn reaching_final_split_marks_run_as_finished() {
        let splits = vec![
            Split {
                name: "Split 1".into(),
                percent: 10,
                time: Some(Duration::from_secs(10)),
                history: vec![],
            },
            Split {
                name: "Split 2".into(),
                percent: 50,
                time: Some(Duration::from_secs(50)),
                history: vec![],
            },
            Split {
                name: "Final Split".into(),
                percent: 100,
                time: Some(Duration::from_secs(100)),
                history: vec![],
            },
        ];

        let mut splits =
            Splits::create(PathBuf::from("fake/path"), splits).expect("splits should be valid");

        // Start the run with the first IGT
        let igt1 = InGameTime {
            percent: 10,
            duration: Duration::from_secs(11),
        };
        splits.update_with_igt(&igt1);
        let run_id = splits.active_run().unwrap().id;

        // Progress through next split
        let igt2 = InGameTime {
            percent: 50,
            duration: Duration::from_secs(52),
        };
        splits.update_with_igt(&igt2);

        // Final split reached
        let ts_end = Utc::now();
        let igt3 = InGameTime {
            percent: 100,
            duration: Duration::from_secs(103),
        };
        splits.update_with_igt(&igt3);

        // Active run should now be completed
        let active_run = splits.active_run().unwrap();
        assert!(active_run.end_time.is_some());
        let end_time = active_run.end_time.unwrap();
        assert!(
            end_time >= ts_end - Duration::from_secs(1)
                && end_time <= ts_end + Duration::from_secs(1)
        );

        // RunSummary should also be updated
        let summary = splits.runs.iter().find(|r| r.id == run_id).unwrap();
        assert_eq!(summary.end_time, Some(end_time));
        assert_eq!(summary.final_time, Some(Duration::from_secs(103)));
    }

    #[test]
    fn final_split_reached_twice_does_not_overwrite() {
        use std::time::Duration;

        let final_split = Split {
            name: "Final Split".into(),
            percent: 100,
            time: Some(Duration::from_secs(120)),
            history: vec![],
        };

        let mut splits = Splits::create(PathBuf::from("fake/path"), vec![final_split])
            .expect("splits should be valid");

        // First update: start and finish run immediately
        let igt = InGameTime {
            percent: 100,
            duration: Duration::from_secs(120),
        };
        splits.update_with_igt(&igt);

        let run_summary = splits
            .runs
            .last()
            .expect("Expected a run summary after finishing run");

        let recorded_end_time = run_summary.end_time;
        let recorded_final_time = run_summary.final_time;

        assert!(recorded_end_time.is_some());
        assert_eq!(recorded_final_time, Some(igt.duration));

        // Second update: same final split
        let later_igt = InGameTime {
            percent: 100,
            duration: Duration::from_secs(999), // bogus new time that shouldn't overwrite
        };
        splits.update_with_igt(&later_igt);

        let updated_run_summary = splits
            .runs
            .last()
            .expect("Expected the same run summary to remain");

        assert_eq!(
            updated_run_summary.end_time, recorded_end_time,
            "end_time should not be overwritten"
        );
        assert_eq!(
            updated_run_summary.final_time, recorded_final_time,
            "final_time should not be overwritten"
        );
    }

    #[test]
    fn reset_works_even_after_final_split() {
        let splits = vec![
            Split {
                name: "First Split".into(),
                percent: 10,
                time: Some(Duration::from_secs(20)),
                history: vec![],
            },
            Split {
                name: "Final Split".into(),
                percent: 100,
                time: Some(Duration::from_secs(200)),
                history: vec![],
            },
        ];

        let mut splits =
            Splits::create(PathBuf::from("fake/path"), splits).expect("splits should be valid");

        // Start a run and finish it
        splits.update_with_igt(&InGameTime {
            percent: 10,
            duration: Duration::from_secs(30),
        });
        splits.update_with_igt(&InGameTime {
            percent: 100,
            duration: Duration::from_secs(220),
        });

        // Sanity check: run should be finished
        let previous_id = {
            let active_run = splits.active_run().expect("active run should exist");
            assert!(active_run.end_time.is_some(), "run should be finished");
            active_run.id
        };

        // Now send an earlier percent → should reset into a new run
        let earlier_igt = InGameTime {
            percent: 10,
            duration: Duration::from_secs(25),
        };
        splits.update_with_igt(&earlier_igt);

        // Verify that a new active run was started and is not the same ID
        let new_active_run = splits.active_run().expect("active run after reset");
        assert_ne!(
            new_active_run.id, previous_id,
            "new run ID should differ from old run ID"
        );
        assert_eq!(new_active_run.latest_split, earlier_igt);
    }

    #[test]
    fn update_with_unknown_percent_does_nothing() {
        let split = Split {
            name: "Known Split".into(),
            percent: 50,
            time: Some(Duration::from_secs(100)),
            history: vec![],
        };

        let mut splits = Splits::create(PathBuf::from("fake/path"), vec![split])
            .expect("splits should be valid");

        // Start a run with a known split percent to have active_run
        let known_igt = InGameTime {
            percent: 50,
            duration: Duration::from_secs(110),
        };
        splits.update_with_igt(&known_igt);
        assert!(splits.active_run().is_some());
        assert_eq!(splits.active_run().unwrap().latest_split, known_igt);

        // Now update with an unknown percent (e.g., 30)
        let unknown_igt = InGameTime {
            percent: 30,
            duration: Duration::from_secs(90),
        };
        splits.update_with_igt(&unknown_igt);

        // Expect no change: active_run.latest_split stays at known_igt
        let active_run = splits.active_run().unwrap();
        assert_eq!(active_run.latest_split, known_igt);

        // Also confirm history for known split did not get a new entry for unknown percent
        let known_split = splits.splits().first().unwrap();
        assert_eq!(known_split.history.len(), 1);
        assert_eq!(known_split.history[0].duration, known_igt.duration);
    }

    #[test]
    fn update_with_unknown_percent_does_not_create_run() {
        let split = Split {
            name: "First Split".into(),
            percent: 10,
            time: Some(Duration::from_secs(20)),
            history: vec![],
        };

        let mut splits = Splits::create(PathBuf::from("fake/path"), vec![split])
            .expect("splits should be valid");

        let unknown_igt = InGameTime {
            percent: 15, // No split at 15%
            duration: Duration::from_secs(25),
        };

        // Precondition: no active run, no runs in history
        assert!(splits.active_run().is_none());
        assert!(splits.runs.is_empty());

        // When: update with unknown percent IGT
        splits.update_with_igt(&unknown_igt);

        // Then: no active run created
        assert!(splits.active_run().is_none());

        // No runs summary added
        assert!(splits.runs.is_empty());

        // No history added to any split
        assert!(splits.splits().iter().all(|s| s.history.is_empty()));
    }

    #[test]
    fn first_run_sets_personal_best() {
        // Arrange
        let mut splits = Splits::create(
            PathBuf::from("fake/path"),
            vec![
                Split {
                    name: "Split 1".into(),
                    percent: 10,
                    time: None,
                    history: vec![],
                },
                Split {
                    name: "Split 2".into(),
                    percent: 20,
                    time: None,
                    history: vec![],
                },
            ],
        )
        .expect("splits should be valid");

        // Act – simulate a run that hits both splits in order
        splits.update_with_igt(&InGameTime {
            percent: 10,
            duration: Duration::from_secs(30),
        });
        splits.update_with_igt(&InGameTime {
            percent: 20,
            duration: Duration::from_secs(65),
        }); // last split -> run finishes

        // Assert – personal best is set and matches run times
        assert!(
            splits.personal_best.is_some(),
            "Expected personal best to be set after first run"
        );
        let pb = splits.personal_best.as_ref().unwrap();
        assert_eq!(pb.final_time, Some(Duration::from_secs(65)));

        assert_eq!(splits.splits()[0].time, Some(Duration::from_secs(30)));
        assert_eq!(splits.splits()[1].time, Some(Duration::from_secs(65)));
    }

    #[test]
    fn slower_run_does_not_update_personal_best() {
        use std::time::Duration;

        let mut splits = Splits::create(
            PathBuf::from("fake/path"),
            vec![
                Split {
                    name: "Split 1".into(),
                    percent: 10,
                    time: None,
                    history: vec![],
                },
                Split {
                    name: "Split 2".into(),
                    percent: 20,
                    time: None,
                    history: vec![],
                },
            ],
        )
        .expect("splits should be valid");

        // First run -> PB
        splits.update_with_igt(&InGameTime {
            percent: 10,
            duration: Duration::from_secs(30),
        });
        splits.update_with_igt(&InGameTime {
            percent: 20,
            duration: Duration::from_secs(65),
        });

        let first_pb = splits.personal_best.clone().unwrap();

        // Second run -> slower
        splits.update_with_igt(&InGameTime {
            percent: 10,
            duration: Duration::from_secs(35),
        });
        splits.update_with_igt(&InGameTime {
            percent: 20,
            duration: Duration::from_secs(70),
        });

        // Assert PB unchanged
        assert_eq!(
            splits.personal_best.as_ref().unwrap().id,
            first_pb.id,
            "PB should not change after slower run"
        );
        assert_eq!(
            splits.personal_best.as_ref().unwrap().final_time,
            first_pb.final_time
        );

        // Assert PB split times unchanged
        assert_eq!(splits.splits()[0].time, Some(Duration::from_secs(30)));
        assert_eq!(splits.splits()[1].time, Some(Duration::from_secs(65)));
    }

    #[test]
    fn faster_run_updates_personal_best_and_splits() {
        use std::time::Duration;

        let mut splits = Splits::create(
            PathBuf::from("fake/path"),
            vec![
                Split {
                    name: "Split 1".into(),
                    percent: 10,
                    time: None,
                    history: vec![],
                },
                Split {
                    name: "Split 2".into(),
                    percent: 20,
                    time: None,
                    history: vec![],
                },
            ],
        )
        .expect("splits should be valid");

        // First run -> slower PB
        splits.update_with_igt(&InGameTime {
            percent: 10,
            duration: Duration::from_secs(35),
        });
        splits.update_with_igt(&InGameTime {
            percent: 20,
            duration: Duration::from_secs(70),
        });

        let first_pb = splits.personal_best.clone().unwrap();

        // Second run -> faster
        splits.update_with_igt(&InGameTime {
            percent: 10,
            duration: Duration::from_secs(30),
        });
        splits.update_with_igt(&InGameTime {
            percent: 20,
            duration: Duration::from_secs(65),
        });

        // Assert PB updated
        assert_ne!(
            splits.personal_best.as_ref().unwrap().id,
            first_pb.id,
            "PB run ID should change"
        );
        assert_eq!(
            splits.personal_best.as_ref().unwrap().final_time,
            Some(Duration::from_secs(65))
        );

        // Assert PB split times updated
        assert_eq!(splits.splits()[0].time, Some(Duration::from_secs(30)));
        assert_eq!(splits.splits()[1].time, Some(Duration::from_secs(65)));
    }

    #[test]
    fn tie_run_does_not_overwrite_personal_best() {
        use std::time::Duration;

        let mut splits = Splits::create(
            PathBuf::from("fake/path"),
            vec![
                Split {
                    name: "Split 1".into(),
                    percent: 10,
                    time: None,
                    history: vec![],
                },
                Split {
                    name: "Split 2".into(),
                    percent: 20,
                    time: None,
                    history: vec![],
                },
            ],
        )
        .expect("splits should be valid");

        // First run -> set PB
        splits.update_with_igt(&InGameTime {
            percent: 10,
            duration: Duration::from_secs(30),
        });
        splits.update_with_igt(&InGameTime {
            percent: 20,
            duration: Duration::from_secs(60),
        });

        let pb_before = splits.personal_best.clone().unwrap();

        // Second run -> exact same final time as PB
        splits.update_with_igt(&InGameTime {
            percent: 10,
            duration: Duration::from_secs(30),
        });
        splits.update_with_igt(&InGameTime {
            percent: 20,
            duration: Duration::from_secs(60),
        });

        // PB should remain unchanged
        assert_eq!(
            splits.personal_best.as_ref().unwrap().id,
            pb_before.id,
            "PB should not be overwritten by a tie"
        );
        assert_eq!(
            splits.personal_best.as_ref().unwrap().final_time,
            pb_before.final_time,
            "PB time should remain unchanged"
        );
    }
}
