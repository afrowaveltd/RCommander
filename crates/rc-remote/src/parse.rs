//! Parsers for the two textual directory formats FTP speaks.
//!
//! * [`parse_mlsd_line`] understands the machine-readable `MLSD`/`MLST` format
//!   of RFC 3659 (`type=file;size=…;modify=…; name`). It is the authoritative
//!   source for size, modification time and permissions.
//! * [`parse_list_line`] understands the traditional unix `LIST` format
//!   (`-rw-r--r-- 1 owner group 1234 Jan 12 12:34 name`), which is all that
//!   older servers offer.
//!
//! Both parsers are total functions on hostile input: they never panic and they
//! return `None` instead of inventing data. They accept exactly the fields they
//! understand and ignore the rest (owner and group are dropped, `perm=` facts
//! are dropped because they describe capabilities, not modes).
//!
//! Known limits, all of them deliberate:
//!
//! * the unix `LIST` format has no standard for anything: month names are
//!   English-only, `owner`/`group` are ignored, and a `LIST` line whose date
//!   cannot be understood still yields an entry, just without an mtime;
//! * `LIST` never reveals whether a symlink points at a file or a directory, so
//!   `is_dir` is `false` for symlinks — use [`crate::RemoteFs::stat`] to find out;
//! * a file name that literally contains `" -> "` is split at that arrow.

use crate::entry::RemoteEntry;
use crate::time::{DAY, civil_from_timestamp, timestamp_from_utc, utc_now};

/// What the first column of a unix `LIST` line says the entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    /// Directory (`d`).
    dir: bool,
    /// Symbolic link (`l`).
    link: bool,
    /// Block/character device, socket or fifo (`b`, `c`, `s`, `p`).
    special: bool,
}

impl Shape {
    /// Classify the type character of a `LIST` line.
    fn from_char(c: char) -> Option<Shape> {
        let shape = match c {
            '-' => Shape {
                dir: false,
                link: false,
                special: false,
            },
            'd' | 'D' => Shape {
                dir: true,
                link: false,
                special: false,
            },
            'l' | 'L' => Shape {
                dir: false,
                link: true,
                special: false,
            },
            'b' | 'B' | 'c' | 'C' | 'p' | 'P' | 's' | 'S' => Shape {
                dir: false,
                link: false,
                special: true,
            },
            _ => return None,
        };
        Some(shape)
    }
}

/// One whitespace-delimited field of a `LIST` line plus its byte offset.
struct Token<'a> {
    /// Byte offset of the field inside the tokenised string.
    start: usize,
    /// The field itself.
    text: &'a str,
}

