//! The passages a lesson section cites, which pass A checks a claim of that
//! section against and nothing else (course evidence index §5.1), and the
//! sources the lesson cites anywhere, which pass B can read first when the
//! order of its reaches says so (§5.2).
use super::*;

/// The retrieval kind of a passage a claim's own section cites.
pub(super) const CITED: &str = "cited";

/// What a lesson cites. Per section (unit), the source passages it cites:
/// each block's and question's quote resolved to its span in the saved
/// references and widened like a retrieved passage. A claim's pass A reads
/// its own section's.
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

    /// The source versions the lesson cites anywhere.
    pub(super) fn sources(&self) -> &HashSet<String> {
        &self.sources
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

/// Add `cited` passages not overlapping `evidence` to it, in citation
/// order, while a check of `claim` against them alone fits the window: pass
/// A's page, which needs no budget. Return the reference passages it now
/// holds.
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
