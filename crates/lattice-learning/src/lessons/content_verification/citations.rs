//! The passages a lesson section cites, which a claim of that section is
//! checked against by its own citation and nothing else (course evidence index
//! §5.1), and the sources the lesson cites anywhere. A sentence the writer cited has its own,
//! which the claim standing on that sentence is read against instead (claim
//! classes design §3.5).
use super::*;
use lattice_core::shared::text::same_text::holds_sentence;

/// A sentence the writer cited, with the passages its quote resolves to.
struct CitedSentence {
    unit: usize,
    sentence: String,
    passages: Vec<EvidencePassage>,
}

/// What a lesson cites. Per section (unit), the source passages it cites:
/// each block's and question's quote, and the quote of each sentence listed in
/// its citations, resolved to its span in the saved references and widened
/// like a retrieved passage. A claim's own citation reads those of the
/// sentence it stands on, its quote, when the writer cited one, otherwise its
/// section's.
#[derive(Default)]
pub(super) struct Citations {
    by_unit: HashMap<usize, Vec<EvidencePassage>>,
    sentences: Vec<CitedSentence>,
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
            sentences: Vec::new(),
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

    /// Whether the writer cited the sentence `sentence` of section `unit`, the
    /// one a claim stands on, its quote.
    pub(super) fn is_cited(&self, unit: usize, sentence: &str) -> bool {
        self.sentences
            .iter()
            .any(|cited| cited.unit == unit && holds_sentence(&cited.sentence, sentence))
    }

    /// The passages the claim standing on `sentence`, its quote, of section
    /// `unit` is read against: those the entries holding that sentence cite,
    /// and when the writer cited none, the section's.
    pub(super) fn of_claim(&self, unit: usize, sentence: &str) -> Vec<EvidencePassage> {
        let mut own = Vec::new();
        for cited in &self.sentences {
            if cited.unit == unit && holds_sentence(&cited.sentence, sentence) {
                merge(&mut own, &cited.passages);
            }
        }
        if own.is_empty() {
            self.of(unit).to_vec()
        } else {
            own
        }
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
    let mut sentences = Vec::new();
    let items = ["blocks", "questions"].into_iter().flat_map(|field| {
        candidate
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
    });
    for (unit, item) in items.enumerate() {
        let mut passages = resolve(references, &mut by_index, item)?;
        for entry in item
            .get("citations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let own = resolve(references, &mut by_index, entry)?;
            merge(&mut passages, &own);
            if let (Some(sentence), false) = (
                entry.get("sentence").and_then(Value::as_str),
                own.is_empty(),
            ) {
                sentences.push(CitedSentence {
                    unit,
                    sentence: sentence.to_owned(),
                    passages: own,
                });
            }
        }
        if !passages.is_empty() {
            citations.insert(unit, passages);
        }
    }
    let mut cited = Citations::new(citations, by_index);
    cited.sentences = sentences;
    Ok(cited)
}

/// The passages the quote of a block, a question or a cited sentence resolves
/// to, noting the source its `sourceIndex` stands for.
fn resolve(
    references: &ReferenceCollection<'_>,
    by_index: &mut HashMap<u64, String>,
    part: &Value,
) -> Result<Vec<EvidencePassage>> {
    let Some(quote) = part.get("quote").and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    let passages = references.cited_passages(&[quote])?;
    if let (Some(index), Some(passage)) = (
        part.get("sourceIndex").and_then(Value::as_u64),
        passages.first(),
    ) {
        by_index
            .entry(index)
            .or_insert_with(|| passage.source_id.clone());
    }
    Ok(passages)
}

/// Add to `into` the `passages` that overlap none it holds.
fn merge(into: &mut Vec<EvidencePassage>, passages: &[EvidencePassage]) {
    for passage in passages {
        if !into.iter().any(|held| held.overlaps(passage)) {
            into.push(passage.clone());
        }
    }
}

/// Where a text names one of the lesson's sources by its index.
pub(super) struct SourceMention {
    /// The byte span of the mention, as in "Source 2".
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) index: u64,
}

/// Whether `text` names a source by its index, as "Source 2".
pub(crate) fn names_a_source(text: &str) -> bool {
    !source_mentions(text).is_empty()
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
/// order, while the judge that reads them still `fits` a check of `claim`
/// against them: the page of a claim's own citation, which needs no budget.
/// Return the reference passages it now holds.
pub(super) fn take_cited(
    fits: impl Fn(&str, &[EvidencePassage]) -> bool,
    claim: &str,
    evidence: &mut Vec<EvidencePassage>,
    cited: &[EvidencePassage],
) -> Vec<EvidencePassage> {
    for passage in cited {
        if evidence.iter().any(|shown| shown.overlaps(passage)) {
            continue;
        }
        evidence.push(passage.clone());
        if !fits(claim, evidence.as_slice()) {
            evidence.pop();
            break;
        }
    }
    evidence
        .iter()
        .filter(|passage| !passage.is_run())
        .cloned()
        .collect()
}
