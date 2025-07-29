use std::time::Duration;

use colored::Colorize;

use crate::in_game_time::InGameTime;

#[derive(Debug, Clone)]
pub struct Split {
    pub percent: u32,
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
        macro_rules! splits {
            ( $( ($percent:expr, $name:expr, $h:expr, $m:expr, $s:expr) ),* $(,)? ) => {
                vec![
                    $(
                        Split {
                            percent: $percent,
                            name: $name.to_string(),
                            time: InGameTime {
                                percent: $percent,
                                duration: std::time::Duration::from_secs($h * 3600 + $m * 60 + $s),
                            },
                        }
                    ),*
                ]
            };
        }

        let splits = splits![
            (18, "Buzz", 0, 25, 43),
            (21, "Crawdad Farm", 0, 28, 15),
            (35, "Enchanted Towers", 0, 55, 46),
            (56, "Fireworks Factory 1", 1, 37, 48),
            (59, "Scorch", 1, 39, 15),
            (67, "Spider Town", 1, 53, 53),
            (70, "Starfish Reef", 1, 57, 23),
            (84, "Agent 9's Lab", 2, 15, 55),
            (85, "Cloud Spires 2", 2, 17, 37),
            // Skipped both 87% entries as requested
            (88, "Fireworks Factory 2", 2, 30, 18),
            (117, "Super Bonus Round", 3, 2, 25)
        ];

        Splits { splits }
    }

    /// Returns the split matching the given percent, if found.
    pub fn find_by_percent(&self, time: &InGameTime) -> Option<&Split> {
        self.splits.iter().find(|s| s.percent == time.percent)
    }

    pub fn compare_and_print(&self, current: &InGameTime) {
        if let Some(split) = self.find_by_percent(current) {
            let delta = current.duration.as_secs() as i64 - split.time.duration.as_secs() as i64;

            let delta_duration = Duration::from_secs(delta.unsigned_abs());
            let colored_delta = if delta > 0 {
                let delta_str = format!(
                    "+{:02}:{:02}",
                    delta_duration.as_secs() / 60,
                    delta_duration.as_secs() % 60
                );
                delta_str.red()
            } else {
                let delta_str = format!(
                    "-{:02}:{:02}",
                    delta_duration.as_secs() / 60,
                    delta_duration.as_secs() % 60
                );
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
