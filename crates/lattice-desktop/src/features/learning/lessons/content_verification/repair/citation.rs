//! The citation repair (course evidence index §5.1): a claim pass B found
//! support for elsewhere in the course keeps its text, and its section's
//! citation moves to the passage found. No model writes it: the found
//! passage is the citation.
use super::super::*;
use std::collections::BTreeSet;

/// The quotation a section can cite for `passage`: the passage itself when a
/// lesson quotation may be that long, otherwise its middle, as long as a
/// quotation may be, cut to whole sentences where the middle has them. The
/// widening a citation gets (`cited_passages`) then centres on the middle
/// of the found passage, where the ranking's hit lies.
fn citable(passage: &str) -> &str {
    let text = passage.trim();
    let longest = crate::features::learning::generation::MAX_QUOTE_CHARS;
    let chars: Vec<usize> = text.char_indices().map(|(at, _)| at).collect();
    if chars.len() <= longest {
        return text;
    }
    let first = (chars.len() - longest) / 2;
    let start = chars.get(first).copied().unwrap_or_default();
    let end = chars.get(first + longest).copied().unwrap_or(text.len());
    let middle = text.get(start..end).unwrap_or_default();
    let from = middle.find(". ").map(|at| at + 2);
    let to = middle.rfind(". ").map(|at| at + 1);
    match (from, to) {
        (Some(from), Some(to)) if from < to => middle.get(from..to).unwrap_or(middle).trim(),
        _ => middle.trim(),
    }
}

/// Rewrites the citation of each section one of whose claims pass B
/// supported elsewhere to the passage it found (`supported_by`): the
/// section's `quote` becomes that passage, or its middle when the passage is
/// longer than a quotation may be ([`citable`]), and its `sourceIndex` the
/// lesson's index for the passage's source, so the next pass reads it as the
/// section's own citation (pass A). The found passage is the citation, so no
/// model writes it. A section is rewritten only when the lesson already
/// names the source by an index, the quotation is one a lesson citation may
/// carry, and the claim's support is not in doubt (the provenance gate asked
/// for no scrutiny); a section's first such claim decides. `tried` holds the
/// citations rewritten before in this job, never written again, so two
/// passages a judge weighs differently on successive passes cannot trade
/// places. Returns whether any section changed.
pub(in crate::features::learning::lessons::content_verification) fn cite_where_supported(
    candidate: &mut Value,
    findings: &[Finding],
    citations: &Citations,
    references: &ReferenceCollection<'_>,
    tried: &mut HashSet<(usize, String)>,
) -> Result<bool> {
    let mut rewritten = BTreeSet::new();
    for finding in findings {
        let Some(found) = finding.two_pass.supported_by.as_ref() else {
            continue;
        };
        if finding.verdict != ClaimVerdict::Supported
            || finding.scrutinised().is_some()
            || rewritten.contains(&finding.unit)
        {
            continue;
        }
        let Some(index) = citations.index_of(&found.source_id) else {
            tracing::info!(
                unit = finding.unit,
                source = %found.source_id,
                "A claim is supported by a source the lesson does not name; its citation stays"
            );
            continue;
        };
        let Some(source) = references
            .sources
            .iter()
            .find(|source| source.id == found.source_id)
        else {
            continue;
        };
        let quote = citable(
            source
                .excerpt
                .get(found.start_byte..found.end_byte)
                .unwrap_or_default(),
        );
        if crate::features::learning::generation::validate_excerpt_quote(quote, &source.excerpt)
            .is_err()
            || !tried.insert((finding.unit, quote.to_owned()))
        {
            continue;
        }
        let (field, position) = super::location(candidate, finding.unit)?;
        let Some(section) = candidate
            .get_mut(field)
            .and_then(Value::as_array_mut)
            .and_then(|items| items.get_mut(position))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        if section.get("quote").and_then(Value::as_str) == Some(quote)
            && section.get("sourceIndex").and_then(Value::as_u64) == Some(index)
        {
            continue;
        }
        section.insert("quote".into(), json!(quote));
        section.insert("sourceIndex".into(), json!(index));
        rewritten.insert(finding.unit);
        tracing::info!(
            unit = finding.unit,
            source = %found.source_id,
            cited = ?found.cited,
            "Repaired a section's citation to the passage that supports its claim"
        );
    }
    Ok(!rewritten.is_empty())
}
