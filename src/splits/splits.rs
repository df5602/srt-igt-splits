use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use colored::Colorize;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::in_game_time::InGameTime;

#[derive(Debug, Clone)]
pub struct Split {
    pub name: String,
    pub time: InGameTime,
}

#[derive(Debug)]
pub struct Splits {
    pub path: Option<PathBuf>,
    pub splits: Vec<Split>,
}

impl Splits {
    /// Constructs `Splits` with some placeholder test data.
    pub fn new() -> Self {
        Splits {
            path: None,
            splits: Vec::new(),
        }
    }

    /// Loads splits from a file
    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        crate::splits::file_persistency::load_from_file(path)
    }

    /// Save splits to file
    pub fn save_to_file(&self) -> anyhow::Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No file path to save to"))?;

        crate::splits::file_persistency::save_to_file(self, path)
    }

    pub fn add_split(&mut self, name: String, time: InGameTime) {
        self.splits.push(Split { name, time });
        self.splits
            .sort_by(|a, b| a.time.percent.cmp(&b.time.percent));
    }

    /// Returns the split matching the given percent, if found.
    pub fn find_by_percent(&self, time: &InGameTime) -> Option<&Split> {
        self.splits.iter().find(|s| s.time.percent == time.percent)
    }

    pub fn compare(&self, current: &InGameTime) -> Option<(i64, &Split)> {
        if let Some(split) = self.find_by_percent(current) {
            let delta = current.duration.as_secs() as i64 - split.time.duration.as_secs() as i64;
            Some((delta, split))
        } else {
            None
        }
    }

    pub fn compare_and_print(&self, current: &InGameTime) {
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

            let current_str = Self::format_time(current.duration);
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
            let display_name = Self::truncate_name(&split.name, name_width);
            let duration_str = Self::format_time(split.time.duration);
            println!(
                "{} {:>8} {:>8}",
                Self::pad_str(&display_name, name_width),
                " ",
                duration_str
            );
        }
    }

    fn compute_name_width(&self) -> usize {
        const MAX_NAME_WIDTH: usize = 25;

        self.splits
            .iter()
            .map(|s| s.name.len())
            .max()
            .map(|len| len.min(MAX_NAME_WIDTH))
            .unwrap_or(0)
    }

    /// Truncates a string to a max display width. If truncation is needed, adds "..".
    fn truncate_name(name: &str, max_display_width: usize) -> String {
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
    fn pad_str(s: &str, target_width: usize) -> String {
        let width = UnicodeWidthStr::width(s);
        if width >= target_width {
            s.to_string()
        } else {
            let pad = " ".repeat(target_width - width);
            format!("{}{}", s, pad)
        }
    }

    fn format_time(duration: Duration) -> String {
        let secs = duration.as_secs();
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;
        format!("{:01}:{:02}:{:02}", hours, minutes, seconds)
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
    fn add_split_sorts_by_percent() {
        let mut splits = Splits::new();
        splits.add_split("Second".into(), make_ingame_time(50, 0, 10, 0));
        splits.add_split("First".into(), make_ingame_time(25, 0, 5, 0));
        splits.add_split("Third".into(), make_ingame_time(75, 0, 15, 0));

        let names: Vec<_> = splits.splits.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn find_by_percent_finds_correct_split() {
        let mut splits = Splits::new();
        splits.add_split("Alpha".into(), make_ingame_time(10, 0, 1, 0));
        splits.add_split("Beta".into(), make_ingame_time(20, 0, 2, 0));

        let result = splits.find_by_percent(&make_ingame_time(20, 0, 0, 0));
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Beta");
    }

    #[test]
    fn find_by_percent_returns_none_for_unknown_percent() {
        let mut splits = Splits::new();
        splits.add_split("One".into(), make_ingame_time(30, 0, 3, 0));

        let result = splits.find_by_percent(&make_ingame_time(40, 0, 0, 0));
        assert!(result.is_none());
    }

    #[test]
    fn compare_returns_correct_positive_delta() {
        let mut splits = Splits::new();
        splits.add_split("One".into(), make_ingame_time(50, 0, 8, 30));
        splits.add_split("Two".into(), make_ingame_time(60, 0, 10, 0));

        let current = make_ingame_time(60, 0, 11, 0); // +60s
        let result = splits.compare(&current);

        assert!(result.is_some());
        let (delta, split) = result.unwrap();
        assert_eq!(delta, 60);
        assert_eq!(split.name, "Two");
    }

    #[test]
    fn compare_returns_correct_negative_delta() {
        let mut splits = Splits::new();
        splits.add_split("One".into(), make_ingame_time(70, 0, 15, 0));

        let current = make_ingame_time(70, 0, 14, 30); // -30s
        let result = splits.compare(&current);

        assert!(result.is_some());
        let (delta, split) = result.unwrap();
        assert_eq!(delta, -30);
        assert_eq!(split.name, "One");
    }

    #[test]
    fn compare_returns_zero_delta() {
        let mut splits = Splits::new();
        let time = make_ingame_time(40, 0, 5, 30);
        splits.add_split("One".into(), time.clone());

        let result = splits.compare(&time);
        assert!(result.is_some());

        let (delta, split) = result.unwrap();
        assert_eq!(delta, 0);
        assert_eq!(split.name, "One");
    }

    #[test]
    fn compare_returns_none_for_nonexistent_split() {
        let mut splits = Splits::new();
        splits.add_split("Unrelated".into(), make_ingame_time(10, 0, 1, 0));

        let result = splits.compare(&make_ingame_time(99, 0, 2, 0));
        assert!(result.is_none());
    }
}
