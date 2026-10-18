//! Shared markup reading for the zip-based office extractors and HTML.
//!
//! Word, Excel and PowerPoint write each part as one long line of XML, and
//! their element names share prefixes (`<w:t>` / `<w:tbl>` / `<w:tab/>`,
//! `<a:t>` / `<a:tbl>` / `<a:tc>`). A reader that scans lines or matches a
//! name prefix picks up the wrong element, so every extractor walks the same
//! tag-by-tag cursor and compares whole element names.

use tracing::warn;

/// One step through a markup document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Markup<'a> {
    /// An opening tag. `attrs` is the raw text after the name; `empty` is set
    /// for a self-closing `<name/>`.
    Open {
        name: &'a str,
        attrs: &'a str,
        empty: bool,
    },
    Close {
        name: &'a str,
    },
    /// Character data between tags, still entity-encoded.
    Text(&'a str),
}

/// Walks tags and text in order. Declarations, processing instructions and
/// comments are skipped; CDATA content is returned as text.
pub(super) struct MarkupCursor<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> MarkupCursor<'a> {
    pub(super) fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    /// Raw text up to the closing `</name>` (ASCII case-insensitive), with the
    /// cursor moved past it. HTML `<script>`/`<style>` bodies are not markup,
    /// so they must be skipped whole rather than tokenised.
    pub(super) fn skip_raw_text(&mut self, name: &str) -> &'a str {
        let rest = &self.src[self.pos..];
        let needle = format!("</{}", name.to_ascii_lowercase());
        let lower = rest.to_ascii_lowercase();
        match lower.find(&needle) {
            Some(at) => {
                let body = &rest[..at];
                let after = &rest[at..];
                let close = after.find('>').map_or(after.len(), |i| i + 1);
                self.pos += at + close;
                body
            }
            None => {
                self.pos = self.src.len();
                rest
            }
        }
    }
}

impl<'a> Iterator for MarkupCursor<'a> {
    type Item = Markup<'a>;

    fn next(&mut self) -> Option<Markup<'a>> {
        loop {
            let rest = &self.src[self.pos..];
            if rest.is_empty() {
                return None;
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                self.pos += end;
                return Some(Markup::Text(&rest[..end]));
            }
            if let Some(body) = rest.strip_prefix("<![CDATA[") {
                let end = body.find("]]>").unwrap_or(body.len());
                self.pos += "<![CDATA[".len() + (end + 3).min(body.len());
                return Some(Markup::Text(&body[..end]));
            }
            if rest.starts_with("<!--") {
                self.pos += rest.find("-->").map_or(rest.len(), |i| i + 3);
                continue;
            }
            // Attribute values may hold '>', so the tag ends at the first '>'
            // outside quotes.
            let mut quote = None;
            let mut end = None;
            for (i, b) in rest.bytes().enumerate().skip(1) {
                match (quote, b) {
                    (Some(q), _) if b == q => quote = None,
                    (Some(_), _) => {}
                    (None, b'"' | b'\'') => quote = Some(b),
                    (None, b'>') => {
                        end = Some(i);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(end) = end else {
                // An unterminated tag: nothing after it is readable markup.
                self.pos = self.src.len();
                return None;
            };
            let tag = &rest[1..end];
            self.pos += end + 1;
            if tag.starts_with('!') || tag.starts_with('?') {
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                return Some(Markup::Close { name: name.trim() });
            }
            let (tag, empty) = match tag.strip_suffix('/') {
                Some(t) => (t, true),
                None => (tag, false),
            };
            let split = tag
                .find(|c: char| c.is_ascii_whitespace())
                .unwrap_or(tag.len());
            let name = &tag[..split];
            if name.is_empty() {
                // A bare '<' in text (HTML is lenient about this).
                return Some(Markup::Text(&rest[..end + 1]));
            }
            return Some(Markup::Open {
                name,
                attrs: &tag[split..],
                empty,
            });
        }
    }
}

/// The value of `key="…"` in a tag's raw attribute text, entity-decoded.
pub(super) fn attr(attrs: &str, key: &str) -> Option<String> {
    let mut rest = attrs;
    while let Some(at) = rest.find(key) {
        let before_ok = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_ascii_whitespace());
        let after = rest[at + key.len()..].trim_start();
        if before_ok {
            if let Some(value) = after.strip_prefix('=') {
                let value = value.trim_start();
                let quote = value.chars().next()?;
                if quote == '"' || quote == '\'' {
                    let body = &value[1..];
                    let end = body.find(quote)?;
                    return Some(decode_entities(&body[..end]));
                }
            }
        }
        rest = &rest[at + key.len()..];
    }
    None
}

/// Decode named and numeric character references in one pass, so `&amp;lt;`
/// stays `&lt;` instead of being decoded twice.
pub(super) fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let decoded = tail[1..]
            .find(';')
            .filter(|&semi| semi <= 32)
            .and_then(|semi| decode_entity(&tail[1..semi + 1]).map(|c| (c, semi + 2)));
        match decoded {
            Some((c, consumed)) => {
                out.push(c);
                rest = &tail[consumed..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    if let Some(num) = entity.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match entity {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "ndash" => '\u{2013}',
        "mdash" => '\u{2014}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201C}',
        "rdquo" => '\u{201D}',
        "hellip" => '\u{2026}',
        "bull" => '\u{2022}',
        "middot" => '\u{00B7}',
        "laquo" => '\u{00AB}',
        "raquo" => '\u{00BB}',
        "copy" => '\u{00A9}',
        "reg" => '\u{00AE}',
        "trade" => '\u{2122}',
        "deg" => '\u{00B0}',
        "times" => '\u{00D7}',
        "euro" => '\u{20AC}',
        "pound" => '\u{00A3}',
        "eacute" => '\u{00E9}',
        "egrave" => '\u{00E8}',
        "aacute" => '\u{00E1}',
        "agrave" => '\u{00E0}',
        "ouml" => '\u{00F6}',
        "uuml" => '\u{00FC}',
        "auml" => '\u{00E4}',
        "szlig" => '\u{00DF}',
        "ccedil" => '\u{00E7}',
        _ => return None,
    })
}

/// Decode a text file's bytes. A BOM decides UTF-8 or UTF-16; otherwise UTF-8
/// is tried and anything that is not UTF-8 is read as Windows-1252, the
/// encoding nearly every legacy non-UTF-8 text file on disk uses. Rejecting
/// the whole file over one accented byte loses far more than a mis-mapped
/// character would.
pub(super) fn decode_text_bytes(bytes: Vec<u8>, path: &std::path::Path) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, u16::from_be_bytes);
    }
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            warn!(
                path = %path.display(),
                "file is not UTF-8; decoding it as Windows-1252"
            );
            err.into_bytes().iter().map(|&b| windows_1252(b)).collect()
        }
    }
}

fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units = bytes.chunks_exact(2).filter_map(|pair| match *pair {
        [lo, hi] => Some(unit([lo, hi])),
        _ => None,
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn windows_1252(b: u8) -> char {
    // 0x80–0x9F are the only bytes where Windows-1252 differs from Latin-1.
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}',
        '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    match b {
        0x80..=0x9F => HIGH
            .get(usize::from(b - 0x80))
            .copied()
            .unwrap_or(char::REPLACEMENT_CHARACTER),
        _ => char::from(b),
    }
}

/// Collapse three or more consecutive newlines to two and trim the ends, so
/// empty paragraphs used as spacing do not become runs of blank lines.
pub(super) fn tidy_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut newlines = 0;
    for line in text.split('\n') {
        let line = line.trim_end();
        if line.is_empty() {
            newlines += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if newlines > 1 { "\n\n" } else { "\n" });
        }
        newlines = 1;
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_reads_whole_names_and_self_closing_tags() {
        let events: Vec<_> =
            MarkupCursor::new(r#"<?xml version="1.0"?><w:tbl><w:t xml:space="preserve">a &amp; b</w:t><w:tab/></w:tbl>"#)
                .collect();
        assert_eq!(
            events,
            vec![
                Markup::Open {
                    name: "w:tbl",
                    attrs: "",
                    empty: false
                },
                Markup::Open {
                    name: "w:t",
                    attrs: r#" xml:space="preserve""#,
                    empty: false
                },
                Markup::Text("a &amp; b"),
                Markup::Close { name: "w:t" },
                Markup::Open {
                    name: "w:tab",
                    attrs: "",
                    empty: true
                },
                Markup::Close { name: "w:tbl" },
            ]
        );
    }

    #[test]
    fn entities_decode_once_including_numeric_references() {
        assert_eq!(
            decode_entities("it&#8217;s &#x2019; &amp;lt; &bogus; &"),
            "it\u{2019}s \u{2019} &lt; &bogus; &"
        );
    }

    #[test]
    fn attributes_match_whole_keys() {
        assert_eq!(attr(r#" r="B2" t="s""#, "t").as_deref(), Some("s"));
        assert_eq!(attr(r#" rt="x""#, "t"), None);
    }

    #[test]
    fn non_utf8_bytes_decode_instead_of_failing() {
        let path = std::path::Path::new("legacy.txt");
        assert_eq!(
            decode_text_bytes(b"caf\xE9 \x93q\x94".to_vec(), path),
            "café \u{201C}q\u{201D}"
        );
        assert_eq!(
            decode_text_bytes(vec![0xEF, 0xBB, 0xBF, b'h', b'i'], path),
            "hi"
        );
        assert_eq!(
            decode_text_bytes(vec![0xFF, 0xFE, b'h', 0, 0xE9, 0], path),
            "hé"
        );
        assert_eq!(
            decode_text_bytes(vec![0xFE, 0xFF, 0, b'h', 0, 0xE9], path),
            "hé"
        );
    }
}