/// Split on runs of single spaces, keeping the byte offsets.
fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (index, c) in text.char_indices() {
        if c == ' ' {
            if let Some(begin) = start.take() {
                out.push(Token {
                    start: begin,
                    text: &text[begin..index],
                });
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(begin) = start {
        out.push(Token {
            start: begin,
            text: &text[begin..],
        });
    }
    out
}

/// Permission bit contribution of the `index`-th `rwx` character.
fn permission_bit(index: usize, c: char) -> Option<u32> {
    let bit = match (index, c) {
        (0, 'r') => 0o400,
        (1, 'w') => 0o200,
        (2, 'x') => 0o100,
        (2, 's') => 0o100 | 0o4000,
        (2, 'S') => 0o4000,
        (2, 't') => 0o100 | 0o1000,
        (2, 'T') => 0o1000,
        (3, 'r') => 0o040,
        (4, 'w') => 0o020,
        (5, 'x') => 0o010,
        (5, 's') => 0o010 | 0o2000,
        (5, 'S') => 0o2000,
        (5, 't') => 0o010 | 0o1000,
        (5, 'T') => 0o1000,
        (6, 'r') => 0o004,
        (7, 'w') => 0o002,
        (8, 'x') => 0o001,
        (8, 't') => 0o001 | 0o1000,
        (8, 'T') => 0o1000,
        (_, '-') | (_, '?') => 0,
        _ => return None,
    };
    Some(bit)
}

/// Month number of an English month name or abbreviation.
fn month_number(name: &str) -> Option<u32> {
    let prefix: String = name
        .chars()
        .take(3)
        .collect::<String>()
        .to_ascii_lowercase();
    let month = match prefix.as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    Some(month)
}

/// Parse the size field of a `LIST` line.
///
/// Device and fifo lines print `major, minor` instead of a byte count; those
/// report a size of zero. Everything else must be a plain `u64`, which is why a
/// negative or oversized size makes the line unparsable.
fn parse_list_size(token: &str, previous: Option<&str>, shape: Shape) -> Option<u64> {
    if shape.special {
        let major_marker = token.ends_with(',')
            || token
                .trim_end_matches(',')
                .bytes()
                .all(|b| b.is_ascii_digit());
        let minor_after_marker = previous.map(|p| p.ends_with(',')).unwrap_or(false);
        if major_marker && minor_after_marker {
            return Some(0);
        }
        if token.ends_with(',') {
            return Some(0);
        }
    }
    token.parse::<u64>().ok()
}

/// Parse `HH:MM` or `HH:MM:SS`.
fn parse_clock(token: &str) -> Option<(u32, u32, u32)> {
    let mut parts = token.split(':');
    let hour: u32 = parts.next()?.parse().ok()?;
    let minute: u32 = parts.next()?.parse().ok()?;
    let second: u32 = match parts.next() {
        Some(text) => text.parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((hour, minute, second))
}

/// Turn `Mon DD <time|year>` into a unix timestamp.
///
/// `LIST` omits the year for files younger than roughly six months, so a time is
/// resolved against `now`: a time that would be more than a day in the future
/// must belong to the previous year (that is the classic `Dec 31 23:59` case).
fn resolve_list_date(month: u32, day: u32, token: &str, now: i64) -> Option<i64> {
    if let Some((hour, minute, second)) = parse_clock(token) {
        let (year, ..) = civil_from_timestamp(now);
        let stamp = timestamp_from_utc(year, month, day, hour, minute, second)?;
        if stamp > now + DAY {
            return timestamp_from_utc(year - 1, month, day, hour, minute, second);
        }
        return Some(stamp);
    }
    if token.len() == 4 && token.bytes().all(|b| b.is_ascii_digit()) {
        let year: i64 = token.parse().ok()?;
        return timestamp_from_utc(year, month, day, 0, 0, 0);
    }
    None
}

/// Split `name -> target` at the arrow of a symlink line.
///
/// A trailing arrow without a target (`broken ->`) keeps the name and reports
/// `None`, which is what a server says when it cannot resolve the link.
fn split_arrow(text: &str) -> (&str, Option<String>) {
    match text.find(" ->") {
        Some(position) => {
            let name = text[..position].trim_end();
            let target = text[position + 3..].trim();
            let target = if target.is_empty() {
                None
            } else {
                Some(target.to_string())
            };
            (name, target)
        }
        None => (text, None),
    }
}

/// Parse one line of a unix-format `LIST` reply into a [`RemoteEntry`].
///
/// Returns `None` for anything that is not a unix `LIST` line: the `total 12`
/// header, a blank line, a DOS/IIS listing, or a line with a truncated or
/// nonsensical field. A file name keeps its spaces, leading dots and any other
/// character; a `-> target` arrow fills in [`RemoteEntry::target`].
///
/// ```
/// use rc_remote::parse_list_line;
///
/// let entry = parse_list_line("-rw-r--r-- 1 owner group 42 Jan 12  2024 notes.txt").unwrap();
/// assert_eq!(entry.name, "notes.txt");
/// assert_eq!(entry.size, 42);
/// assert!(!entry.is_dir);
/// assert_eq!(entry.permissions, Some(0o644));
///
/// // Hostile input never panics and is reported as unparsable.
/// assert!(parse_list_line("total 12").is_none());
/// assert!(parse_list_line("this is not a listing").is_none());
/// ```
pub fn parse_list_line(line: &str) -> Option<RemoteEntry> {
    let line = line.trim_end_matches(['\r', '\n', ' ']);
    let text = line.trim_start();
    if text.is_empty() || text == "total" || text.starts_with("total ") {
        return None;
    }

    let shape = Shape::from_char(text.chars().next()?)?;

    // The nine permission characters, optionally followed by an ACL marker.
    let mut mode: u32 = 0;
    let mut cursor = 1;
    for index in 0..9 {
        let c = text[cursor..].chars().next()?;
        mode |= permission_bit(index, c)?;
        cursor += c.len_utf8();
    }
    if let Some(c) = text[cursor..].chars().next()
        && matches!(c, '+' | '.' | '@')
    {
        cursor += c.len_utf8();
    }

    let rest = text.get(cursor..)?;
    if !rest.starts_with(' ') {
        return None;
    }
    let fields = tokens(rest);
    if fields.len() < 5 {
        return None;
    }
    // A unix listing always starts the metadata with the hard-link count.
    if fields[0].text.is_empty() || !fields[0].text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let (month_index, month) = fields
        .iter()
        .enumerate()
        .find_map(|(index, field)| month_number(field.text).map(|month| (index, month)))?;
    if month_index == 0 || month_index + 3 >= fields.len() {
        return None;
    }
    let size = parse_list_size(
        fields[month_index - 1].text,
        fields.get(month_index.wrapping_sub(2)).map(|f| f.text),
        shape,
    )?;
    let day: u32 = fields[month_index + 1].text.parse().ok()?;
    let date = fields[month_index + 2].text;
    let name_field = &fields[month_index + 3];
    let raw_name = rest.get(name_field.start..)?.trim_end();
    // Only a symlink line carries a target; a regular file whose name happens
    // to contain " -> " keeps the whole name.
    let (name, target) = if shape.link {
        split_arrow(raw_name)
    } else {
        (raw_name, None)
    };
    if name.is_empty() {
        return None;
    }

    Some(RemoteEntry {
        name: name.to_string(),
        is_dir: shape.dir,
        is_symlink: shape.link,
        size,
        mtime: resolve_list_date(month, day, date, utc_now()),
        permissions: Some(mode),
        target: if shape.link { target } else { None },
    })
}

/// Split an `MLSD` line into its facts section and the file name.
///
/// RFC 3659 puts exactly one space between the facts and the name, but real
/// servers also emit `type=file; size=…; name`. The split point is therefore the
/// first space whose following field does not look like another fact, with the
/// last plausible boundary as a fallback.
fn split_mlsd_facts(line: &str) -> Option<(&str, &str)> {
    let mut fallback: Option<usize> = None;
    for (index, _) in line.match_indices(' ') {
        let head = line[..index].trim_end();
        if !head.ends_with(';') {
            continue;
        }
        let tail = &line[index + 1..];
        let next_field = tail.split(' ').next().unwrap_or_default();
        if next_field.contains('=') {
            fallback = Some(index);
            continue;
        }
        return Some((head, tail));
    }
    let index = fallback?;
    Some((line[..index].trim_end(), &line[index + 1..]))
}

/// Parse one line of an `MLSD` reply (or the data line of an `MLST` reply).
///
/// Facts that are understood: `type` (`file`, `dir`, `cdir`, `pdir`,
/// `OS.unix=slink`, `OS.unix=symlink`), `size`, `modify`, `unix.mode` (octal)
/// and `linktarget`/`unix.linktarget`. Everything else is ignored, so an
/// unknown fact never invalidates the line.
///
/// ```
/// use rc_remote::parse_mlsd_line;
///
/// let entry = parse_mlsd_line("type=file;size=1024;modify=20240112123456; file.bin").unwrap();
/// assert_eq!(entry.name, "file.bin");
/// assert_eq!(entry.size, 1024);
/// assert_eq!(entry.mtime, Some(1_705_062_896));
/// assert!(parse_mlsd_line("nonsense").is_none());
/// ```
pub fn parse_mlsd_line(line: &str) -> Option<RemoteEntry> {
    let line = line.trim_end_matches(['\r', '\n']);
    let text = line.trim_start_matches(' ');
    if text.is_empty() {
        return None;
    }
    let (facts, name) = split_mlsd_facts(text)?;
    let name = name.trim_end_matches(['\r', '\n']);
    if name.is_empty() || !facts.contains('=') {
        return None;
    }

    let mut kind: Option<String> = None;
    let mut size: u64 = 0;
    let mut mtime: Option<i64> = None;
    let mut permissions: Option<u32> = None;
    let mut target: Option<String> = None;

    for fact in facts.split(';') {
        let Some((key, value)) = fact.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "type" => kind = Some(value.to_ascii_lowercase()),
            "size" => size = value.parse().unwrap_or(0),
            "modify" | "create" => {
                if mtime.is_none() || key.trim().eq_ignore_ascii_case("modify") {
                    mtime = parse_mdtm_timestamp(value);
                }
            }
            "unix.mode" => {
                permissions = u32::from_str_radix(value.trim_start_matches('0'), 8)
                    .ok()
                    .or(Some(0))
            }
            "unix.linktarget" | "linktarget" => target = Some(value.to_string()),
            _ => {}
        }
    }

    let kind = kind?;
    let is_dir = matches!(kind.as_str(), "dir" | "cdir" | "pdir");
    let is_symlink = kind.ends_with("slink") || kind.ends_with("symlink");
    Some(RemoteEntry {
        name: name.to_string(),
        is_dir,
        is_symlink,
        size: if is_dir { 0 } else { size },
        mtime,
        permissions,
        target,
    })
}

/// Parse the `YYYYMMDDHHMMSS[.sss]` timestamp of `MDTM`/`MLSD`.
pub(crate) fn parse_mdtm_timestamp(text: &str) -> Option<i64> {
    // Servers answer `213 20240112123456`, but a reply may also echo the code
    // or add a comment, so take the first token that is a full timestamp.
    let token = text.split_whitespace().find(|token| {
        token
            .split('.')
            .next()
            .is_some_and(|main| main.len() == 14 && main.bytes().all(|b| b.is_ascii_digit()))
    })?;
    let main = token.split('.').next()?;
    if main.len() != 14 || !main.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i64 = main[0..4].parse().ok()?;
    let month: u32 = main[4..6].parse().ok()?;
    let day: u32 = main[6..8].parse().ok()?;
    let hour: u32 = main[8..10].parse().ok()?;
    let minute: u32 = main[10..12].parse().ok()?;
    let second: u32 = main[12..14].parse().ok()?;
    timestamp_from_utc(year, month, day, hour, minute, second)
}

/// Extract the directory from a `257 "<path>" …` reply, unescaping `""`.
pub(crate) fn parse_pwd(text: &str) -> Option<String> {
    let rest = &text[text.find('"')? + 1..];
    let mut out = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            if chars.peek() == Some(&'"') {
                chars.next();
                out.push('"');
            } else {
                return Some(out);
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// The last run of ASCII digits in a reply (`213 1234` → `1234`).
///
/// Servers usually answer `213 1234` with the code already stripped by the
/// reply parser, but some echo it, and `SIZE` replies may carry a trailing
/// comment — the last number is the size in all of those cases.
pub(crate) fn last_u64(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    let end = bytes.iter().rposition(|b| b.is_ascii_digit())? + 1;
    let start = bytes[..end]
        .iter()
        .rposition(|b| !b.is_ascii_digit())
        .map(|index| index + 1)
        .unwrap_or(0);
    text.get(start..end)?.parse().ok()
}

/// Last path segment of a remote path (tolerates trailing slashes).
pub(crate) fn base_name(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{civil_from_timestamp, timestamp_from_utc};

    fn list(line: &str) -> RemoteEntry {
        parse_list_line(line).unwrap_or_else(|| panic!("unparsable LIST line: {line:?}"))
    }

    #[test]
    fn list_parses_a_plain_file() {
        let e = list("-rw-r--r--   1 owner group   1234 Jan 12 12:34 file.txt");
        assert_eq!(e.name, "file.txt");
        assert!(!e.is_dir && !e.is_symlink);
        assert_eq!(e.size, 1234);
        assert_eq!(e.permissions, Some(0o644));
        let (year, month, day, hour, minute, _) = civil_from_timestamp(e.mtime.unwrap());
        assert_eq!((month, day, hour, minute), (1, 12, 12, 34));
        let (current, ..) = civil_from_timestamp(utc_now());
        assert!(
            year == current || year == current - 1,
            "year {year} not near {current}"
        );
    }

    #[test]
    fn list_parses_a_directory() {
        let e = list("drwxr-xr-x   2 owner group   4096 Jan 12  2024 public_html");
        assert!(e.is_dir && !e.is_symlink);
        assert_eq!(e.name, "public_html");
        assert_eq!(e.permissions, Some(0o755));
        assert_eq!(e.mtime, timestamp_from_utc(2024, 1, 12, 0, 0, 0));
    }

    #[test]
    fn list_parses_a_line_without_a_group_column() {
        let e = list("-rw-rw-r--   1 owner   98765 Feb  3  2023 archive.tar.gz");
        assert_eq!(e.name, "archive.tar.gz");
        assert_eq!(e.size, 98765);
        assert_eq!(e.permissions, Some(0o664));
        assert_eq!(e.mtime, timestamp_from_utc(2023, 2, 3, 0, 0, 0));
    }

    #[test]
    fn list_parses_a_symlink_and_its_target() {
        let e = list("lrwxrwxrwx   1 owner group      7 Jan 12  2024 current -> releases/v2");
        assert!(e.is_symlink && !e.is_dir);
        assert_eq!(e.name, "current");
        assert_eq!(e.size, 7);
        assert_eq!(e.target.as_deref(), Some("releases/v2"));
        assert_eq!(e.permissions, Some(0o777));
    }

    #[test]
    fn list_keeps_spaces_and_dots_in_names() {
        let e = list("-rw-r--r--   1 owner group   42 Jan  1  2024 a file with spaces.txt");
        assert_eq!(e.name, "a file with spaces.txt");
        let e = list("-rw-r--r--   1 owner group   42 Feb  3  2023 report (final) v2.tar.gz");
        assert_eq!(e.name, "report (final) v2.tar.gz");
        let e = list("-rw-r--r--   1 owner group   42 Feb  3  2023 .hidden rc");
        assert_eq!(e.name, ".hidden rc");
    }

    #[test]
    fn list_understands_every_month_name() {
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        for (index, name) in months.iter().enumerate() {
            let line = format!("-rw-r--r-- 1 owner group 7 {name} 15  2021 file");
            let e = list(&line);
            let (_, month, day, ..) = civil_from_timestamp(e.mtime.unwrap());
            assert_eq!(month as usize, index + 1, "{name}");
            assert_eq!(day, 15);
            // Fully spelled month names work too.
            let long = [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ];
            let line = format!("-rw-r--r-- 1 owner group 7 {} 15  2021 file", long[index]);
            assert!(parse_list_line(&line).is_some(), "{line}");
        }
    }

    #[test]
    fn list_resolves_the_year_of_a_time_only_date() {
        // 2024-06-01T00:00:00Z
        let now = timestamp_from_utc(2024, 6, 1, 0, 0, 0).unwrap();
        assert_eq!(
            resolve_list_date(1, 12, "12:34", now),
            timestamp_from_utc(2024, 1, 12, 12, 34, 0)
        );
        // A time in the past stays in the current year.
        assert_eq!(
            resolve_list_date(5, 31, "23:59", now),
            timestamp_from_utc(2024, 5, 31, 23, 59, 0)
        );
        // A time more than a day in the future must belong to the last year.
        assert_eq!(
            resolve_list_date(12, 31, "23:59", now),
            timestamp_from_utc(2023, 12, 31, 23, 59, 0)
        );
        // Seconds are accepted as well, and a year is taken literally.
        assert_eq!(
            resolve_list_date(1, 12, "12:34:56", now),
            timestamp_from_utc(2024, 1, 12, 12, 34, 56)
        );
        assert_eq!(
            resolve_list_date(1, 12, "2019", now),
            timestamp_from_utc(2019, 1, 12, 0, 0, 0)
        );
        // Nonsense dates are rejected instead of guessed.
        assert_eq!(resolve_list_date(1, 12, "not-a-date", now), None);
        assert_eq!(resolve_list_date(2, 30, "12:00", now), None);
        assert_eq!(resolve_list_date(1, 12, "25:00", now), None);
        assert_eq!(resolve_list_date(1, 12, "12:00:00:00", now), None);
    }

    #[test]
    fn list_carries_setuid_setgid_and_sticky_bits() {
        assert_eq!(
            list("-rwsr-xr-x 1 owner group 42 Jul  4  2019 suid").permissions,
            Some(0o4755)
        );
        assert_eq!(
            list("-rwxr-sr-x 1 owner group 42 Jul  4  2019 sgid").permissions,
            Some(0o2755)
        );
        assert_eq!(
            list("drwxrwx--T 1 owner group 42 Jul  4  2019 tmp").permissions,
            Some(0o1770)
        );
        assert_eq!(
            list("drwxrwxrwt 1 owner group 42 Jul  4  2019 shared").permissions,
            Some(0o1777)
        );
    }

    #[test]
    fn list_tolerates_acl_and_alternate_access_markers() {
        let e = list("-rw-r--r--+  1 owner group 42 Jan  1  2024 acl.txt");
        assert_eq!(e.name, "acl.txt");
        assert_eq!(e.size, 42);
        let e = list("drwxr-x---+  2 owner group 42 Jan  1  2024 acl_dir");
        assert!(e.is_dir);
        assert_eq!(e.name, "acl_dir");
    }

    #[test]
    fn list_handles_device_lines_with_major_minor_numbers() {
        let e = list("brw-rw---- 1 root disk 8, 1 Jan  1  2024 sda");
        assert_eq!(e.name, "sda");
        assert_eq!(e.size, 0);
        assert!(!e.is_dir);
        let e = list("crw-rw-rw- 1 root tty 5, 0 Jan  1  2024 tty0");
        assert_eq!(e.size, 0);
        assert_eq!(e.name, "tty0");
    }

    #[test]
    fn list_drops_a_dangling_arrow_without_a_target() {
        let e = list("lrwxrwxrwx 1 owner group 2 Jan  1  2024 broken ->");
        assert_eq!(e.name, "broken");
        assert_eq!(e.target, None);
    }

    #[test]
    fn list_rejects_headers_garbage_and_broken_fields() {
        let garbage = [
            "total 12",
            "total",
            "",
            "   ",
            "hello world, this is not a listing",
            "-rw-r--r--",
            "-rw-r--r-- 1",
            "-rw-r--r-- 1 owner group",
            "-rw-r--r-- 1 owner group 42 Jan 12",
            "-rw-r--r-- 1 owner group 42 Jan 12 2024",
            "?rw-r--r-- 1 owner group 42 Jan 12 2024 weirds",
            "-rw-r--r- 1 owner group 42 Jan 12 2024 short_perms",
            "-rw-r--r-- x owner group 42 Jan 12 2024 bad_nlink",
            "-rw-r--r-- 1 owner group -1 Jan 12 2024 negative",
            "-rw-r--r-- 1 owner group 99999999999999999999999 Jan 12 2024 overflow",
            "-rw-r--r-- 1 owner group 42 Foo 12 2024 not_a_month",
            "08-02-24  10:23AM               <DIR>          windows",
            "drwxr-xr-x 1 owner group 4096 Jan 12 2024",
        ];
        for line in garbage {
            assert!(
                parse_list_line(line).is_none(),
                "should be unparsable: {line:?}"
            );
        }
    }

    #[test]
    fn list_keeps_entries_whose_date_cannot_be_understood() {
        // The structure is fine but the date is nonsense: keep the entry, drop
        // the timestamp rather than hiding the file.
        let e = list("-rw-r--r-- 1 owner group 42 Jan 12 12:99:99 strange");
        assert_eq!(e.name, "strange");
        assert_eq!(e.mtime, None);
    }

    #[test]
    fn mlsd_parses_a_file() {
        let e = parse_mlsd_line("type=file;size=1024;modify=20240112123456;perm=adfr; file.bin")
            .unwrap();
        assert_eq!(e.name, "file.bin");
        assert!(!e.is_dir && !e.is_symlink);
        assert_eq!(e.size, 1024);
        assert_eq!(e.mtime, timestamp_from_utc(2024, 1, 12, 12, 34, 56));
        assert_eq!(e.permissions, None);
        assert_eq!(e.target, None);
    }

    #[test]
    fn mlsd_parses_a_directory_with_unix_mode() {
        let e = parse_mlsd_line("type=dir;modify=20240101000000;perm=elcmp;unix.mode=0755; pub")
            .unwrap();
        assert!(e.is_dir && !e.is_symlink);
        assert_eq!(e.name, "pub");
        assert_eq!(e.size, 0);
        assert_eq!(e.permissions, Some(0o755));
        assert_eq!(e.mtime, timestamp_from_utc(2024, 1, 1, 0, 0, 0));
    }

    #[test]
    fn mlsd_parses_a_symlink_fact() {
        let e = parse_mlsd_line(
            "type=OS.unix=slink;size=11;modify=20240112123456;unix.linktarget=/etc/passwd; current",
        )
        .unwrap();
        assert!(e.is_symlink && !e.is_dir);
        assert_eq!(e.name, "current");
        assert_eq!(e.target.as_deref(), Some("/etc/passwd"));
        let e = parse_mlsd_line("type=OS.unix=symlink;linktarget=../a b; link name").unwrap();
        assert!(e.is_symlink);
        assert_eq!(e.name, "link name");
        assert_eq!(e.target.as_deref(), Some("../a b"));
    }

    #[test]
    fn mlsd_copes_with_spaces_names_and_fractions() {
        let e = parse_mlsd_line("type=file;size=5;modify=20240112123456.123; name with spaces.txt")
            .unwrap();
        assert_eq!(e.name, "name with spaces.txt");
        assert_eq!(e.size, 5);
        assert_eq!(e.mtime, timestamp_from_utc(2024, 1, 12, 12, 34, 56));
        // Servers that put a space after every `;`.
        let e = parse_mlsd_line("type=file; size=7; modify=20240101000000; spaced.txt").unwrap();
        assert_eq!(e.name, "spaced.txt");
        assert_eq!(e.size, 7);
        // A name that itself looks like a fact.
        let e = parse_mlsd_line("type=file;size=1; report=v2.txt").unwrap();
        assert_eq!(e.name, "report=v2.txt");
    }

    #[test]
    fn mlsd_reports_dot_entries_and_unknown_facts() {
        let e = parse_mlsd_line("type=cdir;modify=20240101000000; .").unwrap();
        assert_eq!(e.name, ".");
        assert!(e.is_dir);
        let e = parse_mlsd_line("type=pdir;modify=20240101000000; ..").unwrap();
        assert_eq!(e.name, "..");
        let e = parse_mlsd_line("type=file;size=9;unique=1234;foo=bar;perm=adfr; x").unwrap();
        assert_eq!(e.name, "x");
        assert_eq!(e.size, 9);
    }

    #[test]
    fn mlsd_rejects_garbage() {
        for line in [
            "",
            "   ",
            "hello world",
            ";;;",
            "size=5; no_type",
            "type=file;size=5;",
            "type=file;;;",
            "type=file;size=abc; name_only",
        ] {
            // The last case is structurally valid (a name follows the facts) but
            // has no size, which must not panic either way.
            let parsed = parse_mlsd_line(line);
            if !line.ends_with("name_only") {
                assert!(parsed.is_none(), "should be unparsable: {line:?}");
            }
        }
        // A size that is not a number must not poison the whole entry.
        assert_eq!(
            parse_mlsd_line("type=file;size=abc; name_only").map(|e| e.size),
            Some(0)
        );
    }

    #[test]
    fn mdtm_timestamps_parse_in_both_forms() {
        assert_eq!(
            parse_mdtm_timestamp("20240112123456"),
            timestamp_from_utc(2024, 1, 12, 12, 34, 56)
        );
        assert_eq!(
            parse_mdtm_timestamp("213 20240112123456"),
            timestamp_from_utc(2024, 1, 12, 12, 34, 56)
        );
        assert_eq!(parse_mdtm_timestamp("19700101000000"), Some(0));
        assert_eq!(parse_mdtm_timestamp("20241301123456"), None);
        assert_eq!(parse_mdtm_timestamp("2024"), None);
        assert_eq!(parse_mdtm_timestamp("not a date"), None);
        assert_eq!(parse_mdtm_timestamp(""), None);
    }

    #[test]
    fn pwd_replies_lose_their_quotes() {
        assert_eq!(
            parse_pwd("\"/pub/incoming\" is the current directory"),
            Some("/pub/incoming".to_string())
        );
        assert_eq!(
            parse_pwd("\"a quoted name\" x"),
            Some("a quoted name".to_string())
        );
        assert_eq!(parse_pwd("\"/x\"\"y\" z"), Some("/x\"y".to_string()));
        assert_eq!(parse_pwd("no quotes here"), None);
        assert_eq!(
            parse_pwd("\"unterminated"),
            Some("unterminated".to_string())
        );
        assert_eq!(parse_pwd("\"\""), Some(String::new()));
    }

    #[test]
    fn numeric_and_path_helpers() {
        assert_eq!(last_u64("213 4242"), Some(4242));
        assert_eq!(last_u64("4242 bytes"), Some(4242));
        assert_eq!(last_u64("4242"), Some(4242));
        assert_eq!(last_u64("nothing"), None);
        assert_eq!(last_u64(""), None);
        assert_eq!(base_name("/pub/a.txt"), "a.txt");
        assert_eq!(base_name("/pub/dir/"), "dir");
        assert_eq!(base_name("/"), "");
        assert_eq!(base_name("name"), "name");
    }

    #[test]
    fn tokens_keep_offsets_and_skip_runs_of_spaces() {
        let toks = tokens("  1  owner   group 42");
        assert_eq!(
            toks.iter().map(|t| t.text).collect::<Vec<_>>(),
            vec!["1", "owner", "group", "42"]
        );
        assert_eq!(toks[0].start, 2);
        assert_eq!(toks[3].start, 19);
        assert!(tokens("").is_empty());
        assert!(tokens("   ").is_empty());
    }

    #[test]
    fn giant_and_nested_names_survive() {
        let long = "x".repeat(4096);
        let line = format!("-rw-r--r-- 1 owner group 1 Jan  1  2024 {long}");
        assert_eq!(list(&line).name, long);
        // A regular file keeps the whole tail as its name; targets are a
        // symlink concept.
        let e = list("-rw-r--r-- 1 owner group 1 Jan  1  2024 dir/sub/file -> a -> b");
        assert_eq!(e.name, "dir/sub/file -> a -> b");
        assert_eq!(e.target, None);
        let e = list("lrwxrwxrwx 1 owner group 1 Jan  1  2024 dir/sub/link -> a -> b");
        assert_eq!(e.name, "dir/sub/link");
        assert_eq!(e.target.as_deref(), Some("a -> b"));
    }
}
