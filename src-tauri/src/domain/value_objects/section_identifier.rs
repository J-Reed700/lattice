//! Numbered section references shared by source structure and query navigation.

#[derive(Debug, PartialEq, Eq)]
pub struct SectionIdentifier<'a> {
    pub text: &'a str,
    pub depth: usize,
}

impl<'a> SectionIdentifier<'a> {
    /// Accept complete identifiers, including nested subsections. Never turn
    /// `706.07(a)` into its parent or accept part of a filename or URL.
    pub fn parse(text: &'a str) -> Option<Self> {
        if text.len() > 32 {
            return None;
        }
        let (number, mut suffix) = text.split_once('(').map_or((text, ""), |(n, _)| {
            (n, text.get(n.len()..).unwrap_or_default())
        });
        let segments: Vec<_> = number.split('.').collect();
        if segments
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
            || (number.len() < 3 && segments.len() == 1)
        {
            return None;
        }
        let mut depth = segments.len() + 1;
        while !suffix.is_empty() {
            let (label, rest) = suffix.strip_prefix('(')?.split_once(')')?;
            if label.is_empty() || !label.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return None;
            }
            depth += 1;
            suffix = rest;
        }
        Some(Self { text, depth })
    }

    pub fn from_query_token(token: &'a str) -> Option<Self> {
        let token = token
            .trim_matches(|c: char| {
                matches!(
                    c,
                    ',' | ';' | ':' | '?' | '!' | '.' | '"' | '\'' | '`' | '[' | ']'
                )
            })
            .trim_start_matches('§');
        Self::parse(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsection_references_remain_exact() {
        for (text, depth) in [
            ("101", 2),
            ("1.2", 3),
            ("706.07(a)", 4),
            ("706.07(a)(1)", 5),
        ] {
            assert_eq!(
                SectionIdentifier::parse(text),
                Some(SectionIdentifier { text, depth })
            );
        }
        assert_eq!(
            SectionIdentifier::from_query_token("§706.07(a),")
                .unwrap()
                .text,
            "706.07(a)"
        );
        for text in [
            "",
            "1.",
            "1",
            "706..07",
            "706.07()",
            "706.07(a)file",
            "mpep-706.pdf",
            "§706%",
            "https://a/706",
        ] {
            assert!(SectionIdentifier::parse(text).is_none(), "{text}");
        }
    }
}
