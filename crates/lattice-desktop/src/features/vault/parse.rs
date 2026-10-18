//! Reverse of `writeback::build_frontmatter`. Hand-rolled against the
//! exact format we emit:
//! ```text
//! ---
//! id: <bare>
//! title: "<escaped>"
//! created_at: <bare>
//! updated_at: <bare>
//! tags: [] | ["a", "b"]
//! ---
//!
//! <body>
//! ```
//! Escapes inside double-quoted scalars: `\\`, `\"`, `\n`. Anything
//! else is rejected rather than silently lost.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNote {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub tags: Vec<String>,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// No frontmatter envelope — caller should treat as an untracked
    /// external file and skip.
    #[error("file does not look like a Lattice note (no frontmatter envelope)")]
    NotOurs,

    #[error("frontmatter is missing required field: {0}")]
    MissingField(&'static str),

    #[error("frontmatter contains an unsupported escape sequence: {0:?}")]
    UnsupportedEscape(String),

    #[error("frontmatter tags array is malformed: {0}")]
    MalformedTags(String),
}

pub fn parse_note(contents: &str) -> Result<ParsedNote, ParseError> {
    let (frontmatter, body) = split_frontmatter(contents).ok_or(ParseError::NotOurs)?;

    let mut id: Option<String> = None;
    let mut title: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();

    for raw_line in frontmatter.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim_start();
        match key {
            "id" => id = Some(value.to_string()),
            "title" => title = Some(parse_double_quoted(value)?),
            "created_at" => created_at = Some(value.to_string()),
            "updated_at" => updated_at = Some(value.to_string()),
            "tags" => tags = parse_tags_array(value)?,
            _ => {} // unknown keys forward-compatible — ignore
        }
    }

    Ok(ParsedNote {
        id: id.ok_or(ParseError::MissingField("id"))?,
        title: title.ok_or(ParseError::MissingField("title"))?,
        created_at: created_at.ok_or(ParseError::MissingField("created_at"))?,
        updated_at: updated_at.ok_or(ParseError::MissingField("updated_at"))?,
        tags,
        body: body.to_string(),
    })
}

fn split_frontmatter(contents: &str) -> Option<(&str, &str)> {
    let trimmed = contents
        .strip_prefix("---\n")
        .or_else(|| contents.strip_prefix("---\r\n"))?;
    let close_unix = trimmed.find("\n---\n");
    let close_windows = trimmed.find("\n---\r\n");
    let close = match (close_unix, close_windows) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }?;
    let frontmatter = &trimmed[..close];
    let after = &trimmed[close..];
    let body_start_in_after = if after.starts_with("\n---\r\n") { 6 } else { 5 };
    let body_with_leading_blank = &after[body_start_in_after..];
    // Writer emits one blank line between fence and body; strip it.
    let body = body_with_leading_blank
        .strip_prefix('\n')
        .unwrap_or(body_with_leading_blank);
    Some((frontmatter, body))
}

fn parse_double_quoted(raw: &str) -> Result<String, ParseError> {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| ParseError::UnsupportedEscape(raw.to_string()))?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some(other) => {
                return Err(ParseError::UnsupportedEscape(format!("\\{}", other)));
            }
            None => return Err(ParseError::UnsupportedEscape("\\<eof>".to_string())),
        }
    }
    Ok(out)
}

