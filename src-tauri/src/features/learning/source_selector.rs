//! Stable exact-quote selectors for immutable and refreshed source versions.
//!
//! Immutable citations always retain their original source/version ID. These
//! selectors additionally let the UI reconnect a learner note or tutor quote to
//! a later adopted representation without pretending an ambiguous match is exact.

use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

const MAX_EXACT_CHARS: usize = 2_000;
const CONTEXT_CHARS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTextQuoteSelector {
    pub exact: String,
    pub prefix: String,
    pub suffix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningQuoteMatchStatus {
    Exact,
    ContextDisambiguated,
    Ambiguous,
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuoteMatch {
    pub status: LearningQuoteMatchStatus,
    pub start_byte: Option<usize>,
    pub end_byte: Option<usize>,
    pub candidate_count: usize,
}

fn context_before(text: &str, byte: usize) -> String {
    text.get(..byte)
        .unwrap_or_default()
        .chars()
        .rev()
        .take(CONTEXT_CHARS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn context_after(text: &str, byte: usize) -> String {
    text.get(byte..)
        .unwrap_or_default()
        .chars()
        .take(CONTEXT_CHARS)
        .collect()
}

pub fn create_quote_selector(
    source_text: &str,
    start_byte: usize,
    end_byte: usize,
) -> Result<LearningTextQuoteSelector> {
    if start_byte >= end_byte
        || !source_text.is_char_boundary(start_byte)
        || !source_text.is_char_boundary(end_byte)
    {
        return Err(AppError::InvalidInput(
            "Quote bounds must identify complete characters in the saved source.".into(),
        ));
    }
    let exact = source_text.get(start_byte..end_byte).ok_or_else(|| {
        AppError::InvalidInput("Quote bounds are outside the saved source.".into())
    })?;
    if exact.trim().is_empty() || exact.chars().count() > MAX_EXACT_CHARS {
        return Err(AppError::InvalidInput(format!(
            "A source quote must contain 1–{MAX_EXACT_CHARS} characters."
        )));
    }
    Ok(LearningTextQuoteSelector {
        exact: exact.into(),
        prefix: context_before(source_text, start_byte),
        suffix: context_after(source_text, end_byte),
    })
}

fn matching_offsets(text: &str, exact: &str) -> Vec<(usize, usize)> {
    text.match_indices(exact)
        .map(|(start, value)| (start, start + value.len()))
        .collect()
}

pub fn match_quote_selector(
    source_text: &str,
    selector: &LearningTextQuoteSelector,
) -> Result<LearningQuoteMatch> {
    if selector.exact.trim().is_empty() || selector.exact.chars().count() > MAX_EXACT_CHARS {
        return Err(AppError::InvalidInput(
            "The quote selector has invalid exact text.".into(),
        ));
    }
    if selector.prefix.chars().count() > CONTEXT_CHARS
        || selector.suffix.chars().count() > CONTEXT_CHARS
    {
        return Err(AppError::InvalidInput(
            "The quote selector context is too large.".into(),
        ));
    }
    let matches = matching_offsets(source_text, &selector.exact);
    if matches.is_empty() {
        return Ok(LearningQuoteMatch {
            status: LearningQuoteMatchStatus::NotFound,
            start_byte: None,
            end_byte: None,
            candidate_count: 0,
        });
    }
    if let [only_match] = matches.as_slice() {
        return Ok(LearningQuoteMatch {
            status: LearningQuoteMatchStatus::Exact,
            start_byte: Some(only_match.0),
            end_byte: Some(only_match.1),
            candidate_count: 1,
        });
    }
    let contextual = matches
        .iter()
        .copied()
        .filter(|(start, end)| {
            (selector.prefix.is_empty()
                || source_text
                    .get(..*start)
                    .is_some_and(|before| before.ends_with(&selector.prefix)))
                && (selector.suffix.is_empty()
                    || source_text
                        .get(*end..)
                        .is_some_and(|after| after.starts_with(&selector.suffix)))
        })
        .collect::<Vec<_>>();
    if let [only_match] = contextual.as_slice() {
        return Ok(LearningQuoteMatch {
            status: LearningQuoteMatchStatus::ContextDisambiguated,
            start_byte: Some(only_match.0),
            end_byte: Some(only_match.1),
            candidate_count: matches.len(),
        });
    }
    Ok(LearningQuoteMatch {
        status: LearningQuoteMatchStatus::Ambiguous,
        start_byte: None,
        end_byte: None,
        candidate_count: matches.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_round_trips_unicode_byte_offsets() -> Result<()> {
        let text = "Before 🦀 ownership transfers after acknowledgement. After.";
        let exact = "🦀 ownership transfers after acknowledgement";
        let start = text.find(exact).unwrap_or_default();
        let selector = create_quote_selector(text, start, start + exact.len())?;
        assert_eq!(selector.exact, exact);
        assert_eq!(
            match_quote_selector(text, &selector)?,
            LearningQuoteMatch {
                status: LearningQuoteMatchStatus::Exact,
                start_byte: Some(start),
                end_byte: Some(start + exact.len()),
                candidate_count: 1,
            }
        );
        Ok(())
    }

    #[test]
    fn prefix_and_suffix_disambiguate_repeated_exact_text() -> Result<()> {
        let text = "Alpha. Retry is safe. Beta. Retry is safe. Gamma.";
        let second = text.rfind("Retry is safe.").unwrap_or_default();
        let selector = create_quote_selector(text, second, second + "Retry is safe.".len())?;
        let matched = match_quote_selector(text, &selector)?;
        assert_eq!(
            matched.status,
            LearningQuoteMatchStatus::ContextDisambiguated
        );
        assert_eq!(matched.start_byte, Some(second));
        assert_eq!(matched.candidate_count, 2);
        Ok(())
    }

    #[test]
    fn ambiguity_and_absence_are_never_reported_as_exact() -> Result<()> {
        let ambiguous = LearningTextQuoteSelector {
            exact: "same".into(),
            prefix: String::new(),
            suffix: String::new(),
        };
        assert_eq!(
            match_quote_selector("same then same", &ambiguous)
                .unwrap()
                .status,
            LearningQuoteMatchStatus::Ambiguous
        );
        let absent = LearningTextQuoteSelector {
            exact: "missing".into(),
            prefix: String::new(),
            suffix: String::new(),
        };
        assert_eq!(
            match_quote_selector("same then same", &absent)
                .unwrap()
                .status,
            LearningQuoteMatchStatus::NotFound
        );
        Ok(())
    }

    #[test]
    fn invalid_utf8_boundaries_are_rejected() {
        let text = "a🦀b";
        assert!(create_quote_selector(text, 2, text.len() - 1).is_err());
    }
}
