//! A small CSS tokenizer: strings, comments, `url(` and `@import`, which is all
//! §6.4 needs. No CSS crate: the job is to find every reference, not to
//! understand the cascade.
//!
//! The text is cut into segments at `{`, `;` and `}` outside strings and
//! parentheses. A segment ending in `{` is a prelude (selectors, `@media …`)
//! and is kept; a segment starting with `@` is an at-statement; anything else
//! is a declaration. Declarations are the unit of removal: one whose `url()`
//! cannot be inlined is dropped whole, so nothing external survives (T10).

use url::Url;

use super::MAX_IMPORT_DEPTH;

/// What a reference in CSS points at, which decides the allowed content types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CssRefKind {
    /// `@import`: another stylesheet, inlined as text.
    Import,
    /// A `url()` inside `@font-face`.
    Font,
    /// Any other `url()`.
    Image,
}

/// A reference found in CSS text, unresolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CssRef {
    pub url: String,
    pub kind: CssRefKind,
}

/// The rewriter's view of the fetched resources.
pub(super) trait CssInliner {
    /// The `data:` URL to substitute for an image or font reference, or
    /// `None` when it was not inlined (the declaration or `src` entry goes).
    fn data_url(&self, raw: &str, base: &Url) -> Option<String>;
    /// The imported sheet's decoded text and URL, or `None` to drop the `@import`.
    fn import(&self, raw: &str, base: &Url) -> Option<(String, Url)>;
}

