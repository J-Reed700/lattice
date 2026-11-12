//! Restore a cited source's own bytes over a quote that is the same words apart
//! from typography, emphasis, spacing and letter case ([`same_text::find`]).
use crate::lessons::same_text;
use crate::{dto::LearningSourceDto, generation, outline_evidence};
use serde_json::{json, Value};

/// Recover original source bytes for a typography-only mismatch. The copied
/// span must pass the same quote validator used for publication.
pub(crate) fn exact_source_quote<'a>(source: &'a str, quote: &str) -> Option<&'a str> {
    let (start, end) = same_text::find(source, quote.trim())?;
    let exact = source.get(start..end)?;
    generation::validate_excerpt_quote(exact, source).ok()?;
    Some(exact)
}

/// Copy the cited source's own bytes over a quote that is the same words
/// apart from typography and spacing, so a quote is the source's text exactly.
/// The copied span must pass the same quote validator used for publication.
/// Claims and source indices never change; the full semantic review is still
/// required before acceptance.
pub(crate) fn restore_source_quotes(candidate: &mut Value, sources: &[LearningSourceDto]) -> usize {
    let mut restored = Vec::new();
    for (path, field) in outline_evidence::citable_fields(candidate) {
        let Some(index) = field["sourceIndex"]
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
        else {
            continue;
        };
        let (Some(source), Some(quote)) = (sources.get(index), field["quote"].as_str()) else {
            continue;
        };
        let Some(exact) = same_text::find(&source.excerpt, quote.trim())
            .and_then(|(start, end)| source.excerpt.get(start..end))
        else {
            continue;
        };
        if exact != quote && generation::optional_citation(sources, Some(index), exact).is_ok() {
            restored.push((path, exact.to_owned()));
        }
    }
    let count = restored.len();
    for (path, exact) in restored {
        let pointer = format!(
            "/{}",
            path.replace('[', "/").replace(']', "").replace('.', "/")
        );
        if let Some(field) = candidate.pointer_mut(&pointer) {
            field["quote"] = json!(exact);
        }
    }
    count
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
        assert!(outline_evidence::quote_checks(&candidate, &sources).is_empty());
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
    fn copies_original_smart_quotes_and_whitespace_from_the_source() {
        let exact = "The pastry’s\n\n‘rest’ takes five minutes before rolling.";
        let sources = [source(exact)];
        let mut candidate = json!({"modules":[{"summary":{"sourceIndex":0,"quote":"The pastry's 'rest' takes five minutes before rolling."}}]});
        assert_eq!(restore_source_quotes(&mut candidate, &sources), 1);
        assert_eq!(candidate["modules"][0]["summary"]["quote"], exact);
        assert!(generation::optional_citation(&sources, Some(0), exact).is_ok());
        assert_eq!(restore_source_quotes(&mut candidate, &sources), 0);
    }

    #[test]
    fn a_quote_in_other_letter_case_is_restored_to_the_source_bytes() {
        let exact = "The borrow checker rejects two mutable borrows of one value.";
        let sources = [source(exact)];
        let mut candidate = json!({"modules":[{"summary":{"sourceIndex":0,"quote":"the Borrow Checker rejects two mutable borrows of one value"}}]});
        assert_eq!(restore_source_quotes(&mut candidate, &sources), 1);
        assert_eq!(
            candidate["modules"][0]["summary"]["quote"],
            "The borrow checker rejects two mutable borrows of one value"
        );
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
