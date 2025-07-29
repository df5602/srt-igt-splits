use anyhow::{Result, anyhow};
use std::{fmt, time::Duration};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InGameTime {
    pub percent: u32,
    pub duration: Duration,
}

impl InGameTime {
    /// Parses a string like ": 117% 3:03:23" into an `InGameTime`
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Remove optional leading ':'
        let s = if let Some(rest) = s.strip_prefix(':') {
            rest.trim()
        } else {
            s
        };

        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(anyhow!("Expected two parts: percentage and time"));
        }

        let percent_str = parts[0].trim_end_matches('%');
        let percent: u32 = percent_str.parse()?;

        let time_parts: Vec<&str> = parts[1].split(':').collect();
        if time_parts.len() != 3 {
            return Err(anyhow!("Invalid time '{}': must be H:MM:SS", parts[1]));
        }

        let hours_str = time_parts[0];
        let minutes_str = time_parts[1];
        let seconds_str = time_parts[2];

        // Enforce exactly two digits for MM and SS
        if minutes_str.len() != 2 || seconds_str.len() != 2 {
            return Err(anyhow!(
                "Minutes and seconds must be exactly two digits (MM:SS)"
            ));
        }

        let hours: u64 = hours_str.parse()?;
        let minutes: u64 = minutes_str.parse()?;
        let seconds: u64 = seconds_str.parse()?;

        if minutes >= 60 || seconds >= 60 {
            return Err(anyhow!("Minutes and seconds must be < 60"));
        }

        let duration = Duration::from_secs(hours * 3600 + minutes * 60 + seconds);

        Ok(Self { percent, duration })
    }
}

impl fmt::Display for InGameTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total_secs = self.duration.as_secs();
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;

        write!(
            f,
            "{}% {:01}:{:02}:{:02}",
            self.percent, hours, minutes, seconds
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_parse_valid_input() {
        let input = ": 117% 3:03:23";
        let result = InGameTime::parse(input).unwrap();
        assert_eq!(
            result,
            InGameTime {
                percent: 117,
                duration: Duration::new(3 * 3600 + 3 * 60 + 23, 0),
            }
        );
    }

    #[test]
    fn test_parse_without_colon_prefix() {
        let input = "85% 0:59:01";
        let result = InGameTime::parse(input).unwrap();
        assert_eq!(
            result,
            InGameTime {
                percent: 85,
                duration: Duration::new(59 * 60 + 1, 0),
            }
        );
    }

    #[test]
    fn test_parse_invalid_format_too_few_parts() {
        let input = "85%";
        let result = InGameTime::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_format_time_wrong() {
        let input = "85% 59:01";
        let result = InGameTime::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_large_duration_and_percent() {
        let input = ": 999% 123:45:59";
        let result = InGameTime::parse(input).unwrap();
        assert_eq!(
            result,
            InGameTime {
                percent: 999,
                duration: Duration::new(123 * 3600 + 45 * 60 + 59, 0),
            }
        );
        assert_eq!(format!("{}", result), "999% 123:45:59");
    }

    #[test]
    fn test_parse_extra_whitespace() {
        let input = "   :   42%    1:02:03   ";
        let result = InGameTime::parse(input).unwrap();
        assert_eq!(
            result,
            InGameTime {
                percent: 42,
                duration: Duration::new(1 * 3600 + 2 * 60 + 3, 0),
            }
        );
    }

    #[test]
    fn test_parse_too_many_time_parts() {
        let input = ": 10% 1:02:03:04";
        let result = InGameTime::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_too_few_time_parts() {
        let input = ": 10% 45";
        let result = InGameTime::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_single_digit_seconds_should_fail() {
        let input = "42% 1:03:2"; // seconds not two-digit
        let result = InGameTime::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_minutes_equal_to_60_should_fail() {
        let input = "42% 1:60:00"; // minutes = 60 (invalid)
        let result = InGameTime::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_seconds_equal_to_60_should_fail() {
        let input = "42% 1:00:60"; // seconds = 60 (invalid)
        let result = InGameTime::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_minutes_above_60_should_fail() {
        let input = "42% 1:75:00"; // minutes > 60 (invalid)
        let result = InGameTime::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_seconds_above_60_should_fail() {
        let input = "42% 1:00:75"; // seconds > 60 (invalid)
        let result = InGameTime::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_minutes_and_seconds_just_below_60_should_pass() {
        let input = "42% 1:59:59"; // valid upper bound
        let result = InGameTime::parse(input);
        assert!(result.is_ok());
    }
}
