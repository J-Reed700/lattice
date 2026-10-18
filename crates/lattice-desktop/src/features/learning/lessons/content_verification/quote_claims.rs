//! A claim that quotes a source is settled by finding the quotation in that
//! source's full text, not by asking a model whether a passage says it: apart
//! from spacing, typography and letter case, the words are there or they are
//! not. A match is support, with the passages around it as evidence. Anything
//! else, including a quotation that is not found, goes to the model's check.
use super::*;

/// Marks that open or close a quotation in a claim.
const QUOTE_MARKS: &[char] = &['"', '\u{201c}', '\u{201d}'];
/// Marks a quotation uses for words it leaves out.
const ELLIPSES: &[&str] = &["[\u{2026}]", "[...]", "\u{2026}", "..."];

/// The quotation `statement` attributes to a source: the text before its
/// first quotation mark (the attribution), and the parts of the quotation
/// between its outermost marks, split where it leaves words out. None when
/// the statement asserts anything after the quotation.
fn quotation(statement: &str) -> Option<(&str, Vec<&str>)> {
    let open = statement.find(QUOTE_MARKS)?;
    let close = statement.rfind(QUOTE_MARKS)?;
    let quoted = statement.get(open..close)?;
    let quoted = quoted.get(quoted.chars().next()?.len_utf8()..)?;
    let after = statement.get(close..)?;
    if open >= close || after.chars().any(char::is_alphanumeric) {
        return None;
    }
    let mut parts = vec![quoted];
    for ellipsis in ELLIPSES {
        parts = parts
            .into_iter()
            .flat_map(|part| part.split(ellipsis))
            .collect();
    }
    let parts: Vec<_> = parts
        .into_iter()
        .map(|part| {
            part.trim_matches(|c: char| {
                c.is_whitespace() || QUOTE_MARKS.contains(&c) || matches!(c, '.' | ',' | ';' | ':')
            })
        })
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then_some((statement.get(..open)?, parts))
}

/// The source `attribution` names: the lesson's source by the index its text
/// uses, or a source the lesson cites whose title the attribution contains.
fn attributed<'r>(
    references: &'r ReferenceCollection<'_>,
    citations: &Citations,
    attribution: &str,
) -> Option<&'r LearningSourceDto> {
    let lower = attribution.to_lowercase();
    let id = match source_mentions(attribution).first() {
        Some(mention) => citations.source_at(mention.index)?.to_owned(),
        None => references
            .sources
            .iter()
            .find(|source| {
                citations.cites(&source.id)
                    && !source.title.trim().is_empty()
                    && lower.contains(&source.title.trim().to_lowercase())
            })?
            .id
            .clone(),
    };
    references.sources.iter().find(|source| source.id == id)
}

/// The finding for `claim` when it quotes a source it names and every part
/// of the quotation is in that source's text, in order, each part long
/// enough to be a citation quote of its own. None sends the claim to the
/// model's check.
pub(super) fn settle(
    references: &ReferenceCollection<'_>,
    citations: &Citations,
    unit: usize,
    claim: &Claim,
) -> Result<Option<Finding>> {
    let Some((attribution, parts)) = quotation(&claim.statement) else {
        return Ok(None);
    };
    let Some(source) = attributed(references, citations, attribution) else {
        return Ok(None);
    };
    let mut spans = Vec::new();
    let mut from = 0;
    for part in parts {
        let Some((start, end)) = crate::features::learning::outline_citations::caseless_quote_span(
            &source.excerpt,
            part,
            from,
        ) else {
            return Ok(None);
        };
        let Some(found) = source.excerpt.get(start..end) else {
            return Ok(None);
        };
        if crate::features::learning::generation::validate_excerpt_quote(found, &source.excerpt)
            .is_err()
        {
            return Ok(None);
        }
        spans.push((start, end));
        from = end;
    }
    let supporting_quote = spans
        .first()
        .and_then(|(start, end)| source.excerpt.get(*start..*end))
        .map(str::to_owned);
    let mut evidence = references.source_passages(&source.id, &spans)?;
    canonicalize_evidence(&mut evidence);
    Ok(Some(Finding {
        unit,
        quote: claim.quote.clone(),
        statement: claim.statement.clone(),
        verdict: ClaimVerdict::Supported,
        reason: format!(
            "The quoted wording is in \"{}\", word for word apart from spacing, typography and letter case.",
            source.title
        ),
        evidence,
        supporting_quote,
    }))
}

#[cfg(test)]
#[path = "quote_claims_tests.rs"]
mod tests;
