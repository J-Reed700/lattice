//! Parses and preserves YAML frontmatter for mirrored vault markdown files.
//!
//! `writeback::build_frontmatter` emits a canonical subset of YAML, while
//! external editors (Obsidian, VS Code, Notepad, etc.) may add a UTF-8 BOM,
//! use CRLF line endings, write unquoted or single-quoted scalars, use block
//! lists (`- tag`) or unquoted flow lists for `tags`, or add custom properties
//! (`aliases`, `cssclasses`, etc.). All of those must parse and round-trip cleanly.

use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNote {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub tags: Vec<String>,
    /// Unrecognized frontmatter blocks (key line + any indented/sequence
    /// continuation lines) preserved verbatim so write-back keeps external
    /// properties intact.
    pub extra_frontmatter: Vec<String>,
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
    let (frontmatter, body_raw) = split_frontmatter(contents).ok_or(ParseError::NotOurs)?;

    let mut id: Option<String> = None;
    let mut title: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut extra_frontmatter: Vec<String> = Vec::new();

    let lines: Vec<&str> = frontmatter
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .collect();
    let mut idx = 0usize;
    while let Some(&line) = lines.get(idx) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            idx += 1;
            continue;
        }
        if line.starts_with([' ', '\t']) || is_sequence_item_line(trimmed) {
            idx += 1;
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once(':') else {
            idx += 1;
            continue;
        };
        let key = unquote_key(raw_key.trim());
        let inline_raw = raw_value.trim_start();
        let end = collect_block_end(&lines, idx + 1, inline_raw);
        let continuations = lines.get(idx + 1..end).unwrap_or(&[]);

        match key {
            "id" => {
                let parsed = parse_yaml_scalar(inline_raw)?;
                if !parsed.is_empty() {
                    id = Some(parsed);
                }
            }
            "title" => {
                title = Some(parse_yaml_scalar(inline_raw)?);
            }
            "created_at" => {
                let parsed = parse_yaml_scalar(inline_raw)?;
                if !parsed.is_empty() {
                    created_at = Some(parsed);
                }
            }
            "updated_at" => {
                let parsed = parse_yaml_scalar(inline_raw)?;
                if !parsed.is_empty() {
                    updated_at = Some(parsed);
                }
            }
            "tags" => {
                tags = parse_tags(inline_raw, continuations)?;
            }
            _ => {
                extra_frontmatter.push(line.to_string());
                for &cont in continuations {
                    extra_frontmatter.push(cont.to_string());
                }
            }
        }

        idx = end;
    }

    let body = normalize_line_endings(body_raw).into_owned();

    Ok(ParsedNote {
        id: id.ok_or(ParseError::MissingField("id"))?,
        title: title.ok_or(ParseError::MissingField("title"))?,
        created_at: created_at.ok_or(ParseError::MissingField("created_at"))?,
        updated_at: updated_at.ok_or(ParseError::MissingField("updated_at"))?,
        tags,
        extra_frontmatter,
        body,
    })
}

pub(super) fn normalize_line_endings(s: &str) -> Cow<'_, str> {
    if s.contains("\r\n") {
        Cow::Owned(s.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(s)
    }
}

fn split_frontmatter(contents: &str) -> Option<(&str, &str)> {
    let without_bom = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let (first_line_raw, after_open) = without_bom.split_once('\n')?;
    let first_line = first_line_raw.strip_suffix('\r').unwrap_or(first_line_raw);
    if first_line.trim_end_matches([' ', '\t']) != "---" {
        return None;
    }

    let mut rest = after_open;
    let mut frontmatter_len = 0usize;
    while !rest.is_empty() {
        let (line_raw, next_rest, line_bytes) = match rest.split_once('\n') {
            Some((line, after)) => (line, after, line.len() + 1),
            None => (rest, "", rest.len()),
        };
        let line = line_raw.strip_suffix('\r').unwrap_or(line_raw);
        if line.trim_end_matches([' ', '\t']) == "---" {
            let frontmatter = after_open.get(..frontmatter_len)?;
            // Strip at most the single blank line emitted between fence and body.
            let body = next_rest
                .strip_prefix("\r\n")
                .or_else(|| next_rest.strip_prefix('\n'))
                .unwrap_or(next_rest);
            return Some((frontmatter, body));
        }
        frontmatter_len += line_bytes;
        rest = next_rest;
    }
    None
}

fn unquote_key(key: &str) -> &str {
    key.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| key.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(key)
}

