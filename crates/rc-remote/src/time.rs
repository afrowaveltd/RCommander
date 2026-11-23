//! Minimal proleptic-Gregorian UTC calendar arithmetic.
//!
//! `rc-remote` needs to turn the two date formats that FTP speaks into unix
//! timestamps: `MDTM`/`MLSD` use `YYYYMMDDHHMMSS[.sss]` in UTC, and the unix
//! `LIST` format uses `Mon DD HH:MM` (recent files) or `Mon DD YYYY`. A full
//! date/time library is unnecessary for that, so this module implements the
//! two classic civil-calendar algorithms (Howard Hinnant's `days_from_civil`
//! and `civil_from_days`) together with the seconds↔timestamp conversion.
//!
//! Everything here is UTC: FTP dates are UTC by specification, so no time zone
//! database or local-time offset is ever consulted.

/// Seconds in a day.
pub(crate) const DAY: i64 = 86_400;

/// Days since 1970-01-01 for a Gregorian date.
pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400; // [0, 399]
    let shifted_month = (month as i64 + 9) % 12; // Mar = 0 … Feb = 11
    let day_of_year = (153 * shifted_month + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Inverse of [`days_from_civil`]: `(year, month, day)` for a day count.
pub(crate) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// True for leap years in the proleptic Gregorian calendar.
pub(crate) fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Number of days in a month (1–12) of a given year.
pub(crate) fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Build a UTC unix timestamp, or `None` for a date that does not exist.
pub(crate) fn timestamp_from_utc(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<i64> {
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some(
        days_from_civil(year, month, day) * DAY
            + hour as i64 * 3_600
            + minute as i64 * 60
            + second as i64,
    )
}

/// Break a unix timestamp into `(year, month, day, hour, minute, second)` in UTC.
pub(crate) fn civil_from_timestamp(timestamp: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = timestamp.div_euclid(DAY);
    let seconds = timestamp.rem_euclid(DAY);
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (seconds / 3_600) as u32,
        (seconds % 3_600 / 60) as u32,
        (seconds % 60) as u32,
    )
}

/// Current time as a unix timestamp, or `0` if the system clock is before 1970.
pub(crate) fn utc_now() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(delta) => delta.as_secs() as i64,
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trips_across_centuries() {
        for (y, m, d) in [
            (1970, 1, 1),
            (2000, 2, 29),
            (1900, 3, 1),
            (2024, 12, 31),
            (1999, 7, 15),
            (2100, 1, 1),
        ] {
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d), "{y}-{m}-{d}");
        }
    }

    #[test]
    fn epoch_and_known_timestamps_are_correct() {
        assert_eq!(timestamp_from_utc(1970, 1, 1, 0, 0, 0), Some(0));
        assert_eq!(timestamp_from_utc(2000, 1, 1, 0, 0, 0), Some(946_684_800));
        assert_eq!(
            timestamp_from_utc(2024, 1, 1, 12, 0, 0),
            Some(1_704_110_400)
        );
        assert_eq!(civil_from_timestamp(1_704_110_400), (2024, 1, 1, 12, 0, 0));
    }

    #[test]
    fn impossible_dates_are_rejected() {
        assert_eq!(timestamp_from_utc(2023, 2, 29, 0, 0, 0), None);
        assert_eq!(timestamp_from_utc(2023, 13, 1, 0, 0, 0), None);
        assert_eq!(timestamp_from_utc(2023, 4, 31, 0, 0, 0), None);
        assert_eq!(timestamp_from_utc(2023, 1, 1, 24, 0, 0), None);
        assert_eq!(timestamp_from_utc(2023, 1, 1, 0, 60, 0), None);
        assert!(timestamp_from_utc(2024, 2, 29, 0, 0, 0).is_some());
    }

    #[test]
    fn leap_year_and_month_lengths() {
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2024));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 4), 30);
        assert_eq!(days_in_month(2024, 12), 31);
        assert_eq!(days_in_month(2024, 0), 0);
    }

    #[test]
    fn now_is_after_the_epoch() {
        assert!(utc_now() > 1_600_000_000);
    }
}
