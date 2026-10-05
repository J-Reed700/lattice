//! Restore exact source typography without treating a fuzzy match as evidence.
use crate::features::learning::{dto::LearningSourceDto, generation, outline_evidence};
use serde_json::{json, Value};

fn typography(character: char) -> char {
    match character {
        '\u{2018}' | '\u{2019}' => '\'',
        '\u{201c}' | '\u{201d}' => '"',
        c if c.is_whitespace() => ' ',
        c => c,
    }
}

struct SourceText {
    comparable: String,
    // Character positions in comparable map back to complete original UTF-8 spans.
    spans: Vec<(usize, usize)>,
}
impl SourceText {
    fn new(text: &str) -> Self {
        let mut comparable = String::new();
        let mut spans: Vec<(usize, usize)> = Vec::new();
        for (start, character) in text.char_indices() {
            let end = start + character.len_utf8();
            let folded = typography(character);
            if folded == ' ' && comparable.ends_with(' ') {
                if let Some(span) = spans.last_mut() {
                    span.1 = end;
                }
            } else {
                comparable.push(folded);
                spans.push((start, end));
            }
        }
        Self { comparable, spans }
    }

    fn exact_span<'a>(&self, source: &'a str, quote: &str) -> Option<&'a str> {
        let needle = Self::new(quote.trim()).comparable;
        if needle.is_empty() {
            return None;
        }
        let offset = self.comparable.find(&needle)?;
        let start = self.comparable.get(..offset)?.chars().count();
        let end = start + needle.chars().count() - 1;
        source.get(self.spans.get(start)?.0..self.spans.get(end)?.1)
    }
}

/// Recover original source bytes for a typography-only mismatch. The copied
/// span must pass the same quote validator used for publication.
pub(in crate::features::learning) fn exact_source_quote<'a>(
    source: &'a str,
    quote: &str,
) -> Option<&'a str> {
    let exact = SourceText::new(source).exact_span(source, quote)?;
    generation::validate_excerpt_quote(exact, source).ok()?;
    Some(exact)
}

/// Fix only typography in the cited source, then require the ordinary strict
/// quote check to pass on the copied span. Claims and source indices never change;
/// the full semantic review is still required before acceptance.
pub(in crate::features::learning) fn restore_source_quotes(
    candidate: &mut Value,
    sources: &[LearningSourceDto],
) -> usize {
    let failures = outline_evidence::quote_checks(candidate, sources);
    let mut restored = 0;
    for (index, source) in sources.iter().enumerate() {
        let matching: Vec<_> = failures
            .iter()
            .filter(|check| check["sourceIndex"].as_u64() == Some(index as u64))
            .collect();
        if matching.is_empty() {
            continue;
        }
        let text = SourceText::new(&source.excerpt);
        for check in matching {
            let Some(quote) = check["quote"].as_str() else {
                continue;
            };
            let Some(exact) = text.exact_span(&source.excerpt, quote) else {
                continue;
            };
            if generation::generated_source_ids(sources, Some(index), exact).is_err() {
                continue;
            }
            let path = format!(
                "/{}",
                check["path"]
                    .as_str()
                    .unwrap_or_default()
                    .replace('[', "/")
                    .replace(']', "")
                    .replace('.', "/")
            );
            if let Some(field) = candidate.pointer_mut(&path) {
                field["quote"] = json!(exact);
                restored += 1;
            }
        }
    }
    restored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(text: &str) -> LearningSourceDto {
        LearningSourceDto {
            id: "source".into(),
            title: "Reference".into(),
            url: None,
            excerpt: text.into(),
            acquired_at: 0,
        }
    }

    #[test]
    fn restores_the_three_rust_apostrophe_failures_from_exact_utf8_source_spans() {
        let api = "Rust’s standard library has extensive API documentation, with\nexplanations of how to use various things, as well as example code for\naccomplishing various tasks.";
        let cargo =
            "The Cargo Book is a guide to Cargo, Rust’s build tool and\ndependency manager.";
        let sources = [source(&format!(
            "🔎 Reference\n\n{api} Code examples follow.\n\n{cargo}"
        ))];
        let field = |quote: &str| json!({"text":"Keep the attached claim unchanged.","sourceIndex":0,"quote":quote.replace('’', "'")});
        let mut candidate = json!({"modules":[{"summary":{"text":"Summary", "sourceIndex":0,"quote":cargo},"outcomes":[field(api)],"lessons":[field(cargo),field(api)]}]});
        assert_eq!(
            outline_evidence::quote_checks(&candidate, &sources).len(),
            3
        );
        assert_eq!(restore_source_quotes(&mut candidate, &sources), 3);
        assert!(outline_evidence::quote_checks(&candidate, &sources).is_empty());
        assert_eq!(candidate["modules"][0]["outcomes"][0]["quote"], api);
        assert_eq!(candidate["modules"][0]["lessons"][0]["quote"], cargo);
        assert_eq!(
            candidate["modules"][0]["outcomes"][0]["text"],
            "Keep the attached claim unchanged."
        );
        assert_eq!(restore_source_quotes(&mut candidate, &sources), 0);
    }

    #[test]
    fn copies_original_smart_quotes_and_whitespace_without_loosening_validation() {
        let exact = "The pastry’s\n\n‘rest’ takes five minutes before rolling.";
        let sources = [source(exact)];
        let mut candidate = json!({"modules":[{"summary":{"sourceIndex":0,"quote":"The pastry's 'rest' takes five minutes before rolling."}}]});
        assert_eq!(restore_source_quotes(&mut candidate, &sources), 1);
        assert_eq!(candidate["modules"][0]["summary"]["quote"], exact);
        assert!(generation::generated_source_ids(
            &sources,
            Some(0),
            "The pastry's 'rest' takes five minutes before rolling."
        )
        .is_err());
    }

    #[test]
    fn does_not_repair_changed_meaning_code_punctuation_wrong_sources_or_missing_evidence() {
        let exact = "The cell’s response does not increase by 5 units: x != 10.";
        let sources = [
            source(exact),
            source("A different source discusses another biological response."),
        ];
        let ascii = exact.replace('’', "'");
        for (index, quote) in [
            (Some(0), ascii.replace("does not", "does")),
            (Some(0), ascii.replace("5 units", "6 units")),
            (Some(0), ascii.replace("!=", "=")),
            (Some(0), ascii.replace("response does", "response\\ndoes")),
            (Some(1), ascii.clone()),
            (Some(9), ascii.clone()),
            (None, ascii),
            (Some(0), "The cell’s".into()),
            (Some(0), String::new()),
        ] {
            let mut candidate =
                json!({"modules":[{"summary":{"sourceIndex":index,"quote":quote}}]});
            let before = candidate.clone();
            assert_eq!(restore_source_quotes(&mut candidate, &sources), 0);
            assert_eq!(candidate, before);
            assert_eq!(
                outline_evidence::quote_checks(&candidate, &sources).len(),
                1
            );
        }
    }
}