fn is_sequence_item_line(trimmed: &str) -> bool {
    trimmed == "-" || trimmed.starts_with("- ") || trimmed.starts_with("-\t")
}

fn collect_block_end(lines: &[&str], start_after: usize, inline_raw: &str) -> usize {
    let trimmed_inline = inline_raw.trim_start();
    if trimmed_inline.starts_with('[') && flow_array_inner(trimmed_inline).is_err() {
        let mut combined = String::from(trimmed_inline);
        let mut j = start_after;
        while let Some(&next) = lines.get(j) {
            combined.push('\n');
            combined.push_str(next);
            j += 1;
            if flow_array_inner(&combined).is_ok() {
                return j;
            }
        }
        return start_after;
    }

    let inline_empty = strip_inline_comment(inline_raw).trim().is_empty();
    let mut end = start_after;
    let mut j = start_after;
    while let Some(&next) = lines.get(j) {
        let trimmed = next.trim();
        if trimmed.is_empty() {
            j += 1;
            continue;
        }
        if next.starts_with([' ', '\t']) || (inline_empty && is_sequence_item_line(trimmed)) {
            j += 1;
            end = j;
        } else {
            break;
        }
    }
    end
}

fn strip_inline_comment(raw: &str) -> &str {
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    let mut prev_ws = true;
    let mut chars = raw.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        if in_double {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_double = false;
            }
            prev_ws = false;
            continue;
        }
        if in_single {
            if c == '\'' {
                if chars.peek().is_some_and(|&(_, next_c)| next_c == '\'') {
                    chars.next();
                } else {
                    in_single = false;
                }
            }
            prev_ws = false;
            continue;
        }
        if c == '"' {
            in_double = true;
            prev_ws = false;
        } else if c == '\'' {
            in_single = true;
            prev_ws = false;
        } else if c == '#' && prev_ws {
            return raw.get(..idx).unwrap_or("");
        } else {
            prev_ws = c == ' ' || c == '\t';
        }
    }
    raw
}

fn parse_yaml_scalar(raw: &str) -> Result<String, ParseError> {
    let trimmed = raw.trim();
    if trimmed.starts_with('"') {
        parse_double_quoted_scalar(trimmed)
    } else if trimmed.starts_with('\'') {
        parse_single_quoted_scalar(trimmed)
    } else {
        Ok(strip_inline_comment(trimmed).trim().to_string())
    }
}

fn parse_double_quoted_scalar(raw: &str) -> Result<String, ParseError> {
    let Some(after_open) = raw.strip_prefix('"') else {
        return Err(ParseError::UnsupportedEscape(raw.to_string()));
    };
    let mut out = String::with_capacity(after_open.len());
    let mut chars = after_open.chars();
    let mut closed = false;
    while let Some(c) = chars.next() {
        if c == '"' {
            closed = true;
            break;
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('/') => out.push('/'),
            Some('0') => out.push('\0'),
            Some(' ') => out.push(' '),
            Some(other) => {
                return Err(ParseError::UnsupportedEscape(format!("\\{}", other)));
            }
            None => return Err(ParseError::UnsupportedEscape("\\<eof>".to_string())),
        }
    }
    if !closed {
        return Err(ParseError::UnsupportedEscape(raw.to_string()));
    }
    let tail = chars.as_str().trim_start();
    if !tail.is_empty() && !tail.starts_with('#') {
        return Err(ParseError::UnsupportedEscape(raw.to_string()));
    }
    Ok(out)
}

fn parse_single_quoted_scalar(raw: &str) -> Result<String, ParseError> {
    let Some(after_open) = raw.strip_prefix('\'') else {
        return Err(ParseError::UnsupportedEscape(raw.to_string()));
    };
    let mut out = String::with_capacity(after_open.len());
    let mut chars = after_open.chars().peekable();
    let mut closed = false;
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.peek() == Some(&'\'') {
                chars.next();
                out.push('\'');
                continue;
            }
            closed = true;
            break;
        }
        out.push(c);
    }
    if !closed {
        return Err(ParseError::UnsupportedEscape(raw.to_string()));
    }
    let tail: String = chars.collect();
    let tail = tail.trim_start();
    if !tail.is_empty() && !tail.starts_with('#') {
        return Err(ParseError::UnsupportedEscape(raw.to_string()));
    }
    Ok(out)
}

