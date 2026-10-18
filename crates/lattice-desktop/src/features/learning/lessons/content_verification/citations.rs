//! The passages a lesson section cites, shown to the judge before retrieved
//! ones when it checks a claim of that section.
use super::*;

/// The retrieval kind of a passage a claim's own section cites.
pub(super) const CITED: &str = "cited";

/// Per section (unit), the source passages it cites: each block's and
/// question's quote resolved to its span in the saved references and widened
/// like a retrieved passage. A claim sees its own section's first.
pub(super) type Citations = HashMap<usize, Vec<EvidencePassage>>;

pub(super) fn section_citations(
    candidate: &Value,
    references: &ReferenceCollection<'_>,
) -> Result<Citations> {
    let mut citations = Citations::new();
    let items = ["blocks", "questions"].into_iter().flat_map(|field| {
        candidate
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
    });
    for (unit, item) in items.enumerate() {
        let Some(quote) = item.get("quote").and_then(Value::as_str) else {
            continue;
        };
        let passages = references.cited_passages(&[quote])?;
        if !passages.is_empty() {
            citations.insert(unit, passages);
        }
    }
    Ok(citations)
}

/// Add `cited` passages not overlapping `evidence` to it, in order, while
/// they fit `share`; return the reference passages it now holds.
pub(super) fn take_cited(
    llm: &dyn LLMPort,
    share: usize,
    evidence: &mut Vec<EvidencePassage>,
    cited: &[EvidencePassage],
) -> Vec<EvidencePassage> {
    for passage in cited {
        if evidence.iter().any(|shown| overlaps(shown, passage)) {
            continue;
        }
        if !within_share(llm, share, evidence.iter().chain(std::iter::once(passage))) {
            break;
        }
        evidence.push(passage.clone());
    }
    evidence
        .iter()
        .filter(|passage| passage.retrieval_kind != "execution")
        .cloned()
        .collect()
}

pub(super) fn overlaps(a: &EvidencePassage, b: &EvidencePassage) -> bool {
    a.source_id == b.source_id && a.start_byte < b.end_byte && b.start_byte < a.end_byte
}
