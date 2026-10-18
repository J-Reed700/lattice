//! The passages a lesson section cites, shown to the judge before retrieved
//! ones when it checks a claim of that section, and the sources the lesson
//! cites, whose passages are retrieved before the rest of the course's.
use super::*;

/// The retrieval kind of a passage a claim's own section cites.
pub(super) const CITED: &str = "cited";

/// What a lesson cites. Per section (unit), the source passages it cites:
/// each block's and question's quote resolved to its span in the saved
/// references and widened like a retrieved passage. A claim sees its own
/// section's first, and the sources the lesson cites anywhere before the
/// rest of the course's.
#[derive(Default)]
pub(super) struct Citations {
    by_unit: HashMap<usize, Vec<EvidencePassage>>,
    sources: HashSet<String>,
}

impl Citations {
    pub(super) fn new(by_unit: HashMap<usize, Vec<EvidencePassage>>) -> Self {
        let sources = by_unit
            .values()
            .flatten()
            .map(|passage| passage.source_id.clone())
            .collect();
        Self { by_unit, sources }
    }

    /// The passages section `unit` cites.
    pub(super) fn of(&self, unit: usize) -> &[EvidencePassage] {
        self.by_unit.get(&unit).map_or(&[], Vec::as_slice)
    }

    /// Whether the lesson cites `source_id` anywhere.
    pub(super) fn cites(&self, source_id: &str) -> bool {
        self.sources.contains(source_id)
    }
}

pub(super) fn section_citations(
    candidate: &Value,
    references: &ReferenceCollection<'_>,
) -> Result<Citations> {
    let mut citations = HashMap::new();
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
    Ok(Citations::new(citations))
}

/// The next page of evidence for `claim`, in the `share` it has alongside
/// `held`, skipping passages that overlap `shown`. Passages of the sources
/// the lesson cites that mention the claim's subject come first, best first;
/// passages from anywhere in the course fill the room they leave.
pub(super) async fn lesson_sources_first(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    claim: &str,
    share: usize,
    held: &[EvidencePassage],
    shown: &[EvidencePassage],
    citations: &Citations,
) -> Result<Vec<EvidencePassage>> {
    let mut page = Vec::new();
    if !citations.sources.is_empty() {
        (page, _) = references
            .verification_page_within(
                claim,
                shown,
                |passage| {
                    citations.cites(&passage.source_id)
                        && references.mentions_subject_of(claim, &passage.text)
                },
                |passages| within_share(llm, share, held.iter().chain(passages)),
            )
            .await?;
    }
    let seen: Vec<_> = shown.iter().chain(&page).cloned().collect();
    let (rest, _) = references
        .verification_page(claim, &seen, |passages| {
            within_share(llm, share, held.iter().chain(&page).chain(passages))
        })
        .await?;
    page.extend(rest);
    Ok(page)
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
