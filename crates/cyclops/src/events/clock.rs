use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Clock {
    monotonic_start: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timestamp {
    pub ts_ns: u64,
    pub ts_wall: String,
}

impl Clock {
    pub fn new() -> Self {
        Self {
            monotonic_start: Instant::now(),
        }
    }

    pub fn now(&self) -> Timestamp {
        Timestamp {
            ts_ns: duration_as_saturating_ns(self.monotonic_start.elapsed()),
            ts_wall: format_system_time_rfc3339_nano(SystemTime::now()),
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

fn duration_as_saturating_ns(duration: Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

fn format_system_time_rfc3339_nano(time: SystemTime) -> String {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch");
    let seconds = duration.as_secs() as i64;
    let nanos = duration.subsec_nanos();
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}Z")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, u32, u32) {
    let days = days_since_unix_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_rfc3339_nano_shape(value: &str) -> bool {
        let bytes = value.as_bytes();
        value.len() == "2026-05-21T10:20:30.123456789Z".len()
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes[10] == b'T'
            && bytes[13] == b':'
            && bytes[16] == b':'
            && bytes[19] == b'.'
            && bytes[29] == b'Z'
            && bytes.iter().enumerate().all(|(index, byte)| {
                matches!(index, 4 | 7 | 10 | 13 | 16 | 19 | 29) || byte.is_ascii_digit()
            })
    }

    #[test]
    fn now_is_monotonic_for_1000_consecutive_calls() {
        let clock = Clock::new();
        let mut previous = clock.now().ts_ns;

        for _ in 0..1_000 {
            let current = clock.now().ts_ns;
            assert!(
                current >= previous,
                "timestamp regressed from {previous} to {current}"
            );
            previous = current;
        }
    }

    #[test]
    fn wall_timestamp_matches_rfc3339_nano_shape() {
        let clock = Clock::new();
        let timestamp = clock.now();

        assert!(
            is_rfc3339_nano_shape(&timestamp.ts_wall),
            "unexpected wall timestamp shape: {}",
            timestamp.ts_wall
        );
    }

    #[test]
    fn wall_timestamp_formats_known_unix_times() {
        assert_eq!(
            format_system_time_rfc3339_nano(UNIX_EPOCH),
            "1970-01-01T00:00:00.000000000Z"
        );
        assert_eq!(
            format_system_time_rfc3339_nano(UNIX_EPOCH + Duration::new(1_779_358_830, 123_456_789)),
            "2026-05-21T10:20:30.123456789Z"
        );
    }
}
