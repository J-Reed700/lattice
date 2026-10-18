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
    /// The source each `sourceIndex` of the lesson stands for, as its
    /// citations resolve: what "Source 2" in the lesson's text names.
    by_index: HashMap<u64, String>,
}

impl Citations {
    pub(super) fn new(
        by_unit: HashMap<usize, Vec<EvidencePassage>>,
        by_index: HashMap<u64, String>,
    ) -> Self {
        let sources = by_unit
            .values()
            .flatten()
            .map(|passage| passage.source_id.clone())
            .collect();
        Self {
            by_unit,
            sources,
            by_index,
        }
    }

    /// The source the lesson's source index `index` stands for.
    pub(super) fn source_at(&self, index: u64) -> Option<&str> {
        self.by_index.get(&index).map(String::as_str)
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
    let mut by_index = HashMap::new();
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
        if let (Some(index), Some(passage)) = (
            item.get("sourceIndex").and_then(Value::as_u64),
            passages.first(),
        ) {
            by_index
                .entry(index)
                .or_insert_with(|| passage.source_id.clone());
        }
        if !passages.is_empty() {
            citations.insert(unit, passages);
        }
    }
    Ok(Citations::new(citations, by_index))
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

/// Where a text names one of the lesson's sources by its index.
pub(super) struct SourceMention {
    pub(super) index: u64,
}

/// Every "Source N" (or "source N") in `text`, as the lesson's writer names
/// a source by its index.
pub(super) fn source_mentions(text: &str) -> Vec<SourceMention> {
    const WORD: &str = "source";
    let mut mentions = Vec::new();
    for (start, _) in text.match_indices(['S', 's']) {
        let Some(word) = text.get(start..start + WORD.len()) else {
            continue;
        };
        let (Some(before), Some(after)) = (text.get(..start), text.get(start + WORD.len()..))
        else {
            continue;
        };
        if !word.eq_ignore_ascii_case(WORD)
            || before
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let number = after.trim_start_matches(' ');
        let digits = number.len()
            - number
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .len();
        let end = text.len() - number.len() + digits;
        if digits == 0 || number.len() == after.len() {
            continue;
        }
        if text
            .get(end..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        if let Some(index) = number.get(..digits).and_then(|digits| digits.parse().ok()) {
            mentions.push(SourceMention { index });
        }
    }
    mentions
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
