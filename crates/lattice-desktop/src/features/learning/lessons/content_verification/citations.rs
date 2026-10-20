//! The passages a lesson section cites, shown to the judge before retrieved
//! ones when it checks a claim of that section, and the rings a claim's
//! evidence walks outward through (claim evidence pages §4.1): the sources
//! the lesson cites, then the rest of the course's, then every source's
//! next-best passage, with a counter-evidence page from ring 2 on (§6).
use super::*;
use crate::features::learning::reference_collection::{
    above_cut, best_per_source, Gaps, ValueTerms,
};

/// The retrieval kind of a passage a claim's own section cites.
pub(super) const CITED: &str = "cited";
/// The retrieval kind of a passage a claim's counter-evidence query found.
/// The judge's ranking and the canonical order treat it as retrieved.
pub(super) const COUNTER: &str = "counter";

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

    /// `text` with each "Source N" it names written as that source's title,
    /// as `Source "Title"`, so a model reading it beside numbered passages
    /// cannot take the lesson's source index for a passage number.
    pub(super) fn label(&self, text: &str, references: &ReferenceCollection<'_>) -> String {
        let mut labelled = String::with_capacity(text.len());
        let mut copied = 0;
        for mention in source_mentions(text) {
            let Some(title) = self
                .source_at(mention.index)
                .and_then(|id| references.sources.iter().find(|source| source.id == id))
                .map(|source| source.title.trim())
                .filter(|title| !title.is_empty())
            else {
                continue;
            };
            let (Some(before), Some(word)) = (
                text.get(copied..mention.start),
                text.get(mention.start..mention.start + "source".len()),
            ) else {
                continue;
            };
            labelled.push_str(before);
            labelled.push_str(&format!("{word} \"{title}\""));
            copied = mention.end;
        }
        labelled.push_str(text.get(copied..).unwrap_or_default());
        labelled
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

/// How far a claim's walk through the course's sources has gone. Ring 0,
/// the section's own citations, leads every page and has no stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Stage {
    /// The sources the lesson cites, read in depth: each shows its best
    /// passage on the claim's subject not yet shown, page after page, until
    /// none has one left. The writer's own sources are where a claim's
    /// support most likely lies, so they are read through before the rest of
    /// the course.
    Ring1,
    /// Each of the course's other sources does.
    Ring2,
    /// Every source shows its next-best, the lesson's own sources first at
    /// equal rank, until none has one left.
    Deeper,
}

impl Stage {
    fn next(self) -> Option<Self> {
        match self {
            Self::Ring1 => Some(Self::Ring2),
            Self::Ring2 => Some(Self::Deeper),
            Self::Deeper => None,
        }
    }
}

/// One ring's page for a claim.
pub(super) struct RingPage {
    pub(super) passages: Vec<EvidencePassage>,
    /// The stage the passages come from: `Deeper`, with none, once every
    /// ring is exhausted.
    pub(super) stage: Stage,
    /// Whether each ranking's candidates had a clear gap.
    pub(super) gaps: Gaps,
}

/// Whether `source` has shown a passage of its own in `shown`: anything but
/// the section's citations, which are ring 0.
fn had_turn(shown: &[EvidencePassage], source: &str) -> bool {
    shown
        .iter()
        .any(|passage| passage.source_id == source && passage.retrieval_kind != CITED)
}

/// `claim`'s next ring page from `stage` on (§4.1–4.2), skipping a ring with
/// no candidates: of each source the stage walks, the best passage on the
/// claim's subject not yet `shown`, when it lies above its ranking's cut. A
/// source below the cut waits for a later page, where the cut is taken
/// again over what is left, so every page shows something while the ring has
/// candidates. Ring 1 reads the lesson's sources through, so it ends when
/// none has an on-subject passage left. In ring 2 a source shows one passage
/// of its own in all, so the ring ends when each of its sources has shown
/// one or has none left, and the cut is taken over those sources alone: one
/// the ring has finished never holds back one it has not. Deeper pages show
/// every source's next-best on each page.
pub(super) async fn ring_page(
    references: &ReferenceCollection<'_>,
    claim: &str,
    mut stage: Stage,
    shown: &[EvidencePassage],
    citations: &Citations,
) -> Result<RingPage> {
    loop {
        let walked = stage;
        let walks = |source: &str| match walked {
            Stage::Ring1 => citations.cites(source),
            Stage::Ring2 => !citations.cites(source) && !had_turn(shown, source),
            Stage::Deeper => true,
        };
        let candidates = references
            .next_by_source(claim, claim, shown, walks)
            .await?;
        let (above, gaps) = above_cut(candidates);
        let mut passages = best_per_source(above);
        if stage == Stage::Deeper {
            // Equal ranks put the lesson's own sources first; the sort is
            // stable, so the ranked order holds otherwise.
            passages.sort_by(|a, b| {
                b.score.total_cmp(&a.score).then_with(|| {
                    citations
                        .cites(&b.source_id)
                        .cmp(&citations.cites(&a.source_id))
                })
            });
        }
        match stage.next() {
            Some(next) if passages.is_empty() => stage = next,
            _ => {
                return Ok(RingPage {
                    passages,
                    stage,
                    gaps,
                })
            }
        }
    }
}

/// `claim`'s counter-evidence page (§6), carried by every page from ring 2
/// on: for its counter query ([`ReferenceCollection::counter_query`]), each
/// source's best passage on the claim's subject not yet `shown`, above its
/// ranking's cut, those that give the subject another value, or none of the
/// claim's, ahead of those that repeat the claim's values. `None` for a claim
/// with no value terms: its counter query would be its support query, so
/// none is run.
pub(super) async fn counter_page(
    references: &ReferenceCollection<'_>,
    claim: &str,
    shown: &[EvidencePassage],
) -> Result<Option<Vec<EvidencePassage>>> {
    let Some(query) = references.counter_query(claim) else {
        return Ok(None);
    };
    let values = ValueTerms::of(claim);
    let candidates = references
        .next_by_source(&query, claim, shown, |_| true)
        .await?;
    let (mut above, _) = above_cut(candidates);
    // Stable: within each group the ranked order holds.
    above.sort_by_cached_key(|passage| !values.answered_otherwise_by(&passage.text));
    Ok(Some(
        best_per_source(above)
            .into_iter()
            .map(|passage| EvidencePassage {
                retrieval_kind: COUNTER.into(),
                ..passage
            })
            .collect(),
    ))
}

/// Where a text names one of the lesson's sources by its index.
pub(super) struct SourceMention {
    /// The byte span of the mention, as in "Source 2".
    pub(super) start: usize,
    pub(super) end: usize,
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
            mentions.push(SourceMention { start, end, index });
        }
    }
    mentions
}

/// Add `cited` passages not overlapping `evidence` to it, in order, while
/// `claim` checked alone against them fits the judge's window; return the
/// reference passages it now holds. Those that do not fit lead the claim's
/// next page.
pub(super) fn take_cited(
    llm: &dyn LLMPort,
    claim: &str,
    evidence: &mut Vec<EvidencePassage>,
    cited: &[EvidencePassage],
) -> Vec<EvidencePassage> {
    for passage in cited {
        if evidence.iter().any(|shown| overlaps(shown, passage)) {
            continue;
        }
        if !fits_window(llm, claim, evidence.iter().chain(std::iter::once(passage))) {
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