fn parse_tags(inline_raw: &str, continuations: &[&str]) -> Result<Vec<String>, ParseError> {
    let inline = strip_inline_comment(inline_raw).trim();
    if inline.starts_with('[') {
        let combined = if continuations.is_empty() {
            Cow::Borrowed(inline_raw)
        } else {
            let mut joined = String::from(inline_raw);
            for &cont in continuations {
                joined.push('\n');
                joined.push_str(cont);
            }
            Cow::Owned(joined)
        };
        return parse_flow_tags(&combined);
    }
    if inline.is_empty() || inline == "null" || inline == "~" {
        return parse_block_tags(continuations);
    }
    if inline.starts_with('{') || !continuations.is_empty() {
        return Err(ParseError::MalformedTags(inline.to_string()));
    }
    let single = parse_yaml_scalar(inline).map_err(|e| ParseError::MalformedTags(e.to_string()))?;
    if single.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(vec![single])
    }
}

fn flow_array_inner(raw: &str) -> Result<&str, ParseError> {
    let trimmed = raw.trim();
    let Some(after_open) = trimmed.strip_prefix('[') else {
        return Err(ParseError::MalformedTags(trimmed.to_string()));
    };
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    let mut chars = after_open.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        if in_double {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_double = false;
            }
            continue;
        }
        if in_single {
            if c == '\'' {
                if chars.peek().is_some_and(|&(_, next_c)| next_c == '\'') {
                    chars.next();
                } else {
                    in_single = false;
                }
            }
            continue;
        }
        if c == '"' {
            in_double = true;
        } else if c == '\'' {
            in_single = true;
        } else if c == ']' {
            let tail = after_open.get(idx + 1..).unwrap_or("").trim_start();
            if !tail.is_empty() && !tail.starts_with('#') {
                return Err(ParseError::MalformedTags(trimmed.to_string()));
            }
            return Ok(after_open.get(..idx).unwrap_or(""));
        }
    }
    Err(ParseError::MalformedTags(trimmed.to_string()))
}

fn parse_flow_tags(raw: &str) -> Result<Vec<String>, ParseError> {
    let inner = flow_array_inner(raw)?.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }

    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if in_double {
            if escaped {
                current.push('\\');
                current.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else {
                if c == '"' {
                    in_double = false;
                }
                current.push(c);
            }
            continue;
        }
        if in_single {
            current.push(c);
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    if let Some(next_c) = chars.next() {
                        current.push(next_c);
                    }
                } else {
                    in_single = false;
                }
            }
            continue;
        }
        if c == '"' {
            in_double = true;
            current.push(c);
        } else if c == '\'' {
            in_single = true;
            current.push(c);
        } else if c == ',' {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);

    let last_idx = parts.len().saturating_sub(1);
    let mut tags = Vec::with_capacity(parts.len());
    for (idx, part) in parts.into_iter().enumerate() {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            if idx == last_idx {
                continue;
            }
            return Err(ParseError::MalformedTags(raw.trim().to_string()));
        }
        let parsed =
            parse_yaml_scalar(trimmed).map_err(|e| ParseError::MalformedTags(e.to_string()))?;
        tags.push(parsed);
    }
    Ok(tags)
}