fn parse_tags_array(raw: &str) -> Result<Vec<String>, ParseError> {
    let trimmed = raw.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| ParseError::MalformedTags(trimmed.to_string()))?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    // Manual comma-split that honors `\"` inside quoted strings.
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for c in inner.chars() {
        if escaped {
            current.push('\\');
            current.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' && in_quotes {
            escaped = true;
            continue;
        }
        if c == '"' {
            in_quotes = !in_quotes;
            current.push(c);
            continue;
        }
        if c == ',' && !in_quotes {
            parts.push(std::mem::take(&mut current));
            continue;
        }
        current.push(c);
    }
    parts.push(current);

    parts
        .into_iter()
        .map(|p| parse_double_quoted(p.trim()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ParseError::MalformedTags(format!("{}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doc(body: &str) -> String {
        format!(
            "---\nid: abc-123\ntitle: \"Hello world\"\ncreated_at: 2026-05-01T18:00:00Z\nupdated_at: 2026-05-01T18:05:00Z\ntags: [\"foo\", \"bar\"]\n---\n\n{}",
            body
        )
    }

    #[test]
    fn parses_basic_note_round_trip_friendly() {
        let parsed = parse_note(&sample_doc("Body line one.\nBody line two.")).unwrap();
        assert_eq!(parsed.id, "abc-123");
        assert_eq!(parsed.title, "Hello world");
        assert_eq!(parsed.created_at, "2026-05-01T18:00:00Z");
        assert_eq!(parsed.tags, vec!["foo".to_string(), "bar".to_string()]);
        assert_eq!(parsed.body, "Body line one.\nBody line two.");
    }

    #[test]
    fn empty_body_parses_clean() {
        let parsed = parse_note(&sample_doc("")).unwrap();
        assert_eq!(parsed.body, "");
    }

    #[test]
    fn quoted_escape_in_title_round_trips() {
        let raw = "---\nid: id1\ntitle: \"She said \\\"hi\\\"\"\ncreated_at: t\nupdated_at: t\ntags: []\n---\n\nbody";
        let parsed = parse_note(raw).unwrap();
        assert_eq!(parsed.title, "She said \"hi\"");
    }

    #[test]
    fn newline_escape_in_title_round_trips() {
        let raw = "---\nid: id1\ntitle: \"Line one\\nLine two\"\ncreated_at: t\nupdated_at: t\ntags: []\n---\n\nbody";
        let parsed = parse_note(raw).unwrap();
        assert_eq!(parsed.title, "Line one\nLine two");
    }

    #[test]
    fn empty_tags_array_parses_to_empty_vec() {
        let raw = "---\nid: id1\ntitle: \"t\"\ncreated_at: t\nupdated_at: t\ntags: []\n---\n\nb";
        let parsed = parse_note(raw).unwrap();
        assert!(parsed.tags.is_empty());
    }

    #[test]
    fn tags_with_quoted_comma_parse_correctly() {
        let raw = "---\nid: id1\ntitle: \"t\"\ncreated_at: t\nupdated_at: t\ntags: [\"hello, world\", \"foo\"]\n---\n\nb";
        let parsed = parse_note(raw).unwrap();
        assert_eq!(
            parsed.tags,
            vec!["hello, world".to_string(), "foo".to_string()]
        );
    }

    #[test]
    fn missing_frontmatter_envelope_is_not_ours() {
        let err = parse_note("just some markdown\n").unwrap_err();
        assert!(matches!(err, ParseError::NotOurs));
    }

    #[test]
    fn missing_required_field_errors_explicitly() {
        let raw = "---\nid: id1\ntitle: \"t\"\ncreated_at: t\ntags: []\n---\n\nb";
        let err = parse_note(raw).unwrap_err();
        match err {
            ParseError::MissingField(f) => assert_eq!(f, "updated_at"),
            other => panic!("expected MissingField, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_escape_rejected() {
        let raw = "---\nid: id1\ntitle: \"hi \\t there\"\ncreated_at: t\nupdated_at: t\ntags: []\n---\n\nb";
        let err = parse_note(raw).unwrap_err();
        assert!(matches!(err, ParseError::UnsupportedEscape(_)));
    }

    #[test]
    fn windows_line_endings_in_frontmatter_handled() {
        let raw = "---\r\nid: id1\r\ntitle: \"t\"\r\ncreated_at: t\r\nupdated_at: t\r\ntags: []\r\n---\r\n\r\nbody\r\nline two";
        let parsed = parse_note(raw).unwrap();
        assert_eq!(parsed.id, "id1");
        // Body content is preserved verbatim including its line endings.
        assert!(parsed.body.contains("body"));
    }

    #[test]
    fn unknown_field_silently_ignored() {
        let raw = "---\nid: id1\ntitle: \"t\"\ncreated_at: t\nupdated_at: t\ntags: []\nfuture_field: whatever\n---\n\nb";
        let parsed = parse_note(raw).unwrap();
        assert_eq!(parsed.id, "id1");
    }
}
