mod file_persistency;
mod splits;

pub use splits::Splits;

use colored::{Color, Colorize};
use std::time::Duration;
use uuid::Uuid;

use crate::in_game_time::InGameTime;

pub struct SplitsDisplay {
    last_run_id: Option<Uuid>,
    pb_snapshot: Vec<Option<Duration>>,
    best_segs_snapshot: Vec<Option<Duration>>,
}

impl SplitsDisplay {
    pub fn new() -> Self {
        Self {
            last_run_id: None,
            pb_snapshot: Vec::new(),
            best_segs_snapshot: Vec::new(),
        }
    }

    /// Render a split view of given `window_size` lines centered around the current split
    pub fn render_split_view(
        &mut self,
        splits: &Splits,
        current_igt: &InGameTime,
        window_size: usize,
    ) -> Vec<String> {
        // --- 1. Detect run start & snapshot PBs and best segments ---
        if let Some(active_run) = splits.active_run() {
            if Some(active_run.id) != self.last_run_id {
                self.last_run_id = Some(active_run.id);
                self.pb_snapshot = splits.splits().iter().map(|s| s.time).collect();
                self.best_segs_snapshot = splits.splits().iter().map(|s| s.best_segment).collect();
            }
        }

        // --- 2. Compute current split index ---
        let all_splits = splits.splits();
        if all_splits.is_empty() {
            return Vec::new();
        }

        // Find the split index corresponding to the current IGT percent
        let current_index = all_splits
            .iter()
            .position(|s| s.percent == current_igt.percent);
        let current_index = match current_index {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        // --- 3. Compute window indices ---
        let total = all_splits.len();
        let half = window_size / 2;
        let start = if current_index >= half {
            std::cmp::min(current_index - half, total.saturating_sub(window_size))
        } else {
            0
        };
        let end = std::cmp::min(start + window_size, total);

        // --- 4. Format rows ---
        let name_width = splits.compute_name_width();
        let mut lines = Vec::new();

        for idx in start..end {
            let split = &all_splits[idx];
            let pb_time = self.pb_snapshot.get(idx).copied().unwrap_or(None);

            let (time, delta) = if idx < current_index {
                // Past split
                let run_time = split
                    .history
                    .iter()
                    .find(|h| Some(h.run_id) == self.last_run_id)
                    .map(|h| h.duration);
                let delta = match (run_time, pb_time) {
                    (Some(rt), Some(pb)) => Some(rt.as_secs() as i64 - pb.as_secs() as i64),
                    _ => None,
                };
                (run_time, delta)
            } else if idx == current_index {
                // Current split
                let delta = match pb_time {
                    Some(pb) => Some(current_igt.duration.as_secs() as i64 - pb.as_secs() as i64),
                    None => None,
                };
                (Some(current_igt.duration), delta)
            } else {
                // Future split
                (pb_time, None)
            };

            // Check for golds
            let gold = match (
                self.best_segs_snapshot.get(idx).copied().flatten(),
                split.best_segment,
            ) {
                (Some(old_best), Some(new_best)) => new_best < old_best,
                (None, Some(_)) => true,
                _ => false,
            };

            // Format name
            let name_fmt = {
                let truncated = Splits::truncate_name(&split.name, name_width);
                Splits::pad_str(&truncated, name_width)
            };

            // Format time
            let time_fmt = Splits::format_time(time);

            // Format delta
            let delta_fmt = match delta {
                Some(d) if gold => {
                    format!("-{:02}:{:02}", (-d) / 60, (-d) % 60).color(Color::TrueColor {
                        r: 255,
                        g: 227,
                        b: 0,
                    })
                }
                Some(d) if d >= 0 => format!("+{:02}:{:02}", d / 60, d % 60).red(),
                Some(d) if d < 0 => format!("-{:02}:{:02}", (-d) / 60, (-d) % 60).green(),
                _ => String::from("      ").white(),
            };

            lines.push(format!("{} {:>8} {:>8}", name_fmt, delta_fmt, time_fmt));
        }

        // --- 5. Append BPT ---
        let bpt = splits.best_possible_time();
        // Blank line to separate splits from BPT
        lines.push(String::new());

        let name_width = splits.compute_name_width();
        let name_fmt = Splits::pad_str("BPT:", name_width);
        let time_fmt = Splits::format_time(bpt);
        lines.push(format!("{} {:>8} {:>8}", name_fmt, "      ", time_fmt));

        lines
    }
}