fn parse_block_tags(continuations: &[&str]) -> Result<Vec<String>, ParseError> {
    let mut tags = Vec::new();
    for &line in continuations {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let item_raw = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("-\t"))
            .ok_or_else(|| ParseError::MalformedTags(trimmed.to_string()))?;
        let item_trimmed = item_raw.trim();
        if item_trimmed.is_empty() {
            return Err(ParseError::MalformedTags(trimmed.to_string()));
        }
        let parsed = parse_yaml_scalar(item_trimmed)
            .map_err(|e| ParseError::MalformedTags(e.to_string()))?;
        tags.push(parsed);
    }
    Ok(tags)
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
        assert!(parsed.extra_frontmatter.is_empty());
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
        let raw = "---\nid: id1\ntitle: \"hi \\q there\"\ncreated_at: t\nupdated_at: t\ntags: []\n---\n\nb";
        let err = parse_note(raw).unwrap_err();
        assert!(matches!(err, ParseError::UnsupportedEscape(_)));
    }

    #[test]
    fn windows_line_endings_in_frontmatter_and_body_handled() {
        let raw = "---\r\nid: id1\r\ntitle: \"t\"\r\ncreated_at: t\r\nupdated_at: t\r\ntags: []\r\n---\r\n\r\nbody\r\nline two";
        let parsed = parse_note(raw).unwrap();
        assert_eq!(parsed.id, "id1");
        assert_eq!(parsed.body, "body\nline two");
    }

    #[test]
    fn utf8_bom_is_accepted() {
        let raw = "\u{feff}---\nid: id1\ntitle: \"Bom Title\"\ncreated_at: t\nupdated_at: t\ntags: [\"one\"]\n---\n\nbody";
        let parsed = parse_note(raw).unwrap();
        assert_eq!(parsed.id, "id1");
        assert_eq!(parsed.title, "Bom Title");
        assert_eq!(parsed.tags, vec!["one".to_string()]);
        assert_eq!(parsed.body, "body");
    }

    #[test]
    fn block_style_yaml_tags_parse_indented_and_unindented() {
        let indented = "---\nid: id1\ntitle: \"t\"\ncreated_at: t\nupdated_at: t\ntags:\n  - research\n  - \"deep learning\"\n  - 'ai''s'\n---\n\nbody";
        let parsed = parse_note(indented).unwrap();
        assert_eq!(
            parsed.tags,
            vec![
                "research".to_string(),
                "deep learning".to_string(),
                "ai's".to_string()
            ]
        );
        assert_eq!(parsed.body, "body");

        let unindented = "---\nid: id1\ntitle: \"t\"\ncreated_at: t\nupdated_at: t\ntags:\n- alpha\n- beta\n---\n\nbody";
        let parsed_unindented = parse_note(unindented).unwrap();
        assert_eq!(
            parsed_unindented.tags,
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn empty_or_unquoted_or_single_quoted_tags_and_multiline_flow_parse() {
        let empty_tags = "---\nid: id1\ntitle: t\ncreated_at: t\nupdated_at: t\ntags:\n---\n\nb";
        assert!(parse_note(empty_tags).unwrap().tags.is_empty());

        let omitted_tags = "---\nid: id1\ntitle: t\ncreated_at: t\nupdated_at: t\n---\n\nb";
        assert!(parse_note(omitted_tags).unwrap().tags.is_empty());

        let flow_mixed = "---\nid: id1\ntitle: t\ncreated_at: t\nupdated_at: t\ntags: [alpha, 'beta''s', \"gamma\"]\n---\n\nb";
        assert_eq!(
            parse_note(flow_mixed).unwrap().tags,
            vec![
                "alpha".to_string(),
                "beta's".to_string(),
                "gamma".to_string()
            ]
        );

        let multiline_flow = "---\nid: id1\ntitle: t\ncreated_at: t\nupdated_at: t\ntags: [\n  alpha,\n  \"beta\"\n]\n---\n\nb";
        assert_eq!(
            parse_note(multiline_flow).unwrap().tags,
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn unquoted_and_single_quoted_titles_and_scalars_parse() {
        let unquoted = "---\nid: id1\ntitle: Note: Part 1#tag-like # inline comment\ncreated_at: '2026-05-01T00:00:00Z'\nupdated_at: \"2026-05-01T01:00:00Z\"\ntags: []\n---\n\nbody";
        let parsed = parse_note(unquoted).unwrap();
        assert_eq!(parsed.title, "Note: Part 1#tag-like");
        assert_eq!(parsed.created_at, "2026-05-01T00:00:00Z");
        assert_eq!(parsed.updated_at, "2026-05-01T01:00:00Z");

        let single_quoted = "---\nid: 'id1'\ntitle: 'It''s \\raw\\'\ncreated_at: t\nupdated_at: t\ntags: []\n---\n\nbody";
        let parsed_sq = parse_note(single_quoted).unwrap();
        assert_eq!(parsed_sq.id, "id1");
        assert_eq!(parsed_sq.title, "It's \\raw\\");
    }

    #[test]
    fn unknown_fields_are_preserved_in_extra_frontmatter() {
        let raw = "---\nid: id1\ntitle: \"t\"\naliases:\n  - Alias 1\n  - Alias 2\nstatus: draft\ncreated_at: t\nupdated_at: t\ntags: []\n---\n\nb";
        let parsed = parse_note(raw).unwrap();
        assert_eq!(parsed.id, "id1");
        assert_eq!(
            parsed.extra_frontmatter,
            vec![
                "aliases:".to_string(),
                "  - Alias 1".to_string(),
                "  - Alias 2".to_string(),
                "status: draft".to_string(),
            ]
        );
    }
}