/// Every reference in `css` that a rewrite would try to inline.
pub(super) fn collect_refs(css: &str) -> Vec<CssRef> {
    let css = strip_comments(css);
    let mut refs = Vec::new();
    for segment in segments(&css) {
        match classify(&segment) {
            SegmentKind::Import => {
                if let Some((url, _)) = import_target(segment.text) {
                    refs.push(CssRef {
                        url,
                        kind: CssRefKind::Import,
                    });
                }
            }
            SegmentKind::Declaration if !declaration_is_refused(segment.text) => {
                let kind = if segment.font_face {
                    CssRefKind::Font
                } else {
                    CssRefKind::Image
                };
                for token in url_tokens(segment.text) {
                    if !is_fragment(&token.value) {
                        refs.push(CssRef {
                            url: token.value,
                            kind,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    refs
}

/// Rewrite a stylesheet (a `<style>` element, a linked or imported sheet):
/// comments stripped, `@import` inlined while `depth < MAX_IMPORT_DEPTH`,
/// `url()` replaced by `data:` or its declaration dropped, `@charset` dropped.
pub(super) fn rewrite_sheet(css: &str, base: &Url, inliner: &dyn CssInliner, depth: u8) -> String {
    let css = strip_comments(css);
    let mut out = String::with_capacity(css.len());
    for segment in segments(&css) {
        let (text, keep_end) = rewrite_segment(&segment, base, inliner, depth);
        out.push_str(&text);
        if keep_end || matches!(segment.end, End::Open | End::Close) {
            out.push_str(segment.end.as_str());
        }
    }
    out
}

/// Rewrite a `style=""` attribute value: declarations only, no `@import`.
pub(super) fn rewrite_declarations(style: &str, base: &Url, inliner: &dyn CssInliner) -> String {
    rewrite_sheet(style, base, inliner, MAX_IMPORT_DEPTH)
        .trim()
        .to_string()
}

/// Whether `value` holds a `url()` that is not a same-document `#fragment`.
pub(super) fn has_external_url(value: &str) -> bool {
    url_tokens(value)
        .iter()
        .any(|token| !is_fragment(&token.value))
}

/// Wrap `css` in `@media <condition>` (a `<link media>` or an `@import`
/// condition). `None` when the condition could break out of the rule.
pub(super) fn wrap_media(css: String, condition: &str) -> Option<String> {
    let condition = condition.trim();
    let lower = condition.to_ascii_lowercase();
    if condition.is_empty()
        || lower == "all"
        || lower.starts_with("layer")
        || lower.starts_with("supports")
    {
        return Some(css);
    }
    if condition.contains(['{', '}', ';', '<', '>', '\\', '"', '\'']) {
        return None;
    }
    Some(format!("@media {condition} {{\n{css}\n}}"))
}

/// Remove `/* … */` outside strings. Comments can hide `url(` from a naive
/// scan, and the browser sees exactly the text that was checked.
pub(super) fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.char_indices().peekable();
    let mut quote: Option<char> = None;
    while let Some((i, ch)) = chars.next() {
        if let Some(q) = quote {
            out.push(ch);
            if ch == '\\' {
                if let Some((_, next)) = chars.next() {
                    out.push(next);
                }
            } else if ch == q || ch == '\n' {
                quote = None;
            }
            continue;
        }
        if ch == '/' && css[i + 1..].starts_with('*') {
            chars.next();
            let rest = &css[i + 2..];
            let skip = rest.find("*/").map(|end| end + 2).unwrap_or(rest.len());
            while chars.peek().is_some_and(|(j, _)| *j < i + 2 + skip) {
                chars.next();
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch == '\\' {
            out.push(ch);
            if let Some((_, next)) = chars.next() {
                out.push(next);
            }
            continue;
        }
        out.push(ch);
    }
    out
}

/// How a segment ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Open,
    Semi,
    Close,
    Eof,
}

impl End {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "{",
            Self::Semi => ";",
            Self::Close => "}",
            Self::Eof => "",
        }
    }
}

/// Text between two top-level delimiters.
struct Segment<'a> {
    text: &'a str,
    end: End,
    /// Inside an `@font-face` block.
    font_face: bool,
}

enum SegmentKind {
    Prelude,
    Import,
    Charset,
    Namespace,
    OtherAt,
    Declaration,
    Blank,
}

/// Cut `css` at `{`, `;` and `}` that are outside strings and parentheses.
fn segments(css: &str) -> Vec<Segment<'_>> {
    let bytes = css.as_bytes();
    let mut out = Vec::new();
    let mut blocks: Vec<bool> = Vec::new();
    let (mut start, mut i, mut parens) = (0usize, 0usize, 0usize);
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let Some(&b) = bytes.get(i) else { break };
        if let Some(q) = quote {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == q || b == b'\n' {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'\\' => {
                i += 2;
                continue;
            }
            b'"' | b'\'' => quote = Some(b),
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'{' | b';' | b'}' if parens == 0 => {
                let font_face = blocks.last().copied().unwrap_or(false);
                let text = &css[start..i];
                let end = match b {
                    b'{' => End::Open,
                    b';' => End::Semi,
                    _ => End::Close,
                };
                out.push(Segment {
                    text,
                    end,
                    font_face,
                });
                match end {
                    End::Open => {
                        blocks.push(text.trim().to_ascii_lowercase().starts_with("@font-face"))
                    }
                    End::Close => {
                        blocks.pop();
                    }
                    _ => {}
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < bytes.len() {
        let font_face = blocks.last().copied().unwrap_or(false);
        out.push(Segment {
            text: &css[start..],
            end: End::Eof,
            font_face,
        });
    }
    out
}

fn classify(segment: &Segment<'_>) -> SegmentKind {
    let trimmed = segment.text.trim_start();
    if segment.end == End::Open {
        return SegmentKind::Prelude;
    }
    if trimmed.trim_end().is_empty() {
        return SegmentKind::Blank;
    }
    let lower = trimmed.get(..10).unwrap_or(trimmed).to_ascii_lowercase();
    if lower.starts_with("@import") {
        SegmentKind::Import
    } else if lower.starts_with("@charset") {
        SegmentKind::Charset
    } else if lower.starts_with("@namespace") {
        SegmentKind::Namespace
    } else if lower.starts_with('@') {
        SegmentKind::OtherAt
    } else {
        SegmentKind::Declaration
    }
}

/// The rewritten text of one segment, and whether its `;` is still wanted.
fn rewrite_segment(
    segment: &Segment<'_>,
    base: &Url,
    inliner: &dyn CssInliner,
    depth: u8,
) -> (String, bool) {
    let keep = |text: &str| (text.to_string(), true);
    match classify(segment) {
        SegmentKind::Prelude | SegmentKind::Blank | SegmentKind::Namespace => keep(segment.text),
        SegmentKind::Charset => (String::new(), false),
        SegmentKind::Import => match rewrite_import(segment.text, base, inliner, depth) {
            Some(text) => (text, false),
            None => (String::new(), false),
        },
        SegmentKind::OtherAt => {
            let external = url_tokens(segment.text)
                .iter()
                .any(|t| !is_fragment(&t.value));
            if external || has_escape_outside_strings(segment.text) {
                (String::new(), false)
            } else {
                keep(segment.text)
            }
        }
        SegmentKind::Declaration => match rewrite_declaration(segment.text, base, inliner) {
            Some(text) => (text, true),
            None => (String::new(), false),
        },
    }
}

/// Replace an `@import` with the imported sheet, rewritten one level deeper.
fn rewrite_import(
    statement: &str,
    base: &Url,
    inliner: &dyn CssInliner,
    depth: u8,
) -> Option<String> {
    if depth >= MAX_IMPORT_DEPTH || has_escape_outside_strings(statement) {
        return None;
    }
    let (raw, condition) = import_target(statement)?;
    let (text, sheet_url) = inliner.import(&raw, base)?;
    let inner = rewrite_sheet(&text, &sheet_url, inliner, depth + 1);
    wrap_media(inner, &condition)
}

/// The URL and trailing condition of `@import url(x) screen` / `@import "x" screen`.
fn import_target(statement: &str) -> Option<(String, String)> {
    let rest = statement.trim().get(7..)?.trim_start();
    if rest
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("url("))
    {
        let token = url_tokens(rest).into_iter().next()?;
        return Some((token.value, rest[token.end..].trim().to_string()));
    }
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let (value, end) = read_string(rest, 1, quote);
    Some((value, rest[end..].trim().to_string()))
}

/// A declaration with its `url()`s inlined, or `None` to drop it: refused
/// content, or a reference that was not inlined. A `src` list (in
/// `@font-face`) loses only the entries that failed.
fn rewrite_declaration(declaration: &str, base: &Url, inliner: &dyn CssInliner) -> Option<String> {
    if declaration_is_refused(declaration) {
        return None;
    }
    if url_tokens(declaration).is_empty() {
        return Some(declaration.to_string());
    }
    let (property, value) = declaration.split_once(':')?;
    if property.trim().eq_ignore_ascii_case("src") {
        let entries: Vec<String> = split_top_level_commas(value)
            .into_iter()
            .filter_map(|entry| inline_urls(entry, base, inliner))
            .collect();
        return (!entries.is_empty()).then(|| format!("{property}:{}", entries.join(",")));
    }
    inline_urls(declaration, base, inliner)
}

/// Every `url()` in `text` replaced by its `data:` URL; `None` if any failed.
fn inline_urls(text: &str, base: &Url, inliner: &dyn CssInliner) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for token in url_tokens(text) {
        out.push_str(&text[last..token.start]);
        if is_fragment(&token.value) {
            out.push_str(&text[token.start..token.end]);
        } else {
            let data = inliner.data_url(&token.value, base)?;
            out.push_str(&format!("url(\"{data}\")"));
        }
        last = token.end;
    }
    out.push_str(&text[last..]);
    Some(out)
}

/// Declarations dropped whatever their URLs: script-bearing legacy features,
/// `image-set()` (its bare strings are fetches) and backslash escapes outside
/// strings (which can spell `url(` without the letters).
fn declaration_is_refused(declaration: &str) -> bool {
    let compact: String = declaration
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "expression(",
        "-moz-binding",
        "behavior:",
        "javascript:",
        "vbscript:",
        "image-set(",
    ]
    .iter()
    .any(|needle| compact.contains(needle))
        || has_escape_outside_strings(declaration)
}

/// Whether a `\` appears outside a quoted string.
fn has_escape_outside_strings(text: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(ch),
            (None, '\\') => return true,
            _ => {}
        }
    }
    false
}

/// `url(#id)`: a same-document SVG reference, never a fetch.
fn is_fragment(url: &str) -> bool {
    url.trim_start().starts_with('#')
}

/// A `url(...)` token: its byte range in the text and its unquoted value.
#[derive(Debug)]
struct UrlToken {
    start: usize,
    end: usize,
    value: String,
}

/// Find every `url(` function (not `myurl(`), case-insensitively.
fn url_tokens(text: &str) -> Vec<UrlToken> {
    let lower = text.to_ascii_lowercase();
    let mut tokens = Vec::new();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("url(") {
        let start = from + pos;
        let preceded_by_ident = start
            .checked_sub(1)
            .and_then(|at| lower.as_bytes().get(at))
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
        if preceded_by_ident {
            from = start + 4;
            continue;
        }
        let (value, end) = read_url_body(text, start + 4);
        tokens.push(UrlToken {
            start,
            end,
            value: value.trim().to_string(),
        });
        from = end.max(start + 4);
    }
    tokens
}

/// Read the inside of `url(` starting at `pos`: a quoted string or a bare
/// URL up to `)`. Returns the value and the index just past the `)`.
fn read_url_body(text: &str, pos: usize) -> (String, usize) {
    let rest = &text[pos..];
    let skipped = rest.len() - rest.trim_start().len();
    let start = pos + skipped;
    let (value, mut end) = match text[start..].chars().next() {
        Some(q @ ('"' | '\'')) => read_string(text, start + 1, q),
        _ => read_bare_url(text, start),
    };
    if let Some(close) = text[end..].find(')') {
        end += close + 1;
    } else {
        end = text.len();
    }
    (value, end)
}

/// Read a string body from `pos` (just past the opening quote) to the closing
/// `quote`, resolving simple backslash escapes. Returns the value and the
/// index just past the closing quote.
fn read_string(text: &str, pos: usize, quote: char) -> (String, usize) {
    let mut value = String::new();
    let mut chars = text[pos..].char_indices();
    while let Some((i, ch)) = chars.next() {
        match ch {
            '\\' => {
                if let Some((_, next)) = chars.next() {
                    value.push(next);
                }
            }
            c if c == quote => return (value, pos + i + 1),
            '\n' => return (value, pos + i),
            c => value.push(c),
        }
    }
    (value, text.len())
}

/// Read an unquoted URL up to `)` (not consumed).
fn read_bare_url(text: &str, pos: usize) -> (String, usize) {
    let mut value = String::new();
    let mut chars = text[pos..].char_indices();
    while let Some((i, ch)) = chars.next() {
        match ch {
            ')' => return (value, pos + i),
            '\\' => {
                if let Some((_, next)) = chars.next() {
                    value.push(next);
                }
            }
            c => value.push(c),
        }
    }
    (value, text.len())
}

/// Split a value on commas outside strings and parentheses.
fn split_top_level_commas(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, mut parens) = (0usize, 0usize);
    let mut quote: Option<char> = None;
    let mut chars = value.char_indices();
    while let Some((i, ch)) = chars.next() {
        match (quote, ch) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(ch),
            (None, '(') => parens += 1,
            (None, ')') => parens = parens.saturating_sub(1),
            (None, ',') if parens == 0 => {
                parts.push(&value[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
}
