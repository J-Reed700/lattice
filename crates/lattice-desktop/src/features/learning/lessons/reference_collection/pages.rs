//! A claim's evidence pages (claim evidence pages design §4, §6): the
//! on-subject candidates retrieval ranks for a claim, where their own scores
//! say relevance drops, and the counter-evidence query that looks for another
//! value of the claim's subject.
use super::*;
use lattice_core::shared::text::{
    capitalised_words, has_digit, padded_lowercase, MONTH_MARKERS, NEGATION_MARKERS,
};

/// Function words that match nearly every passage; never a content term.
const FUNCTION_WORDS: &[&str] = &[
    "a", "about", "after", "all", "also", "an", "and", "any", "are", "as", "at", "be", "been",
    "before", "but", "by", "can", "could", "did", "do", "does", "each", "for", "from", "had",
    "has", "have", "how", "if", "in", "into", "is", "it", "its", "may", "more", "most", "must",
    "no", "not", "of", "on", "only", "or", "other", "should", "so", "some", "such", "than", "that",
    "the", "their", "them", "then", "there", "these", "they", "this", "those", "to", "under", "up",
    "use", "used", "uses", "using", "was", "we", "were", "what", "when", "where", "which", "while",
    "who", "will", "with", "without", "would", "you", "your",
];

/// The words of `text` a keyword match should rest on: not function words.
fn content_terms(text: &str) -> impl Iterator<Item = String> + '_ {
    terms_iter(text).filter(|word| !FUNCTION_WORDS.contains(&word.as_str()))
}

fn overlapping(a: &ReferencePassage, b: &ReferencePassage) -> bool {
    a.source_id == b.source_id && a.start_byte < b.end_byte && b.start_byte < a.end_byte
}

/// The stretches of `source` that no passage of `shown` covers: where a
/// passage not yet shown can only lie.
fn unshown_text<'s>(source: &'s LearningSourceDto, shown: &[ReferencePassage]) -> Vec<&'s str> {
    let mut covered: Vec<(usize, usize)> = shown
        .iter()
        .filter(|passage| passage.source_id == source.id)
        .map(|passage| (passage.start_byte, passage.end_byte))
        .collect();
    covered.sort_unstable();
    let mut stretches = Vec::new();
    let mut from = 0;
    for (start, end) in covered {
        if start > from {
            stretches.extend(source.excerpt.get(from..start));
        }
        from = from.max(end);
    }
    stretches.extend(source.excerpt.get(from..));
    stretches
}

