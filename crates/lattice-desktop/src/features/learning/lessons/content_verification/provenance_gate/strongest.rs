//! The strongest counted support names the claim (claim sources design §8):
//! once the gate has weighed every support of a sufficient claim, the one
//! that decided it is the support its check records and its citation repair
//! cites, so a claim is never recorded as verified by a weaker source than
//! the one that met its bar.

use super::*;

/// Rewrites `finding`'s supporting quotation (and `supported_by`) to the
/// first support that met the bar, when that is not the one named now. The
/// support it replaces moves to `also_supported`; the strongest passage joins
/// the finding's evidence so the check finds it there.
pub(in crate::features::learning::lessons::content_verification) fn name_strongest(
    finding: &mut Finding,
) {
    let Some(best) = finding
        .trust
        .as_deref()
        .and_then(|trust| match &trust.decision {
            Decision::Sufficient { met_by, .. } => met_by.first().cloned(),
            Decision::Insufficient { .. } => None,
        })
    else {
        return;
    };
    let quote = finding.supporting_quote.clone().unwrap_or_default();
    let holds_quote = |passage: &EvidencePassage| {
        !quote.is_empty() && passage.retrieval_kind != EXECUTION && passage.text.contains(&quote)
    };
    let current = finding
        .evidence
        .iter()
        .position(|passage| holds_quote(passage));
    if current.is_some_and(|at| finding.evidence[at].source_id == best) {
        return;
    }
    // Another passage of the evidence holding the same quotation: it goes
    // first, where the check looks.
    if let Some(at) = finding
        .evidence
        .iter()
        .position(|passage| passage.source_id == best && holds_quote(passage))
    {
        let passage = finding.evidence.remove(at);
        finding.evidence.insert(0, passage);
        return;
    }
    let Some(at) = finding
        .two_pass
        .also_supported
        .iter()
        .position(|other| other.passage.source_id == best)
    else {
        return;
    };
    let strongest = finding.two_pass.also_supported.remove(at);
    // What the section cites: as pass B recorded it, else the passages pass
    // A compared, which are the section's citations.
    let cited: Vec<String> = match &finding.two_pass.supported_by {
        Some(found) => found.cited.clone(),
        None => {
            let mut cited: Vec<String> = finding
                .evidence
                .iter()
                .filter(|passage| passage.retrieval_kind != EXECUTION)
                .map(|passage| passage.source_id.clone())
                .collect();
            cited.sort();
            cited.dedup();
            cited
        }
    };
    if let Some(at) = current {
        finding.two_pass.also_supported.push(SupportingPassage {
            passage: finding.evidence[at].clone(),
            quote,
        });
    }
    finding.two_pass.supported_by =
        (!cited.contains(&strongest.passage.source_id)).then(|| SupportedElsewhere {
            source_id: strongest.passage.source_id.clone(),
            start_byte: strongest.passage.start_byte,
            end_byte: strongest.passage.end_byte,
            cited,
        });
    if !finding
        .evidence
        .iter()
        .any(|passage| same_passage(passage, &strongest.passage))
    {
        finding.evidence.insert(0, strongest.passage.clone());
    }
    finding.supporting_quote = Some(strongest.quote);
}
