//! Small time helpers.
//!
//! Archive timestamps arrive as DOS date/time pairs (zip), octal seconds (tar),
//! Windows FILETIME (7z) or [`std::time::SystemTime`] (file system metadata).
//! Everything is converted to unix seconds, which is what
//! [`rc_core::entry::Entry::mtime`] wants.

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`), used by the DOS timestamp conversion.
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Convert a broken-down UTC date/time into unix seconds.
///
/// Returns `None` for obviously invalid fields so that a corrupt header cannot
/// produce a nonsense timestamp.
pub(crate) fn civil_to_unix(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
) -> Option<i64> {
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Convert a zip [`zip::DateTime`] (DOS date/time, local time by format
/// convention) into unix seconds.
pub(crate) fn dos_to_unix(time: zip::DateTime) -> Option<i64> {
    civil_to_unix(
        i64::from(time.year()),
        i64::from(time.month()),
        i64::from(time.day()),
        i64::from(time.hour()),
        i64::from(time.minute()),
        i64::from(time.second()),
    )
}

/// Convert a Windows FILETIME (100 ns ticks since 1601-01-01 UTC) into unix
/// seconds, saturating instead of panicking on absurd values.
pub(crate) fn filetime_to_unix(ticks: u64) -> Option<i64> {
    const TICKS_PER_SEC: u64 = 10_000_000;
    const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    let ticks = ticks.max(UNIX_EPOCH_TICKS);
    i64::try_from((ticks - UNIX_EPOCH_TICKS) / TICKS_PER_SEC).ok()
}

/// Convert a [`std::time::SystemTime`] into unix seconds.
pub(crate) fn system_time_to_unix(time: std::time::SystemTime) -> Option<i64> {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).ok(),
        Err(e) => i64::try_from(e.duration().as_secs()).ok().map(|s| -s),
    }
}

/// Current time as unix seconds (0 if the clock is before the epoch).
pub(crate) fn now_unix() -> i64 {
    system_time_to_unix(std::time::SystemTime::now()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_epoch_conversions_match_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 2), 1);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(civil_to_unix(1970, 1, 1, 0, 0, 0), Some(0));
        assert_eq!(civil_to_unix(2020, 1, 2, 3, 4, 5), Some(1_577_934_245));
    }

    #[test]
    fn invalid_civil_values_are_rejected() {
        assert_eq!(civil_to_unix(2020, 13, 1, 0, 0, 0), None);
        assert_eq!(civil_to_unix(2020, 0, 1, 0, 0, 0), None);
        assert_eq!(civil_to_unix(2020, 1, 32, 0, 0, 0), None);
        assert_eq!(civil_to_unix(2020, 1, 1, 24, 0, 0), None);
        assert_eq!(civil_to_unix(2020, 1, 1, 0, 60, 0), None);
    }

    #[test]
    fn dos_timestamps_convert() {
        let dt = zip::DateTime::from_date_and_time(2020, 1, 2, 3, 4, 4).unwrap();
        assert_eq!(dos_to_unix(dt), civil_to_unix(2020, 1, 2, 3, 4, 4));
        assert!(dos_to_unix(zip::DateTime::default()).is_some());
    }

    #[test]
    fn filetime_and_systemtime_convert() {
        assert_eq!(filetime_to_unix(116_444_736_000_000_000), Some(0));
        assert_eq!(
            filetime_to_unix(0),
            Some(0),
            "values before 1970 saturate at the epoch"
        );
        let now = std::time::SystemTime::now();
        let unix = system_time_to_unix(now).unwrap();
        assert!((unix - now_unix()).abs() < 5);
    }
}