impl ReferenceCollection<'_> {
    /// Whether `text` mentions what `query` is about: one of the query's
    /// distinctive content terms, those no more common across this
    /// collection's passages than the median of the query's own content
    /// terms. Words the whole subject shares (in a .NET collection, "net" or
    /// "core") do not count, so a walk through deeper evidence ends once only
    /// the subject's common vocabulary is left. Vector neighbours alone never
    /// count. Before the median is taken, two kinds of query term are left
    /// out because no page can tell anything by them: a term in no passage of
    /// the collection, such as a name a lesson invented for its example (no
    /// page can contain it, so it neither keeps a walk going nor lowers the
    /// bar the real terms must meet), and a term in every passage. When every
    /// term the query shares with the collection is in every passage, as in a
    /// collection of one passage, those terms are all it has: a text with one
    /// of them is as much about the subject as any passage can be.
    pub(in crate::features::learning) fn mentions_subject_of(
        &self,
        query: &str,
        text: &str,
    ) -> bool {
        mentions(&self.distinctive_terms(query), text)
    }

    /// The distinctive content terms of `query` (see
    /// [`Self::mentions_subject_of`]).
    fn distinctive_terms(&self, query: &str) -> HashSet<String> {
        let occurring: Vec<(usize, String)> = content_terms(query)
            .filter_map(|term| {
                let frequency = self.subject_frequencies.get(&term).copied().unwrap_or(0);
                (frequency > 0).then_some((frequency, term))
            })
            .collect();
        let mut wanted: Vec<(usize, String)> = occurring
            .iter()
            .filter(|(frequency, _)| *frequency < self.subject_passages)
            .cloned()
            .collect();
        if wanted.is_empty() {
            wanted = occurring;
        }
        wanted.sort();
        wanted.dedup();
        let Some(&(median, _)) = wanted.get(wanted.len().saturating_sub(1) / 2) else {
            return HashSet::new();
        };
        wanted
            .into_iter()
            .filter(|(frequency, _)| *frequency <= median)
            .map(|(_, term)| term)
            .collect()
    }

    /// The candidates for a claim's next evidence page (§4.2): the passages
    /// retrieval ranks for `query` in the sources `within` accepts, best
    /// first, each widened as verification widens a passage, that overlap
    /// nothing in `shown` and mention the subject of `subject`, the claim (a
    /// counter query ranks for the claim but is not it). Each keeps its
    /// branch score, so the page can end where those scores drop. Retrieval
    /// pages deeper, doubling, until every accepted source whose unshown text
    /// mentions the subject has a candidate or the ranking is exhausted; a
    /// source whose unshown text never mentions it cannot give one, so it is
    /// not waited for.
    pub(in crate::features::learning) async fn next_by_source(
        &self,
        query: &str,
        subject: &str,
        shown: &[ReferencePassage],
        within: impl Fn(&str) -> bool,
    ) -> Result<Vec<ReferencePassage>> {
        let distinctive = self.distinctive_terms(subject);
        let awaited: HashSet<&str> = self
            .sources
            .iter()
            .filter(|source| {
                within(&source.id)
                    && unshown_text(source, shown)
                        .into_iter()
                        .any(|text| mentions(&distinctive, text))
            })
            .map(|source| source.id.as_str())
            .collect();
        let mut depth = FIRST_PAGE;
        loop {
            let hits = self.retrieve(query, depth).await?;
            let exhausted = hits.len() < depth;
            let candidates: Vec<_> = self
                .verification_context(
                    hits.into_iter()
                        .filter(|hit| within(&hit.source_id))
                        .collect(),
                )?
                .into_iter()
                .filter(|passage| {
                    !shown.iter().any(|seen| overlapping(seen, passage))
                        && mentions(&distinctive, &passage.text)
                })
                .collect();
            let found: HashSet<&str> = candidates
                .iter()
                .map(|passage| passage.source_id.as_str())
                .collect();
            if exhausted || awaited.iter().all(|source| found.contains(source)) {
                return Ok(candidates);
            }
            depth = depth.saturating_mul(2);
        }
    }

    /// The query for a claim's counter-evidence (§6): passages that give its
    /// subject another value, which a query carrying the claim's own values
    /// ranks below those that repeat them. For a claim that negates, its
    /// words without the negation, so passages asserting the positive rank;
    /// otherwise its subject terms alone. Either way none of its numbers,
    /// months or names. `None` for a claim with no value terms, whose counter
    /// query would be its support query, and for one with nothing left.
    pub(in crate::features::learning) fn counter_query(&self, claim: &str) -> Option<String> {
        let values = ValueTerms::of(claim);
        if values.is_empty() {
            return None;
        }
        let words: Vec<String> = if values.negated {
            let mut positive = padded_lowercase(claim);
            while let Some(marker) = NEGATION_MARKERS
                .iter()
                .find(|marker| positive.contains(**marker))
            {
                positive = positive.replacen(marker, " ", 1);
            }
            positive
                .split_whitespace()
                .filter(|word| !values.holds(word))
                .map(str::to_owned)
                .collect()
        } else {
            let subject = self.distinctive_terms(claim);
            content_terms(claim)
                .filter(|term| subject.contains(term) && !values.holds(term))
                .collect()
        };
        let mut seen = HashSet::new();
        let words: Vec<String> = words
            .into_iter()
            .filter(|word| seen.insert(word.clone()))
            .collect();
        (!words.is_empty()).then(|| words.join(" "))
    }
}

/// Whether `text` has one of the `distinctive` terms.
fn mentions(distinctive: &HashSet<String>, text: &str) -> bool {
    !distinctive.is_empty() && content_terms(text).any(|term| distinctive.contains(&term))
}

/// Where relevance drops most in one branch's candidate scores, sorted best
/// first: how many lie above the largest gap between neighbours (Adaptive-k).
/// The page ends there whether or not the drop stands out; what lies below
/// waits for a later page, where the cut is taken again over what is left.
/// Equal scores cut after the first. `None` with fewer than two scores:
/// nothing in them ends the page.
pub(in crate::features::learning) fn gap_cut(scores: &[f64]) -> Option<usize> {
    largest_gap(scores).map(|(at, _)| at + 1)
}

/// Whether the largest gap in `scores` is clear: larger than all the other
/// gaps together, one drop more than half the list's whole descent. A long
/// flat tail adds little to the descent, so a drop at the head still
/// dominates it, and a drop deep in the list dominates only when the head
/// above it is flat. Recorded per page so a run shows how often the scores
/// set the page's end plainly; it never changes the cut.
pub(in crate::features::learning) fn gap_is_clear(scores: &[f64]) -> bool {
    match (scores.first(), scores.last(), largest_gap(scores)) {
        (Some(first), Some(last), Some((_, largest))) => largest > (first - last) - largest,
        _ => false,
    }
}

/// The position and size of the largest drop between neighbours in
/// `scores`, sorted best first; the first of equal drops.
fn largest_gap(scores: &[f64]) -> Option<(usize, f64)> {
    scores
        .windows(2)
        .map(|pair| match pair {
            [higher, lower] => higher - lower,
            _ => 0.0,
        })
        .enumerate()
        .fold(None, |best: Option<(usize, f64)>, (at, gap)| match best {
            Some((_, largest)) if largest >= gap => best,
            _ => Some((at, gap)),
        })
}

