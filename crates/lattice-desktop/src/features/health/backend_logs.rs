//! Recent lines of the backend's file log, for the Logs tab in Settings.
//!
//! The file layer in `infrastructure::observability::tracing` writes the
//! default `fmt` format with target, file and line, and no ANSI colours:
//!
//! ```text
//! 2026-10-14T09:15:02.123456Z  WARN turn{id=7}:retrieve: lattice::search: src/search.rs:42: message
//! ```
//!
//! Each line becomes timestamp, level, target and message. The `file:line`
//! location is dropped and span context stays in front of the message. A line
//! that does not match (the continuation of a multi-line message, say) comes
//! back whole with an unknown level.

use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::features::health::plugin::types::{BackendLogEntry, BackendLogLevel};
use crate::infrastructure::observability::tracing::{daily_log_files, log_directory};

/// Bytes read from the end of each file, so a large log costs a bounded read.
const TAIL_BYTES: u64 = 512 * 1024;
const DEFAULT_LIMIT: usize = 300;
const MAX_LIMIT: usize = 1000;

/// The newest backend log lines, oldest first. `limit` defaults to 300 and is
/// capped at 1000. No log directory or no log files gives an empty list.
pub(super) fn recent_backend_logs(limit: Option<u32>) -> Vec<BackendLogEntry> {
    read_recent(&log_directory(), limit)
}

fn read_recent(log_dir: &Path, limit: Option<u32>) -> Vec<BackendLogEntry> {
    let limit = limit.map_or(DEFAULT_LIMIT, |limit| {
        usize::try_from(limit).map_or(MAX_LIMIT, |limit| limit.min(MAX_LIMIT))
    });
    let mut lines = Vec::new();
    // The newest file, then the one before it when the newest is short.
    for path in daily_log_files(log_dir).iter().take(2) {
        if lines.len() >= limit {
            break;
        }
        let mut older = tail_lines(path, TAIL_BYTES);
        older.append(&mut lines);
        lines = older;
    }
    let skip = lines.len().saturating_sub(limit);
    lines
        .iter()
        .skip(skip)
        .map(|line| parse_line(line))
        .collect()
}

/// The complete lines in the last `max_bytes` of `path`. An unreadable file
/// has none.
fn tail_lines(path: &Path, max_bytes: u64) -> Vec<String> {
    read_tail(path, max_bytes).unwrap_or_else(|error| {
        tracing::debug!(%error, "could not read a backend log file");
        Vec::new()
    })
}

fn read_tail(path: &Path, max_bytes: u64) -> std::io::Result<Vec<String>> {
    let mut file = std::fs::File::open(path)?;
    let start = file.metadata()?.len().saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))?;
    // `take` because the appender may still be writing to the newest file.
    let mut bytes = Vec::new();
    file.take(max_bytes).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    // A read that starts mid-file starts mid-line: drop that fragment.
    let whole: &str = if start > 0 {
        text.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        &text
    };
    Ok(whole
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect())
}

fn parse_line(raw: &str) -> BackendLogEntry {
    let line = strip_ansi(raw);
    match parse_formatted(&line) {
        Some(entry) => entry,
        None => BackendLogEntry {
            timestamp: None,
            level: BackendLogLevel::Unknown,
            target: None,
            message: line.into_owned(),
        },
    }
}

/// `<timestamp> <level> [<spans> ]<target>: <file>:<line>: <message>`
fn parse_formatted(line: &str) -> Option<BackendLogEntry> {
    let (timestamp, rest) = line.split_once(' ')?;
    chrono::DateTime::parse_from_rfc3339(timestamp).ok()?;
    // INFO and WARN are padded to the width of the five-letter levels.
    let (level, rest) = rest.trim_start().split_once(' ')?;
    let level = match level {
        "TRACE" => BackendLogLevel::Trace,
        "DEBUG" => BackendLogLevel::Debug,
        "INFO" => BackendLogLevel::Info,
        "WARN" => BackendLogLevel::Warn,
        "ERROR" => BackendLogLevel::Error,
        _ => return None,
    };
    let (target, message) = match split_location(rest) {
        Some(("", target, message)) => (Some(target), message.to_owned()),
        Some((spans, target, message)) => (Some(target), format!("{spans} {message}")),
        // An event recorded without a file and line has nothing to anchor the
        // target on, so the rest of the line is the message.
        None => (None, rest.to_owned()),
    };
    Some(BackendLogEntry {
        timestamp: Some(timestamp.to_owned()),
        level,
        target: target.map(str::to_owned),
        message,
    })
}

