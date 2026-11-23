//! Reply parsing and server-code classification.

use crate::error::RemoteError;

/// How many continuation lines a multi-line reply may have.
const MAX_REPLY_LINES: usize = 256;

/// One complete FTP reply (the first line plus any continuation lines).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reply {
    /// The three digit server code.
    pub(crate) code: u16,
    /// The human readable text, with continuation lines joined by newlines and
    /// the code and separator stripped from the first line.
    pub(crate) text: String,
}

impl Reply {
    /// True for a `2xx` completion reply.
    pub(crate) fn is_ok(&self) -> bool {
        (200..300).contains(&self.code)
    }

    /// True for `125`/`150`, i.e. "the data connection is about to be opened".
    pub(crate) fn is_preliminary(&self) -> bool {
        self.code == 125 || self.code == 150
    }

    /// `code text`, for logs and error messages.
    pub(crate) fn detail(&self) -> String {
        let text = self.text.trim();
        if text.is_empty() {
            format!("{}", self.code)
        } else {
            format!("{} {text}", self.code)
        }
    }
}

/// Parse the first line of a reply into `(code, separator, text)`.
pub(crate) fn parse_first_line(line: &str) -> Result<(u16, u8, String), RemoteError> {
    let bytes = line.as_bytes();
    if bytes.len() < 3 || !bytes[..3].iter().all(u8::is_ascii_digit) {
        return Err(RemoteError::Protocol(format!(
            "unexpected reply line {line:?}"
        )));
    }
    let code: u16 = line[..3]
        .parse()
        .map_err(|_| RemoteError::Protocol(format!("unexpected reply code in {line:?}")))?;
    let separator = bytes.get(3).copied().unwrap_or(b' ');
    let text = line.get(4..).unwrap_or("").trim_start().to_string();
    Ok((code, separator, text))
}

/// True when `line` closes the multi-line reply opened by `code`.
///
/// Compares bytes, never string slices: a hostile line may start with a
/// multi-byte character and slicing it at byte 3 would panic.
pub(crate) fn closes_reply(line: &str, code: u16) -> bool {
    let expected = format!("{code:03}");
    let expected = expected.as_bytes();
    let bytes = line.as_bytes();
    match bytes.len() {
        0..=2 => false,
        3 => bytes == expected,
        _ => &bytes[..3] == expected && bytes[3] == b' ',
    }
}

/// Join continuation lines into the reply body, guarding the line count.
pub(crate) fn join_lines(first: String, continuation: &[String]) -> Result<String, RemoteError> {
    if continuation.len() > MAX_REPLY_LINES {
        return Err(RemoteError::Protocol(
            "the server sent a reply with too many lines".to_string(),
        ));
    }
    let mut text = first;
    for line in continuation {
        text.push('\n');
        text.push_str(line);
    }
    Ok(text)
}

/// Does a `55x` message describe a permission problem rather than a missing path?
fn looks_like_permission_denial(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "denied",
        "permission",
        "not permitted",
        "not allowed",
        "access is prevented",
        "no access",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// Map a server reply onto the error variant the GUI shows.
pub(crate) fn error(reply: &Reply) -> RemoteError {
    let detail = reply.detail();
    match reply.code {
        421 => RemoteError::Protocol(format!(
            "the server closed the control connection: {detail}"
        )),
        425 | 426 => RemoteError::Protocol(format!(
            "the data connection was refused or aborted: {detail}"
        )),
        450..=452 => RemoteError::Protocol(format!("temporary server failure: {detail}")),
        530 | 532 => RemoteError::Authentication(detail),
        500 | 501 | 502 | 504 | 202 => RemoteError::Unsupported(detail),
        550 | 551 => {
            if looks_like_permission_denial(&reply.text) {
                RemoteError::PermissionDenied(detail)
            } else {
                RemoteError::NotFound(detail)
            }
        }
        553 => RemoteError::PermissionDenied(detail),
        _ => RemoteError::Protocol(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(code: u16, text: &str) -> Reply {
        Reply {
            code,
            text: text.to_string(),
        }
    }

    #[test]
    fn codes_are_classified() {
        assert!(reply(220, "ready").is_ok());
        assert!(reply(257, "created").is_ok());
        assert!(!reply(550, "nope").is_ok());
        assert!(reply(150, "").is_preliminary());
        assert!(!reply(200, "").is_preliminary());
        assert_eq!(reply(213, " 12 ").detail(), "213 12");
        assert_eq!(reply(213, "   ").detail(), "213");
    }

    #[test]
    fn first_lines_are_parsed_and_validated() {
        assert_eq!(
            parse_first_line("220 hello").unwrap(),
            (220, b' ', "hello".to_string())
        );
        assert_eq!(
            parse_first_line("250-Listing").unwrap(),
            (250, b'-', "Listing".to_string())
        );
        assert_eq!(parse_first_line("213").unwrap(), (213, b' ', String::new()));
        assert!(parse_first_line("oops").is_err());
        assert!(parse_first_line("").is_err());
        assert!(parse_first_line("2 0 hello").is_err());
        assert!(parse_first_line("２２０ unicode digits").is_err());
    }

    #[test]
    fn continuations_close_on_the_matching_code() {
        assert!(closes_reply("250 Done", 250));
        assert!(closes_reply("250", 250));
        assert!(!closes_reply("250-Done", 250));
        assert!(!closes_reply("251 Done", 250));
        assert!(!closes_reply("25", 250));
    }

    #[test]
    fn reply_lines_are_bounded() {
        let first = "Listing".to_string();
        assert_eq!(join_lines(first.clone(), &[]).unwrap(), "Listing");
        assert_eq!(
            join_lines(first.clone(), &[" a".into(), " b".into()]).unwrap(),
            "Listing\n a\n b"
        );
        let many = vec!["x".to_string(); MAX_REPLY_LINES + 1];
        assert!(join_lines(first, &many).is_err());
    }

    #[test]
    fn server_codes_map_to_the_right_error_variant() {
        assert!(matches!(
            error(&reply(530, "Login incorrect")),
            RemoteError::Authentication(_)
        ));
        assert!(matches!(
            error(&reply(532, "need account")),
            RemoteError::Authentication(_)
        ));
        assert!(matches!(
            error(&reply(500, "Unknown command")),
            RemoteError::Unsupported(_)
        ));
        assert!(matches!(
            error(&reply(502, "Not implemented")),
            RemoteError::Unsupported(_)
        ));
        assert!(matches!(
            error(&reply(550, "No such file or directory")),
            RemoteError::NotFound(_)
        ));
        assert!(matches!(
            error(&reply(550, "Permission denied")),
            RemoteError::PermissionDenied(_)
        ));
        assert!(matches!(
            error(&reply(551, "Page type unknown")),
            RemoteError::NotFound(_)
        ));
        assert!(matches!(
            error(&reply(553, "File name not allowed")),
            RemoteError::PermissionDenied(_)
        ));
        assert!(matches!(
            error(&reply(421, "Timeout")),
            RemoteError::Protocol(_)
        ));
        assert!(matches!(
            error(&reply(426, "Aborted")),
            RemoteError::Protocol(_)
        ));
        assert!(matches!(
            error(&reply(450, "Busy")),
            RemoteError::Protocol(_)
        ));
        assert!(matches!(
            error(&reply(999, "Weird")),
            RemoteError::Protocol(_)
        ));
    }
}