/// Whether each branch's candidates on a page had a clear gap: `None` when
/// the branch had no candidates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::features::learning) struct Gaps {
    pub(in crate::features::learning) library: Option<bool>,
    pub(in crate::features::learning) keyword: Option<bool>,
}

/// The `candidates` above their branch's cut ([`gap_cut`]), in their ranked
/// order, and whether each branch's gap was clear ([`gap_is_clear`]). Each
/// branch is cut on its own scores, which are not comparable with the
/// other's. A passage no branch ranked is never cut.
pub(in crate::features::learning) fn above_cut(
    candidates: Vec<ReferencePassage>,
) -> (Vec<ReferencePassage>, Gaps) {
    let mut gaps = Gaps::default();
    let mut floors: HashMap<Branch, f64> = HashMap::new();
    for branch in [Branch::Library, Branch::Keyword] {
        let mut scores: Vec<f64> = candidates
            .iter()
            .filter(|passage| passage.branch == Some(branch))
            .map(|passage| passage.branch_score)
            .collect();
        if scores.is_empty() {
            continue;
        }
        scores.sort_by(|a, b| b.total_cmp(a));
        let cut = gap_cut(&scores);
        let clear = Some(gap_is_clear(&scores));
        match branch {
            Branch::Library => gaps.library = clear,
            Branch::Keyword => gaps.keyword = clear,
        }
        if let Some(floor) = cut.and_then(|cut| scores.get(cut.saturating_sub(1))) {
            floors.insert(branch, *floor);
        }
    }
    let above = candidates
        .into_iter()
        .filter(|passage| {
            passage
                .branch
                .and_then(|branch| floors.get(&branch))
                .is_none_or(|floor| passage.branch_score >= *floor)
        })
        .collect();
    (above, gaps)
}

/// The first passage of each source in `passages`, in their order: a page
/// shows one passage per source (§4.1).
pub(in crate::features::learning) fn best_per_source(
    passages: Vec<ReferencePassage>,
) -> Vec<ReferencePassage> {
    let mut seen = HashSet::new();
    passages
        .into_iter()
        .filter(|passage| seen.insert(passage.source_id.clone()))
        .collect()
}

/// The values a text gives its subject (§6): numbers written in digits,
/// months and names, and whether it negates.
#[derive(Default)]
pub(in crate::features::learning) struct ValueTerms {
    numbers: HashSet<String>,
    months: HashSet<String>,
    names: HashSet<String>,
    negated: bool,
}

impl ValueTerms {
    /// The values `sentence` gives, read as one sentence: its first word is
    /// never a name.
    pub(in crate::features::learning) fn of(sentence: &str) -> Self {
        let padded = padded_lowercase(sentence);
        Self {
            numbers: padded
                .split_whitespace()
                .filter(|word| has_digit(word))
                .map(str::to_owned)
                .collect(),
            months: MONTH_MARKERS
                .iter()
                .filter(|month| padded.contains(**month))
                .map(|month| month.trim().to_owned())
                .collect(),
            names: capitalised_words(sentence).collect(),
            negated: NEGATION_MARKERS
                .iter()
                .any(|marker| padded.contains(*marker)),
        }
    }

    /// The values a passage gives, sentence by sentence.
    fn of_passage(text: &str) -> Self {
        let mut values = Self::default();
        for sentence in text.split(['.', '!', '?', '\n']).map(Self::of) {
            values.numbers.extend(sentence.numbers);
            values.months.extend(sentence.months);
            values.names.extend(sentence.names);
            values.negated |= sentence.negated;
        }
        values
    }

    fn is_empty(&self) -> bool {
        !self.negated && self.numbers.is_empty() && self.months.is_empty() && self.names.is_empty()
    }

    /// Whether `word` is one of these numbers, months or names.
    fn holds(&self, word: &str) -> bool {
        self.numbers.contains(word) || self.months.contains(word) || self.names.contains(word)
    }

    /// Whether `passage` gives the claim's subject another value of a class
    /// the claim gives one of (another number, month or name), or none of
    /// the claim's values: what a counter page leads with. A passage that
    /// only repeats the claim's values is what its support query finds.
    pub(in crate::features::learning) fn answered_otherwise_by(&self, passage: &str) -> bool {
        let found = Self::of_passage(passage);
        let classes = [
            (&self.numbers, &found.numbers),
            (&self.months, &found.months),
            (&self.names, &found.names),
        ];
        let repeats = classes
            .iter()
            .any(|(claimed, found)| !claimed.is_disjoint(found));
        !repeats
            || classes
                .iter()
                .any(|(claimed, found)| !claimed.is_empty() && !found.is_subset(claimed))
    }
}
