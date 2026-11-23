//! Human readable formatting and parsing helpers.
//!
//! Sizes use the binary multiples that file managers conventionally show
//! (1 KiB = 1024 B) while still honouring the classic "1 k" style used in
//! status bars.

use chrono::{DateTime, Local, TimeZone};

/// Format a byte count in the compact "1 234 k" / "12 345 678" style used in
/// directory listings: exact byte counts below 1024, otherwise kilobytes with
/// thousands separators.
pub fn format_size_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        bytes.to_string()
    } else {
        let k = bytes / 1024;
        format_thousands(k)
    }
}

/// Format a byte count with an explicit binary unit suffix, e.g. `1.5 MiB`.
pub fn format_size_unit(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut idx = 0;
    while value >= 1024.0 && idx + 1 < UNITS.len() {
        value /= 1024.0;
        idx += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[idx])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[idx])
    } else {
        format!("{value:.2} {}", UNITS[idx])
    }
}

/// Insert a thin space as a thousands separator (no locale dependency).
pub fn format_thousands(value: u64) -> String {
    let s = value.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = bytes.len();
    for (i, ch) in s.char_indices() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Format a timestamp as `YYYY-MM-DD HH:MM` in local time.
pub fn format_datetime(ts: Option<i64>) -> String {
    match ts.and_then(|t| Local.timestamp_opt(t, 0).single()) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => String::new(),
    }
}

/// Format a timestamp as `YYYY-MM-DD`.
pub fn format_date(ts: Option<i64>) -> String {
    match ts.and_then(|t| Local.timestamp_opt(t, 0).single()) {
        Some(dt) => dt.format("%Y-%m-%d").to_string(),
        None => String::new(),
    }
}

/// Parse the flexible date syntax used by search filters.
///
/// Accepted forms: `YYYY-MM-DD`, `YYYY-MM-DD HH:MM`, `DD.MM.YYYY` and the
/// relative keywords `today`, `yesterday`, `-N` (N days ago) and `-Nm`
/// (N months ago). Returns a unix timestamp in local time.
pub fn parse_date_filter(input: &str) -> Option<i64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    let today_start = {
        let now = Local::now();
        now.date_naive().and_hms_opt(0, 0, 0)?
    };
    match lower.as_str() {
        "today" => return Some(today_start.and_local_timezone(Local).single()?.timestamp()),
        "yesterday" => {
            let d = today_start - chrono::Duration::days(1);
            return Some(d.and_local_timezone(Local).single()?.timestamp());
        }
        _ => {}
    }
    if let Some(rest) = lower.strip_prefix('-') {
        if let Some(days) = rest.strip_suffix('d').or(Some(rest))
            && let Ok(n) = days.parse::<i64>()
        {
            let d = today_start - chrono::Duration::days(n);
            return Some(d.and_local_timezone(Local).single()?.timestamp());
        }
        if let Some(months) = rest.strip_suffix('m')
            && let Ok(n) = months.parse::<u32>()
        {
            let date = today_start.date();
            let (_, y) = date.year_ce();
            let m = date.month0();
            let total = y as i64 * 12 + m as i64 - n as i64;
            let ny = total.div_euclid(12) as i32;
            let nm = total.rem_euclid(12) as u32;
            let d = chrono::NaiveDate::from_ymd_opt(ny, nm + 1, 1)?.and_hms_opt(0, 0, 0)?;
            return Some(d.and_local_timezone(Local).single()?.timestamp());
        }
    }
    for fmt in ["%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(naive.and_local_timezone(Local).single()?.timestamp());
        }
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = date.and_hms_opt(0, 0, 0)?;
        return Some(dt.and_local_timezone(Local).single()?.timestamp());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%d.%m.%Y") {
        let dt = date.and_hms_opt(0, 0, 0)?;
        return Some(dt.and_local_timezone(Local).single()?.timestamp());
    }
    None
}

use chrono::Datelike;

/// Format a duration in seconds as `H:MM:SS` (used by transfer dialogs).
pub fn format_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h}:{m:02}:{s:02}")
}

/// Format a transfer rate in bytes/second.
pub fn format_speed(bytes_per_sec: f64) -> String {
    if !bytes_per_sec.is_finite() || bytes_per_sec < 0.0 {
        return String::from("-");
    }
    format!("{}/s", format_size_unit(bytes_per_sec as u64))
}

/// Format a timestamp the way the ISO-like `descript.ion` files want it.
pub fn format_comment_date(ts: Option<i64>) -> String {
    match ts.and_then(|t| Local.timestamp_opt(t, 0).single()) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => String::new(),
    }
}

/// Format a `DateTime<Local>` as an ISO timestamp.
pub fn format_iso(dt: DateTime<Local>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_sizes_are_exact_bytes() {
        assert_eq!(format_size_bytes(0), "0");
        assert_eq!(format_size_bytes(1), "1");
        assert_eq!(format_size_bytes(1023), "1023");
    }

    #[test]
    fn larger_sizes_are_kilobytes_with_thousands_separator() {
        assert_eq!(format_size_bytes(1024), "1");
        assert_eq!(format_size_bytes(1536), "1");
        assert_eq!(format_size_bytes(1_048_576), "1 024");
        assert_eq!(format_size_bytes(12_345_678), "12 056");
    }

    #[test]
    fn thousands_separator_groups_by_three() {
        assert_eq!(format_thousands(0), "0");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_thousands(1000), "1 000");
        assert_eq!(format_thousands(123456789), "123 456 789");
    }

    #[test]
    fn units_scale_and_round() {
        assert_eq!(format_size_unit(512), "512 B");
        assert_eq!(format_size_unit(1024), "1.00 KiB");
        assert_eq!(format_size_unit(1536), "1.50 KiB");
        assert_eq!(format_size_unit(20 * 1024 * 1024), "20.0 MiB");
        assert_eq!(format_size_unit(150 * 1024 * 1024), "150 MiB");
    }

    #[test]
    fn durations_are_hh_mm_ss() {
        assert_eq!(format_duration(0), "0:00:00");
        assert_eq!(format_duration(61), "0:01:01");
        assert_eq!(format_duration(3661), "1:01:01");
    }

    #[test]
    fn speed_is_human_readable() {
        assert_eq!(format_speed(0.0), "0 B/s");
        assert_eq!(format_speed(-1.0), "-");
        assert_eq!(format_speed(f64::NAN), "-");
        assert!(format_speed(1_500_000.0).ends_with("MiB/s"));
    }

    #[test]
    fn date_filters_understand_relative_keywords() {
        let today = parse_date_filter("today").expect("today parses");
        let now = Local::now().timestamp();
        assert!(today <= now && now - today < 86_400 * 2);
        assert!(parse_date_filter("yesterday").unwrap() < today);
        let week_ago = parse_date_filter("-7").unwrap();
        assert!(week_ago < today - 6 * 86_400);
        assert!(parse_date_filter("bogus").is_none());
    }

    #[test]
    fn absolute_dates_parse_in_several_notations() {
        assert!(parse_date_filter("2024-05-06").is_some());
        assert!(parse_date_filter("2024-05-06 07:08").is_some());
        assert!(parse_date_filter("06.05.2024").is_some());
        let a = parse_date_filter("2024-05-06").unwrap();
        let b = parse_date_filter("06.05.2024").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn datetime_formatting_of_none_is_empty() {
        assert_eq!(format_datetime(None), "");
        assert_eq!(format_date(None), "");
        assert_eq!(format_comment_date(None), "");
    }
}
