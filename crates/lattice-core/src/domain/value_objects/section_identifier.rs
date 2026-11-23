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
        let (number, suffix) = split_parenthesized_suffix(text);
        let segments: Vec<_> = number.split('.').collect();
        if segments
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
            || (number.len() < 3 && segments.len() == 1)
        {
            return None;
        }
        let suffix_depth = count_parenthesized_groups(suffix)?;
        Some(Self {
            text,
            depth: segments.len() + 1 + suffix_depth,
        })
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

    /// Whether natural-language `question` explicitly asks about a numbered
    /// clause, section, article, paragraph, or legal subsection identifier.
    /// Rejects bare numbers such as years (`2024`), decimals (`3.5`), and
    /// software versions (`3.12`) unless preceded by an explicit structural cue
    /// (`clause 12`, `section 7`, `Article 5`, `§ 4.2`) or written with `§` /
    /// parenthesized subsection syntax (`706.07(a)`).
    pub fn has_explicit_reference(question: &str) -> bool {
        let mut prev_was_cue = false;
        for raw in question.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
            let token = clean_reference_token(raw);
            if token.is_empty() {
                continue;
            }
            if let Some(after_sec_mark) = token.strip_prefix('§') {
                let target = after_sec_mark.trim_start_matches('§');
                if target.is_empty() {
                    prev_was_cue = true;
                    continue;
                }
                if is_explicit_numbered_ref(target) {
                    return true;
                }
                prev_was_cue = false;
                continue;
            }
            if prev_was_cue && is_explicit_numbered_ref(token) {
                return true;
            }
            if has_legal_subsection_syntax(token) {
                return true;
            }
            prev_was_cue = is_reference_cue(token);
        }
        false
    }
}

const REFERENCE_CUES: &[&str] = &[
    "section",
    "sections",
    "sec",
    "clause",
    "clauses",
    "cl",
    "article",
    "articles",
    "art",
    "paragraph",
    "paragraphs",
    "para",
    "paras",
    "subsection",
    "subsections",
    "subsec",
    "subclause",
    "subclauses",
    "subparagraph",
    "subparagraphs",
    "chapter",
    "chapters",
    "ch",
    "chap",
    "schedule",
    "schedules",
    "sched",
    "appendix",
    "annex",
    "rule",
    "rules",
    "regulation",
    "regulations",
    "reg",
];

fn split_parenthesized_suffix(text: &str) -> (&str, &str) {
    text.split_once('(')
        .map_or((text, ""), |(stem, _)| (stem, &text[stem.len()..]))
}

fn count_parenthesized_groups(mut suffix: &str) -> Option<usize> {
    let mut count = 0usize;
    while !suffix.is_empty() {
        let (label, rest) = suffix.strip_prefix('(')?.split_once(')')?;
        if label.is_empty() || !label.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        count += 1;
        suffix = rest;
    }
    Some(count)
}

fn clean_reference_token(token: &str) -> &str {
    let token = token.trim_matches(|c: char| {
        matches!(
            c,
            ',' | ';' | ':' | '?' | '!' | '"' | '\'' | '`' | '[' | ']' | '{' | '}'
        )
    });
    let token = token.strip_prefix('(').unwrap_or(token);
    let token = token.strip_suffix('.').unwrap_or(token);
    if token.ends_with(')')
        && token.bytes().filter(|&b| b == b'(').count()
            < token.bytes().filter(|&b| b == b')').count()
    {
        &token[..token.len() - 1]
    } else {
        token
    }
}

fn is_reference_cue(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    REFERENCE_CUES.contains(&lower.as_str())
}

fn is_explicit_numbered_ref(token: &str) -> bool {
    if token.len() > 32 {
        return false;
    }
    let (stem, suffix) = split_parenthesized_suffix(token);
    if stem.is_empty()
        || !stem.as_bytes().first().is_some_and(u8::is_ascii_digit)
        || count_parenthesized_groups(suffix).is_none()
    {
        return false;
    }
    stem.split(['.', '-'])
        .all(|seg| !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn has_legal_subsection_syntax(token: &str) -> bool {
    if token.len() > 32 {
        return false;
    }
    let (stem, suffix) = split_parenthesized_suffix(token);
    if stem.is_empty() || suffix.is_empty() {
        return false;
    }
    stem.split('.')
        .all(|seg| !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_digit()))
        && count_parenthesized_groups(suffix).is_some_and(|groups| groups >= 1)
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

    #[test]
    fn explicit_reference_detection_requires_cues_or_legal_subsection_syntax() {
        for matched in [
            "What does clause 12 say?",
            "Check section 7 of the policy.",
            "Does Article 5 allow subletting?",
            "Look at § 4.2 in the contract.",
            "Look at §4.2, please.",
            "Compare Section 3.1.2 against Schedule 1.",
            "What does paragraph 4(b)(1) require?",
            "Notice 706.07(a) applies here.",
            "See (clause 12) for exceptions.",
            "Review sec. 101 and ch. 3.",
        ] {
            assert!(
                SectionIdentifier::has_explicit_reference(matched),
                "expected match: {matched}"
            );
        }

        for rejected in [
            "What was revenue in 2024?",
            "What is the 3.5 GPA requirement?",
            "How do I upgrade Python 3.12?",
            "Is deploying 4.2.2 to staging allowed yet?",
            "What does section above say about 2024?",
            "Read the 2024 section summary.",
            "Compare mpep-706.pdf with https://a/706.",
            "Check §706% formatting.",
            "Explain rule of 72.",
        ] {
            assert!(
                !SectionIdentifier::has_explicit_reference(rejected),
                "expected non-match: {rejected}"
            );
        }
    }
}