/// Splits `[<spans> ]<target>: <file>:<line>: <message>` into spans, target
/// and message. The target is the token before the first `: ` that a
/// `file:line: ` location follows, so `: ` inside span fields is skipped.
fn split_location(rest: &str) -> Option<(&str, &str, &str)> {
    rest.match_indices(": ").find_map(|(at, _)| {
        let head = rest.get(..at)?;
        let (location, message) = rest.get(at + 2..)?.split_once(": ")?;
        let (file, line) = location.rsplit_once(':')?;
        if file.is_empty() || line.is_empty() || !line.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (spans, target) = head.rsplit_once(' ').unwrap_or(("", head));
        (!target.is_empty()).then_some((spans, target, message))
    })
}

/// Drops ANSI colour sequences. The file layer writes none itself, but span
/// fields are formatted once and shared with the coloured stdout layer.
fn strip_ansi(line: &str) -> Cow<'_, str> {
    if !line.contains('\x1b') {
        return Cow::Borrowed(line);
    }
    let mut plain = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            plain.push(c);
        } else if chars.next_if_eq(&'[').is_some() {
            // A CSI sequence ends at its first byte in `@..=~`.
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        }
    }
    Cow::Owned(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_lines(path: &Path, lines: impl IntoIterator<Item = String>) {
        let text: String = lines.into_iter().map(|line| line + "\n").collect();
        std::fs::write(path, text).unwrap();
    }

    fn messages(entries: &[BackendLogEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.message.as_str()).collect()
    }

    #[test]
    fn a_short_newest_file_is_topped_up_from_the_one_before() {
        let dir = tempfile::tempdir().unwrap();
        write_lines(
            &dir.path().join("lattice.log.2026-10-13"),
            (1..=4).map(|n| format!("old {n}")),
        );
        write_lines(
            &dir.path().join("lattice.log.2026-10-14"),
            (1..=2).map(|n| format!("new {n}")),
        );
        write_lines(&dir.path().join("notes.txt"), ["not a log".to_string()]);

        let entries = read_recent(dir.path(), Some(4));
        assert_eq!(messages(&entries), ["old 3", "old 4", "new 1", "new 2"]);

        let entries = read_recent(dir.path(), Some(2));
        assert_eq!(messages(&entries), ["new 1", "new 2"]);
    }

    #[test]
    fn the_line_limit_defaults_to_300_and_is_capped_at_1000() {
        let dir = tempfile::tempdir().unwrap();
        write_lines(
            &dir.path().join("lattice.log.2026-10-14"),
            (0..1500).map(|n| format!("line {n}")),
        );

        assert_eq!(read_recent(dir.path(), None).len(), 300);
        let capped = read_recent(dir.path(), Some(5000));
        assert_eq!(capped.len(), 1000);
        assert_eq!(capped.last().map(|e| e.message.as_str()), Some("line 1499"));
    }

    #[test]
    fn the_size_cap_reads_only_whole_lines_from_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lattice.log.2026-10-14");
        write_lines(&path, (0..1000).map(|n| format!("line {n:04}")));

        // Each line is ten bytes, so 95 bytes cut the tenth line from the end.
        let lines = tail_lines(&path, 95);
        assert_eq!(lines.len(), 9);
        assert_eq!(lines.first().map(String::as_str), Some("line 0991"));
        assert_eq!(lines.last().map(String::as_str), Some("line 0999"));
    }

    #[test]
    fn no_log_directory_reads_as_no_lines() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_recent(&dir.path().join("missing"), None).is_empty());
    }

    #[test]
    fn a_formatted_line_parses_into_its_parts() {
        let line = "2026-10-14T09:15:02.123456Z  WARN turn{\x1b[3mid\x1b[0m=7}:retrieve: \
                    lattice::search: src/search.rs:42: no results: falling back";
        assert_eq!(
            parse_line(line),
            BackendLogEntry {
                timestamp: Some("2026-10-14T09:15:02.123456Z".into()),
                level: BackendLogLevel::Warn,
                target: Some("lattice::search".into()),
                message: "turn{id=7}:retrieve: no results: falling back".into(),
            }
        );
    }

    #[test]
    fn a_malformed_line_is_kept_whole_with_an_unknown_level() {
        let line = "    0: lattice::search::run at src/search.rs:42";
        assert_eq!(
            parse_line(line),
            BackendLogEntry {
                timestamp: None,
                level: BackendLogLevel::Unknown,
                target: None,
                message: line.into(),
            }
        );
    }
}
