//! A claim's corroboration list (course evidence index §5.4, §5.5): one
//! ranking of the collection for the claim, its counter query fused in, cut
//! where the passages stop mentioning what the claim is about.
//!
//! The claim itself is the query; when it gives its subject a value (a
//! number, a month, a name) or negates, a counter query asks for the subject
//! without those values, and its dense and full-text branches join the
//! claim's in the same fusion, so a passage both framings find rises and one
//! only the counter framing finds enters where fusion places it. There is
//! one order: the fused one.
//!
//! The list's end is set by content, never by a count or a score: the last
//! passage that mentions the claim's subject. A claim with no term in the
//! collection has an empty list; one whose every term is in every passage
//! (a course of one short source) cannot be told apart by them, so its list
//! is the whole ranking. Retrieval fetches a depth and
//! doubles it while the fetched tail still mentions the subject and the
//! scope may hold more, so the depth is found, not chosen. Passages before
//! the end stay in fused order whether or not they mention the subject, so
//! a passage the dense branch ranks high without sharing a word is read.
use super::subject::{content_terms, Subject};
use super::*;
use lattice_core::shared::text::{
    capitalised_words, has_digit, padded_lowercase, MONTH_MARKERS, NEGATION_MARKERS,
};

/// The values a sentence gives its subject: numbers written in digits,
/// months and names, and whether it negates.
#[derive(Default)]
struct ValueTerms {
    numbers: HashSet<String>,
    months: HashSet<String>,
    names: HashSet<String>,
    negated: bool,
}

impl ValueTerms {
    /// The values `sentence` gives, read as one sentence: its first word is
    /// never a name.
    fn of(sentence: &str) -> Self {
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

    fn is_empty(&self) -> bool {
        !self.negated && self.numbers.is_empty() && self.months.is_empty() && self.names.is_empty()
    }

    /// Whether `word` is one of these numbers, months or names.
    fn holds(&self, word: &str) -> bool {
        self.numbers.contains(word) || self.months.contains(word) || self.names.contains(word)
    }
}

/// The query for passages that give `claim`'s subject another value, which
/// a query carrying the claim's own values ranks below those repeating them
/// (support-, refute- and neutral-framed queries find different evidence).
/// For a claim that negates, its words without the negation, so passages
/// asserting the positive rank; otherwise its distinctive terms (`subject`)
/// alone. Either way none of its numbers, months or names. `None` for a
/// claim with no value terms, whose counter query would be the claim, and
/// for one with nothing left.
fn counter_query(claim: &str, subject: &Subject) -> Option<String> {
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
        content_terms(claim)
            .filter(|term| subject.has(term) && !values.holds(term))
            .collect()
    };
    let mut seen = HashSet::new();
    let words: Vec<String> = words
        .into_iter()
        .filter(|word| seen.insert(word.clone()))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// One reach's corroboration list for a claim (§5.4).
#[derive(Debug, Clone, Default)]
pub(in crate::features::learning) struct ClaimList {
    /// The fused ranking, widened as verification widens a hit, without what
    /// the claim was shown, up to the last passage on the claim's subject.
    pub passages: Vec<ReferencePassage>,
    /// Whether a counter query's branches were fused into the ranking.
    pub counter: bool,
    /// Whether the scope had passages past the depth the list was found at:
    /// `false` when the list's end is the end of the scope itself.
    pub deeper: bool,
}

fn overlapping(a: &ReferencePassage, b: &ReferencePassage) -> bool {
    a.source_id == b.source_id && a.start_byte < b.end_byte && b.start_byte < a.end_byte
}

impl ReferenceCollection<'_> {
    /// `claim`'s corroboration list over `scope`: the claim and its counter
    /// query ranked together, each hit widened to its surrounding source
    /// text, minus passages overlapping `shown`, ending at the last passage
    /// that mentions the claim's subject. A claim with no term in the
    /// collection has an empty list; one the collection cannot tell its
    /// passages apart by is on every passage. Branch and fused scores at the
    /// list's end are
    /// logged; none of them ends it.
    pub(in crate::features::learning) async fn claim_list(
        &self,
        claim: &str,
        scope: &Scope,
        shown: &[ReferencePassage],
    ) -> Result<ClaimList> {
        let subject = self.subject_of(claim).await?;
        let counter = counter_query(claim, &subject);
        let queries: Vec<&str> = std::iter::once(claim).chain(counter.as_deref()).collect();
        // A collection that cannot tell its passages apart by the claim's
        // terms has every one of them on the subject: its whole ranking.
        let on_subject = |text: &str| subject.undivided() || subject.mentioned_in(text);
        let mut depth = FIRST_PAGE;
        loop {
            let ranked = self.rank(&queries, scope, depth).await?;
            let deeper = ranked.len() >= depth;
            let tail = ranked
                .last()
                .map(|hit| (hit.passage.score, hit.dense, hit.lexical));
            let widened =
                self.verification_context(ranked.into_iter().map(|hit| hit.passage).collect())?;
            // The fetched tail still on the subject: the subject's passages
            // may go on past this depth.
            if deeper
                && widened
                    .last()
                    .is_some_and(|passage| on_subject(&passage.text))
            {
                depth = depth.saturating_mul(2);
                continue;
            }
            let mut passages: Vec<ReferencePassage> = widened
                .into_iter()
                .filter(|passage| !shown.iter().any(|seen| overlapping(seen, passage)))
                .collect();
            let end = passages
                .iter()
                .rposition(|passage| on_subject(&passage.text))
                .map_or(0, |last| last + 1);
            passages.truncate(end);
            tracing::debug!(
                depth,
                passages = passages.len(),
                counter = counter.is_some(),
                first_fused = ?passages.first().map(|passage| passage.score),
                last_fused = ?passages.last().map(|passage| passage.score),
                fetched_tail = ?tail,
                "A claim's corroboration list ends at its last passage on the subject"
            );
            return Ok(ClaimList {
                passages,
                counter: counter.is_some(),
                deeper,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_counter_query_drops_the_claims_values_and_its_negation() {
        let subject = Subject::default();
        assert_eq!(
            counter_query("Retries happen on every request.", &subject),
            None
        );
        assert_eq!(
            counter_query("Retries do not happen on a timeout.", &subject).as_deref(),
            Some("retries do happen on a timeout")
        );
    }
}
