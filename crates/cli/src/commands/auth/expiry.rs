//! Saying when a key created by a browser login runs out.
//!
//! Such a key carries an expiry the server chose. A person has to log in again
//! around then, so `auth status` shows the date and speaks up when it is near.

/// How close to its expiry a key gets before `auth status` points it out.
const WARN_WITHIN_DAYS: i64 = 7;

const SECONDS_PER_DAY: i64 = 86_400;

/// How long a key has left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Remaining {
    /// The expiry has passed.
    Expired,
    /// Less than a day.
    Hours,
    /// This many days, to the nearest day.
    Days(i64),
}

impl Remaining {
    fn between(now: i64, expires_at: i64) -> Self {
        let seconds = expires_at.saturating_sub(now);
        if seconds <= 0 {
            Self::Expired
        } else if seconds < SECONDS_PER_DAY {
            Self::Hours
        } else {
            // To the nearest day, so a key issued for 90 days reads as 90 the
            // moment it is created rather than 89.
            Self::Days((seconds + SECONDS_PER_DAY / 2) / SECONDS_PER_DAY)
        }
    }
}

/// `2027-02-14 (in 90 days)`, for the status listing.
pub fn describe(expires_at: i64, now: i64) -> String {
    let date = utc_date(expires_at);
    match Remaining::between(now, expires_at) {
        Remaining::Expired => format!("{date} (expired)"),
        Remaining::Hours => format!("{date} (in less than a day)"),
        Remaining::Days(1) => format!("{date} (in 1 day)"),
        Remaining::Days(days) => format!("{date} (in {days} days)"),
    }
}

/// What to say, if anything, about a key that is running out or has run out.
pub fn warning(expires_at: i64, now: i64) -> Option<String> {
    const AGAIN: &str = "run `memorylake auth login` for a new one";
    if expires_at.saturating_sub(now) >= WARN_WITHIN_DAYS * SECONDS_PER_DAY {
        return None;
    }
    Some(match Remaining::between(now, expires_at) {
        Remaining::Expired => format!("this API key has expired; {AGAIN}"),
        Remaining::Hours => format!("this API key expires within a day; {AGAIN}"),
        Remaining::Days(1) => format!("this API key expires in 1 day; {AGAIN}"),
        Remaining::Days(days) => format!("this API key expires in {days} days; {AGAIN}"),
    })
}

/// The current time in Unix seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// The UTC calendar date of a Unix timestamp, as `YYYY-MM-DD`.
///
/// Days-to-civil conversion after Howard Hinnant's `civil_from_days`, to avoid
/// a date library for one line of output.
pub fn utc_date(unix_seconds: i64) -> String {
    let days = unix_seconds.div_euclid(SECONDS_PER_DAY);
    // Shift the epoch to 0000-03-01 so leap days fall at the end of a year.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_become_utc_dates() {
        for (seconds, date) in [
            (0, "1970-01-01"),
            (86_399, "1970-01-01"),
            (86_400, "1970-01-02"),
            (951_782_400, "2000-02-29"),
            (1_709_251_199, "2024-02-29"),
            (1_709_251_200, "2024-03-01"),
            (1_786_953_600, "2026-08-17"),
            (4_102_444_800, "2100-01-01"),
            (-1, "1969-12-31"),
        ] {
            assert_eq!(utc_date(seconds), date, "{seconds}");
        }
    }

    #[test]
    fn the_listing_says_how_long_is_left() {
        let expires = 100 * SECONDS_PER_DAY;
        let at = |days_before: i64| expires - days_before * SECONDS_PER_DAY;
        assert_eq!(describe(expires, at(89)), "1970-04-11 (in 89 days)");
        assert_eq!(
            describe(expires, at(90) + 5),
            "1970-04-11 (in 90 days)",
            "seconds after a 90-day key is issued it still reads as 90"
        );
        assert_eq!(describe(expires, at(1)), "1970-04-11 (in 1 day)");
        assert_eq!(
            describe(expires, expires - 60),
            "1970-04-11 (in less than a day)"
        );
        assert_eq!(describe(expires, expires), "1970-04-11 (expired)");
        assert_eq!(describe(expires, expires + 1), "1970-04-11 (expired)");
    }

    #[test]
    fn a_warning_comes_only_when_the_end_is_near() {
        let expires = 100 * SECONDS_PER_DAY;
        let at = |days_before: i64| expires - days_before * SECONDS_PER_DAY;
        assert_eq!(warning(expires, at(30)), None);
        assert_eq!(warning(expires, at(7)), None);
        assert!(
            warning(expires, at(7) + 1)
                .expect("near")
                .contains("in 7 days")
        );
        assert!(warning(expires, at(3)).expect("near").contains("in 3 days"));
        assert!(warning(expires, at(1)).expect("near").contains("in 1 day;"));
        assert!(
            warning(expires, expires - 60)
                .expect("near")
                .contains("within a day")
        );
        assert!(
            warning(expires, expires + 60)
                .expect("past")
                .contains("has expired")
        );
    }
}
