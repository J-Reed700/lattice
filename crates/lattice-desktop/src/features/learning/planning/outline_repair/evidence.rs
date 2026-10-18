//! The evidence a repair verdict rests on, and whether that support is
//! enough for the assertion's kind (source provenance §6.2, P7).
use crate::features::learning::dto::LearningSourceDto;
use crate::features::learning::generation;
use crate::features::learning::lessons::content_verification::{gate_reason, weigh_claim};
use crate::features::learning::reference_collection::ReferencePassage;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::claim_verification::ClaimVerdict;
use lattice_core::domain::learning::claim_review::{GateVerdict, SupportedBy};
use lattice_core::shared::error::Result;

/// A span of the located passage short enough to cite: the passage itself
/// when it fits, else the retrieved chunk the verification window was built
/// around. The source's own text is the match target, as in evidence linking.
fn citable_span(
    passage: &ReferencePassage,
    base: &[ReferencePassage],
    sources: &[LearningSourceDto],
) -> Option<String> {
    let source = sources.iter().find(|s| s.id == passage.source_id)?;
    let fits = |text: &str| generation::validate_excerpt_quote(text, &source.excerpt).is_ok();
    if fits(&passage.text) {
        return Some(passage.text.trim().to_owned());
    }
    base.iter()
        .find(|chunk| {
            chunk.source_id == passage.source_id
                && chunk.start_byte >= passage.start_byte
                && chunk.end_byte <= passage.end_byte
                && fits(&chunk.text)
        })
        .map(|chunk| chunk.text.trim().to_owned())
}

/// The evidence a verdict rests on: the source of the passage the judge named
/// and the text to keep. A supported claim needs a citable span; a
/// contradiction shows the whole located passage to the user.
pub(super) fn locate(
    located: &str,
    verdict: ClaimVerdict,
    passages: &[ReferencePassage],
    base: &[ReferencePassage],
    sources: &[LearningSourceDto],
) -> Option<(String, String)> {
    let passage = passages.iter().find(|p| p.text == located)?;
    let quote = match verdict {
        ClaimVerdict::Supported => citable_span(passage, base, sources)?,
        _ => passage.text.trim().to_owned(),
    };
    Some((passage.source_id.clone(), quote))
}

/// The provenance gate's verdict on a supported assertion: `Some` with the
/// gate's reason when its support fails the bar for its kind, `None` when
/// it is enough or the gate cannot weigh it (no trust scope, or no passage
/// of `page` holds the quotation). The outline has no claim ledger and no
/// review queue, so a failing support is repaired as unsupported: narrowed
/// on the first attempt, removed on the next.
pub(super) async fn insufficient(
    llm: &dyn LLMPort,
    unit: usize,
    claim: &str,
    page: &[ReferencePassage],
    source_id: &str,
    quote: &str,
) -> Result<Option<String>> {
    let supported = SupportedBy {
        source_version_id: source_id.to_owned(),
        supporting_quote: quote.to_owned(),
    };
    let Some(trust) = weigh_claim(llm, unit, claim, page, supported).await? else {
        return Ok(None);
    };
    Ok(match trust.verdict() {
        GateVerdict::Sufficient => None,
        GateVerdict::Fails(_) => Some(gate_reason(&trust)),
    })
}
