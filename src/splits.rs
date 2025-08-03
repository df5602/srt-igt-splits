use std::time::Duration;

use colored::Colorize;

use crate::in_game_time::InGameTime;

#[derive(Debug, Clone)]
pub struct Split {
    pub name: String,
    pub time: InGameTime,
}

#[derive(Debug)]
pub struct Splits {
    pub splits: Vec<Split>,
}

impl Splits {
    /// Constructs `Splits` with some placeholder test data.
    pub fn new() -> Self {
        Splits { splits: Vec::new() }
    }

    pub fn add_split(&mut self, name: String, time: InGameTime) {
        self.splits.push(Split { name, time });
        self.splits
            .sort_by(|a, b| a.time.percent.cmp(&b.time.percent));
    }

    // Temporary prototype code, to be removed (load from file)
    pub fn create_from_current_pb() -> Self {
        let mut splits = Splits::new();
        macro_rules! splits {
            ( $( ($percent:expr, $name:expr, $h:expr, $m:expr, $s:expr) ),* $(,)? ) => {
                    $(
                        splits.add_split($name.to_string(),
                            InGameTime {
                                percent: $percent,
                                duration: std::time::Duration::from_secs($h * 3600 + $m * 60 + $s),
                            });
                    )*
            };
        }

        splits![
            (18, "Buzz", 0, 25, 43),
            (21, "Crawdad Farm", 0, 28, 15),
            (35, "Enchanted Towers", 0, 55, 46),
            (56, "Fireworks Factory 1", 1, 37, 48),
            (59, "Scorch", 1, 39, 15),
            (67, "Spider Town", 1, 53, 53),
            (70, "Starfish Reef", 1, 57, 23),
            (84, "Agent 9's Lab", 2, 15, 55),
            (85, "Cloud Spires 2", 2, 17, 37),
            (88, "Fireworks Factory 2", 2, 30, 18),
            (117, "Super Bonus Round", 3, 2, 25)
        ];

        splits
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
            let colored_delta = if delta >= 0 {
                let delta_str = format!("+{:02}:{:02}", delta / 60, delta % 60);
                delta_str.red()
            } else {
                let delta_str = format!("-{:02}:{:02}", delta.abs() / 60, delta.abs() % 60);
                delta_str.green()
            };

            let current_str = Self::format_time(current.duration);
            println!("{:<22} {:>8} {:>8}", split.name, colored_delta, current_str);
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
    fn test_new_splits_is_empty() {
        let splits = Splits::new();
        assert!(splits.splits.is_empty());
    }

    #[test]
    fn test_add_split_sorts_by_percent() {
        let mut splits = Splits::new();
        splits.add_split("Second".into(), make_ingame_time(50, 0, 10, 0));
        splits.add_split("First".into(), make_ingame_time(25, 0, 5, 0));
        splits.add_split("Third".into(), make_ingame_time(75, 0, 15, 0));

        let names: Vec<_> = splits.splits.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn test_find_by_percent_finds_correct_split() {
        let mut splits = Splits::new();
        splits.add_split("Alpha".into(), make_ingame_time(10, 0, 1, 0));
        splits.add_split("Beta".into(), make_ingame_time(20, 0, 2, 0));

        let result = splits.find_by_percent(&make_ingame_time(20, 0, 0, 0));
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Beta");
    }

    #[test]
    fn test_find_by_percent_returns_none_for_unknown_percent() {
        let mut splits = Splits::new();
        splits.add_split("One".into(), make_ingame_time(30, 0, 3, 0));

        let result = splits.find_by_percent(&make_ingame_time(40, 0, 0, 0));
        assert!(result.is_none());
    }

    #[test]
    fn test_compare_returns_correct_positive_delta() {
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
    fn test_compare_returns_correct_negative_delta() {
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
    fn test_compare_returns_zero_delta() {
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
    fn test_compare_returns_none_for_nonexistent_split() {
        let mut splits = Splits::new();
        splits.add_split("Unrelated".into(), make_ingame_time(10, 0, 1, 0));

        let result = splits.compare(&make_ingame_time(99, 0, 2, 0));
        assert!(result.is_none());
    }
}
