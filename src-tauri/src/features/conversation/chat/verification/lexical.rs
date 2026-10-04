//! Lexical grounding pre-filter.
//!
//! Stemmed token overlap between each claim sentence and the passages it cites.
//! Cheap and deterministic, so it runs first on every turn and decides which
//! sentences are worth an LLM call. It cannot read meaning: a sentence that
//! reuses the source's vocabulary while inverting its meaning still scores
//! high, which is why numbers, dates and negations are escalated to the judge
//! regardless of overlap.

pub(super) use crate::application::services::evidence_retrieval::{best_windows, ScoredWindow};
use crate::application::services::evidence_retrieval::{
    extract_normalized_tokens, passage_windows,
};
use crate::features::qa::dto::SourceDto;
use crate::shared::text::normalize_whitespace;
use std::collections::HashSet;

const MIN_CLAIM_CHARS: usize = 24;
pub(super) const MIN_CLAIM_TOKENS: usize = 5;
pub(super) const MIN_OVERLAP_RATIO: f32 = 0.32;
pub(super) const MIN_MATCHING_TOKENS: usize = 2;

/// Overlap at or above this ratio, with enough matching tokens, is treated as
/// settled by the lexical pass alone. Well clear of `MIN_OVERLAP_RATIO` so the
/// judge still sees everything in the ambiguous band just above threshold.
pub(super) const STRONG_OVERLAP_RATIO: f32 = 0.55;
pub(super) const STRONG_MATCHING_TOKENS: usize = 4;

/// One sentence the lexical pass considered a claim.
#[derive(Debug, Clone)]
pub(super) struct LexicalClaim {
    /// Sentence as written, citation markers included.
    pub(super) sentence: String,
    /// Sentence with `[n]` markers removed — the text the judge reads.
    pub(super) claim_text: String,
    /// Citation numbers exactly as the model wrote them (1-based).
    pub(super) citation_ids: Vec<u32>,
    /// Zero-based source indices those citation numbers resolve to.
    pub(super) cited_source_indices: Vec<usize>,
    pub(super) overlap_ratio: f32,
    pub(super) matching_tokens: usize,
    /// The lexical verdict: enough overlap to call this grounded.
    pub(super) supported: bool,
    /// Carries a number, date or negation, so vocabulary overlap cannot settle it.
    pub(super) contradiction_prone: bool,
}

/// Score every claim sentence in `response` against `sources`.
///
/// Returns one entry per sentence that cleared the claim filters; sentences
/// that are too short, are questions, or are boilerplate never appear.
pub(super) fn lexical_pass(response: &str, sources: &[SourceDto]) -> Vec<LexicalClaim> {
    if response.trim().is_empty() || sources.is_empty() {
        return Vec::new();
    }

    let source_token_sets = collect_source_token_sets(sources);
    if source_token_sets.is_empty() {
        return Vec::new();
    }

    let mut claims = Vec::new();
    for (sentence, citation_ids) in sentences_with_citations(response) {
        if !is_claim_candidate(&sentence) {
            continue;
        }

        let cited_source_indices: Vec<usize> = citation_ids
            .iter()
            .filter_map(|id| {
                let mut matches = sources
                    .iter()
                    .enumerate()
                    .filter(|(idx, source)| source.citation_id.unwrap_or((*idx + 1) as u32) == *id);
                let (idx, _) = matches.next()?;
                matches.next().is_none().then_some(idx)
            })
            .collect();
        let claim_text = strip_citation_markers(&sentence);
        let claim_tokens = extract_normalized_tokens(&claim_text);
        if claim_tokens.is_empty()
            || (citation_ids.is_empty() && claim_tokens.len() < MIN_CLAIM_TOKENS)
        {
            continue;
        }

        let citations_valid = citation_ids.len() == cited_source_indices.len();
        let (overlap_ratio, matching_tokens) = if !citations_valid {
            (0.0, 0)
        } else if citation_ids.is_empty() {
            compute_best_overlap_ratio(&claim_tokens, source_token_sets.iter())
        } else {
            compute_best_overlap_ratio(
                &claim_tokens,
                cited_source_indices
                    .iter()
                    .filter_map(|idx| source_token_sets.get(*idx)),
            )
        };
        let supported =
            matching_tokens >= MIN_MATCHING_TOKENS && overlap_ratio >= MIN_OVERLAP_RATIO;

        claims.push(LexicalClaim {
            sentence: claim_for_note(&sentence),
            contradiction_prone: is_contradiction_prone(&claim_text),
            claim_text: claim_for_note(&claim_text),
            citation_ids,
            cited_source_indices,
            overlap_ratio,
            matching_tokens,
            supported,
        });
    }

    claims
}

/// Whether this claim is worth an LLM call.
///
/// Strong overlap settles a plain sentence. It never settles one carrying a
/// number, date or negation: those are exactly the claims that echo a source's
/// wording while stating the opposite of it.
///
/// A sentence with no citation is never escalated. The judge rules a claim
/// against the passages it cites, and a sentence that cites nothing has none —
/// the only evidence to hand it would be whichever sources happened to rank
/// first, which it was never making a statement about. Asking a judge to rule
/// on unrelated text does not produce "unverified", it produces a verdict, and
/// a closing line that summarises the cited ones has been marked contradicted
/// on the strength of passages from an entirely different document.
pub(super) fn needs_judge(claim: &LexicalClaim) -> bool {
    if claim.citation_ids.is_empty() {
        return false;
    }
    let strongly_supported = claim.supported
        && claim.overlap_ratio >= STRONG_OVERLAP_RATIO
        && claim.matching_tokens >= STRONG_MATCHING_TOKENS;
    !strongly_supported || claim.contradiction_prone
}

/// Tokens that flip a sentence's meaning without changing its vocabulary.
const NEGATION_MARKERS: &[&str] = &[
    " not ",
    "n't ",
    " no ",
    " never ",
    " without ",
    " cannot ",
    " none ",
    " neither ",
    " nor ",
    " unlike ",
    " instead of ",
    " rather than ",
    " fails to ",
    " failed to ",
    " unable to ",
    " only ",
    " except ",
];

const MONTH_MARKERS: &[&str] = &[
    " january ",
    " february ",
    " march ",
    " april ",
    " june ",
    " july ",
    " august ",
    " september ",
    " october ",
    " november ",
    " december ",
];

fn is_contradiction_prone(claim: &str) -> bool {
    if claim.chars().any(|ch| ch.is_ascii_digit()) {
        return true;
    }

    // Pad and flatten punctuation so " not " matches "did not." as well.
    let mut padded = String::with_capacity(claim.len() + 2);
    padded.push(' ');
    for ch in claim.chars() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '\u{2019}' {
            padded.extend(ch.to_lowercase());
        } else {
            padded.push(' ');
        }
    }
    padded.push(' ');

    NEGATION_MARKERS
        .iter()
        .chain(MONTH_MARKERS.iter())
        .any(|marker| padded.contains(marker))
}

/// A full stop ends a sentence only when the text pauses after it. One inside
/// a token ("1.2 °C", "v2.1", "example.com") does not, and splitting there cut
/// every sentence with a decimal in half: the claims most worth checking were
/// judged as two fragments, neither of which says what the sentence says.
/// Every sentence with the citations it answers to.
///
/// A marker covers the point it closes, not only the sentence it sits in:
/// "…LED lights are the stated workaround [11]. So 'a lot of sun' becomes
/// '6–8 hours of strong light'." is one cited point written as two sentences.
/// Scored alone, the second read as uncited and the answer said "No source
/// cited for this" beside a visible `[11]`. So a sentence without a marker of
/// its own takes the next marker in its paragraph or list item, or failing
/// that the previous one. Its line is the boundary: a heading or a paragraph
/// with no marker at all stays uncited, which is what it is.
fn sentences_with_citations(response: &str) -> Vec<(String, Vec<u32>)> {
    let mut out = Vec::new();
    for block in response.split('\n') {
        let block = strip_emphasis_markers(block);
        let sentences = split_sentences(&block);
        let own: Vec<Vec<u32>> = sentences
            .iter()
            .map(|sentence| extract_sentence_citation_ids(sentence))
            .collect();
        for (index, sentence) in sentences.into_iter().enumerate() {
            let citations = own
                .get(index)
                .filter(|ids| !ids.is_empty())
                .or_else(|| own.iter().skip(index + 1).find(|ids| !ids.is_empty()))
                .or_else(|| own.iter().take(index).rev().find(|ids| !ids.is_empty()))
                .cloned()
                .unwrap_or_default();
            out.push((sentence, citations));
        }
    }
    out
}

/// Drop markdown emphasis so it cannot be read as sentence structure.
///
/// [`split_sentences`] breaks after a full stop whose next character is not
/// alphanumeric, and a bolded opening sentence closes *after* its full stop:
/// `**Bury it all the way.** The ideal depth is 4–6 inches [24].` split at the
/// stop inside the emphasis, leaving one claim cut off before its closing `**`
/// and the next one beginning `** The ideal depth…`. Both went to the judge
/// malformed, and the stray marker was shown to the user in the verification
/// panel as part of the claim.
///
/// Asterisk runs always go. Underscores go only when they are not inside a
/// word, so `snake_case` in a quoted identifier survives.
fn strip_emphasis_markers(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut idx = 0usize;

    while let Some(&current) = chars.get(idx) {
        if current == '*' || current == '_' {
            let mut end = idx;
            while chars.get(end) == Some(&current) {
                end += 1;
            }
            let run = end - idx;
            let inside_word = current == '_'
                && idx > 0
                && chars
                    .get(idx - 1)
                    .is_some_and(|previous| previous.is_alphanumeric())
                && chars.get(end).is_some_and(|next| next.is_alphanumeric());
            // Four or more is a horizontal rule or ASCII art, not emphasis.
            // Either way the whole run is consumed here: stepping one character
            // into a kept run would re-measure the tail as a shorter one and
            // strip that instead, turning `____` into `_`.
            if run > 3 || inside_word {
                out.extend(std::iter::repeat_n(current, run));
            }
            idx = end;
            continue;
        }
        out.push(current);
        idx += 1;
    }

    out
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        current.push(ch);
        let pauses = chars.peek().is_none_or(|next| !next.is_alphanumeric());
        if ch == '\n' || (matches!(ch, '.' | '!' | '?') && pauses) {
            let candidate = current.trim();
            if !candidate.is_empty() {
                sentences.push(candidate.to_string());
            }
            current.clear();
        }
    }

    let trailing = current.trim();
    if !trailing.is_empty() {
        sentences.push(trailing.to_string());
    }

    sentences
}

fn is_claim_candidate(sentence: &str) -> bool {
    let s = sentence.trim();
    if s.len() < MIN_CLAIM_CHARS && extract_sentence_citation_ids(s).is_empty() {
        return false;
    }
    if s.ends_with('?') {
        return false;
    }
    // A sentence that points at lines in an Explorer folder (`src/main.rs:10`)
    // is about the file on the user's screen, not about the retrieved
    // passages, so judging it against them can only produce a false verdict.
    if crate::features::explorer::line_refs::contains_line_reference(s) {
        return false;
    }
    // The app's own lines, appended when the model hit its output limit or ran
    // out of tool rounds. The splitter stops at a full stop, so compare the
    // words, not the wrapper — and the emphasis around them is gone by now, so
    // match either form.
    let app_note = |note: &'static str| {
        note.trim()
            .trim_start_matches('_')
            .trim_start_matches('(')
            .trim_end_matches('_')
            .trim_end_matches(')')
            .trim_end_matches('.')
    };
    let unwrapped = s.trim_start_matches('_').trim_start_matches('(');
    if [
        super::super::tool_loop::CUT_SHORT_NOTE,
        super::super::tool_loop::OUT_OF_ROUNDS_NOTE,
    ]
    .into_iter()
    .any(|note| unwrapped.starts_with(app_note(note)))
    {
        return false;
    }

    let lowercase = s.to_lowercase();
    if lowercase.starts_with("verification note:")
        || lowercase.starts_with("sources:")
        || lowercase.starts_with("reference:")
        || lowercase.starts_with("relevance")
    {
        return false;
    }

    true
}

fn claim_for_note(claim: &str) -> String {
    normalize_whitespace(claim).trim().to_string()
}

fn strip_citation_markers(sentence: &str) -> String {
    let mut out = String::with_capacity(sentence.len());
    let chars: Vec<char> = sentence.chars().collect();
    let mut idx = 0usize;

    while idx < chars.len() {
        let Some(&current) = chars.get(idx) else {
            break;
        };
        if current == '[' {
            let mut j = idx + 1;
            let mut saw_digit = false;
            while chars.get(j).is_some_and(|ch| ch.is_ascii_digit()) {
                saw_digit = true;
                j += 1;
            }
            if saw_digit && chars.get(j) == Some(&']') {
                idx = j + 1;
                continue;
            }
        }

        out.push(current);
        idx += 1;
    }

    out
}

/// Preserve invalid numbers: silently dropping them would turn a fabricated
/// citation into an uncited claim and let it borrow evidence from other sources.
fn extract_sentence_citation_ids(sentence: &str) -> Vec<u32> {
    let chars: Vec<char> = sentence.chars().collect();
    let mut idx = 0usize;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    while idx < chars.len() {
        if chars.get(idx) != Some(&'[') {
            idx += 1;
            continue;
        }

        let mut j = idx + 1;
        let mut digits = String::new();
        while let Some(ch) = chars.get(j).filter(|ch| ch.is_ascii_digit()) {
            digits.push(*ch);
            j += 1;
        }

        if digits.is_empty() || chars.get(j) != Some(&']') {
            idx += 1;
            continue;
        }

        let raw_index = digits.parse::<u32>().unwrap_or(0);
        if seen.insert(raw_index) {
            out.push(raw_index);
        }

        idx = j + 1;
    }

    out
}

/// Characters per scoring window over a long passage.
///
/// A cited web page arrives whole, and one token set over a whole article
/// matches almost any sentence on its topic. Scoring against windows asks
/// instead whether one stretch of the page says what the claim says.
/// The sentence of `text` that shares the most vocabulary with `claim`.
///
/// Shown beside a verdict as the words it rests on. Taken from the passage
/// rather than asked of the model: a one-word judge cannot quote, and a quote
/// a model writes has to be checked against the page anyway.
pub(super) fn best_sentence(claim: &str, text: &str) -> Option<String> {
    let claim_tokens = extract_normalized_tokens(claim);
    let mut best: Option<(usize, String)> = None;
    for sentence in split_sentences(&normalize_whitespace(text)) {
        let score = claim_tokens
            .intersection(&extract_normalized_tokens(&sentence))
            .count();
        if score > 0 && best.as_ref().is_none_or(|(top, _)| score > *top) {
            best = Some((score, sentence));
        }
    }
    best.map(|(_, sentence)| sentence)
}

/// One entry per source, each a token set per window of its text. Title, path
/// and excerpt tokens are shared by every window of their source.
fn collect_source_token_sets(sources: &[SourceDto]) -> Vec<Vec<HashSet<String>>> {
    let mut sets = Vec::new();
    for source in sources {
        let mut label = String::new();
        label.push_str(&source.file_name);
        label.push(' ');
        label.push_str(&source.file_path);
        label.push(' ');
        if let Some(path) = source.path.as_deref() {
            label.push_str(path);
            label.push(' ');
        }
        if let Some(excerpt) = source.excerpt.as_deref() {
            label.push_str(excerpt);
            label.push(' ');
        }
        if let Some(excerpts) = source.chunk_excerpts.as_ref() {
            for excerpt in excerpts {
                label.push_str(&excerpt.excerpt);
                label.push(' ');
            }
        }
        let label_tokens = extract_normalized_tokens(&label);

        // Keep one entry per source; dropping an empty passage shifts citation indices.
        sets.push(
            passage_windows(&source.content)
                .iter()
                .map(|window| {
                    let mut tokens = extract_normalized_tokens(window);
                    tokens.extend(label_tokens.iter().cloned());
                    tokens
                })
                .collect(),
        );
    }
    sets
}

fn compute_best_overlap_ratio<'a>(
    claim_tokens: &HashSet<String>,
    source_token_sets: impl IntoIterator<Item = &'a Vec<HashSet<String>>>,
) -> (f32, usize) {
    let mut best_ratio = 0.0f32;
    let mut best_match_count = 0usize;

    for source_tokens in source_token_sets.into_iter().flatten() {
        let match_count = claim_tokens.intersection(source_tokens).count();
        if claim_tokens.is_empty() {
            continue;
        }
        let ratio = match_count as f32 / claim_tokens.len() as f32;
        if ratio > best_ratio || (ratio == best_ratio && match_count > best_match_count) {
            best_ratio = ratio;
            best_match_count = match_count;
        }
    }

    (best_ratio, best_match_count)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::source;
    use super::*;

    #[test]
    fn a_claim_deep_in_a_whole_cited_page_is_found_but_a_scattered_one_is_not() {
        // The page the model read, not the search snippet: the supporting
        // sentence sits thousands of characters in.
        let page = format!(
            "{} Bush beans mature within sixty days and grow well in shallow window boxes. {} {}",
            "Notes on compost, mulch and seed trays. ".repeat(60),
            "Trellis spacing and irrigation schedules. ".repeat(60),
            "Pollination needs wind or hand shaking for corn. ".repeat(10),
        );
        let claims = lexical_pass(
            "Bush beans mature within sixty days in shallow window boxes [1].",
            &[source(&page)],
        );
        assert!(claims[0].supported);

        // Every word appears somewhere on the page, but no one stretch of it
        // says this; a single token set over the whole article would pass it.
        let claims = lexical_pass(
            "Compost pollination needs trellis irrigation seed windows [1].",
            &[source(&page)],
        );
        assert!(claims[0].overlap_ratio < 1.0);
    }

    #[test]
    fn a_follow_on_sentence_answers_to_the_marker_of_its_own_point() {
        let sources = [
            source(
                "High-calorie fruiting crops need 6–8 hours of strong light; use LED grow lights.",
            ),
            source("Thyme is tough indoors while rosemary wants direct sunlight."),
        ];
        let claims = lexical_pass(
            "- Fruiting crops need 6–8 hours of strong light, and LED grow lights are the workaround [1]. \
             So needing a lot of sun really means needing strong light for hours.\n\
             - Thyme copes indoors but rosemary wants direct sunlight [2].\n\
             ## A heading that says something about peanut pods underground\n\
             Peanuts develop pods underground, which is my inference and cites nothing at all.",
            &sources,
        );
        let follow_on = claims
            .iter()
            .find(|claim| claim.sentence.starts_with("So needing"))
            .expect("follow-on sentence is a claim");
        // The next marker is on another line, so it cannot claim this one.
        assert_eq!(follow_on.citation_ids, vec![1]);
        let aside = claims
            .iter()
            .find(|claim| claim.sentence.starts_with("Peanuts"))
            .expect("uncited paragraph is a claim");
        assert!(aside.citation_ids.is_empty());
    }

    #[test]
    fn a_bolded_opening_sentence_does_not_split_inside_its_emphasis() {
        // Verbatim from a turn this got wrong: the bold closes after the full
        // stop, so the splitter cut there and the second claim reached the
        // judge — and the user — reading "** The ideal initial planting depth".
        let claims = lexical_pass(
            "**Bury it all the way — you don't leave the top sticking out.** The ideal initial \
             planting depth is 4–6 inches of soil above the seed potato [1].",
            &[source(
                "The ideal initial planting depth for potatoes is 4 to 6 inches. This initial 4- \
                 to 6-inch soil covering provides the seed potato with insulation and moisture.",
            )],
        );

        assert_eq!(claims.len(), 2);
        assert!(
            claims.iter().all(|claim| !claim.sentence.contains('*')),
            "emphasis markers reached the claim text: {:?}",
            claims.iter().map(|c| &c.sentence).collect::<Vec<_>>()
        );
        assert!(claims[0].sentence.starts_with("Bury it all the way"));
        assert!(claims[1]
            .sentence
            .starts_with("The ideal initial planting depth"));
    }

    #[test]
    fn an_underscore_inside_a_word_is_not_emphasis() {
        assert_eq!(
            strip_emphasis_markers("Set _max_tokens_ in the *config* file"),
            "Set max_tokens in the config file"
        );
        assert_eq!(
            strip_emphasis_markers("A rule: ---- and ____"),
            "A rule: ---- and ____"
        );
    }

    #[test]
    fn a_sentence_with_no_citation_is_never_sent_to_the_judge() {
        // Its only possible evidence would be whichever sources ranked first,
        // which it never claimed anything about. A closing summary line was
        // marked "contradicted" that way, against a different document.
        let claims = lexical_pass(
            "Potatoes want 4–6 inches of cover [1].\n\
             So: fully buried at first, then buried deeper as the plant grows.",
            &[source(
                "The ideal initial planting depth for potatoes is 4 to 6 inches of soil cover.",
            )],
        );

        let summary = claims
            .iter()
            .find(|claim| claim.sentence.starts_with("So:"))
            .expect("the uncited closing line is still a claim");
        assert!(summary.citation_ids.is_empty());
        assert!(!needs_judge(summary));
        // The cited one still goes, numbers being exactly what overlap cannot settle.
        let cited = claims
            .iter()
            .find(|claim| claim.citation_ids == vec![1])
            .expect("the cited claim");
        assert!(needs_judge(cited));
    }

    #[test]
    fn grounded_claim_scores_above_threshold() {
        let claims = lexical_pass(
            "Blueberry plants showed improved cold tolerance after salicylic acid treatment [1].",
            &[source(
                "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress.",
            )],
        );

        assert_eq!(claims.len(), 1);
        assert!(claims[0].supported);
        assert_eq!(claims[0].citation_ids, vec![1]);
    }

    #[test]
    fn unsupported_claim_scores_below_threshold() {
        let claims = lexical_pass(
            "Blueberry plants tripled yield after lunar-cycle irrigation [1].",
            &[source(
                "The study measured antioxidant enzyme response under low-temperature stress.",
            )],
        );

        assert_eq!(claims.len(), 1);
        assert!(!claims[0].supported);
    }

    #[test]
    fn short_or_question_sentences_are_skipped() {
        let claims = lexical_pass("Any update?\nThanks.", &[source("Random source text.")]);
        assert!(claims.is_empty());
    }

    #[test]
    fn the_cut_short_note_is_not_a_claim() {
        let answer = format!(
            "Bush beans mature within sixty days in shallow window boxes [1].{}",
            super::super::super::tool_loop::CUT_SHORT_NOTE
        );
        let claims = lexical_pass(
            &answer,
            &[source(
                "Bush beans mature within sixty days and grow well in shallow window boxes.",
            )],
        );
        assert_eq!(claims.len(), 1);
        assert!(claims[0].supported);
    }

    #[test]
    fn the_out_of_rounds_note_is_not_a_claim() {
        let answer = format!(
            "Bush beans mature within sixty days in shallow window boxes [1].{}",
            super::super::super::tool_loop::OUT_OF_ROUNDS_NOTE
        );
        let claims = lexical_pass(
            &answer,
            &[source(
                "Bush beans mature within sixty days and grow well in shallow window boxes.",
            )],
        );
        assert_eq!(claims.len(), 1);
        assert!(claims[0].supported);
    }

    #[test]
    fn citation_index_selects_the_referenced_source() {
        let claims = lexical_pass(
            "Blueberry anthocyanins support vascular function and endothelial health [2].",
            &[
                source("Irrigation protocol and frost timing notes for greenhouse control."),
                source("Anthocyanins from blueberries were associated with improved vascular endothelial function in adults."),
            ],
        );

        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].citation_ids, vec![2]);
        assert_eq!(claims[0].cited_source_indices, vec![1]);
        assert!(claims[0].supported);
    }

    #[test]
    fn mismatched_citation_index_is_flagged_even_if_another_source_matches() {
        let claims = lexical_pass(
            "Blueberry anthocyanins support vascular function and endothelial health [1].",
            &[
                source("Irrigation protocol and frost timing notes for greenhouse control."),
                source("Anthocyanins from blueberries were associated with improved vascular endothelial function in adults."),
            ],
        );

        assert_eq!(claims.len(), 1);
        assert!(!claims[0].supported);
    }

    #[test]
    fn short_cited_numeric_answers_are_not_skipped() {
        let claims = lexical_pass(
            "Retention is 14 days [1].",
            &[source("Retention is 30 days.")],
        );
        assert_eq!(claims.len(), 1);
        assert!(needs_judge(&claims[0]));
    }

    #[test]
    fn invalid_citation_cannot_borrow_support_from_another_source() {
        for citation in ["[99]", "[0]", "[1][99]"] {
            let claims = lexical_pass(
                &format!("Blueberry plants showed improved cold tolerance after salicylic acid treatment {citation}."),
                &[source("Blueberry plants showed improved cold tolerance after salicylic acid treatment.")],
            );
            assert_eq!(claims.len(), 1);
            assert!(
                !claims[0].supported,
                "invalid citation {citation} must fail closed"
            );
        }
    }

    #[test]
    fn empty_source_does_not_shift_citation_evidence() {
        let mut empty = source("");
        empty.file_name.clear();
        empty.file_path.clear();
        let claims = lexical_pass(
            "Blueberry plants showed improved cold tolerance after salicylic acid treatment [2].",
            &[empty, source("Blueberry plants showed improved cold tolerance after salicylic acid treatment.")],
        );
        assert_eq!(claims[0].cited_source_indices, vec![1]);
        assert!(claims[0].supported);
    }

    #[test]
    fn assigned_citation_ids_survive_source_reordering() {
        let mut matching = source(
            "Blueberry plants showed improved cold tolerance after salicylic acid treatment.",
        );
        matching.citation_id = Some(7);
        let mut unrelated =
            source("Irrigation protocol and frost timing notes for greenhouse control.");
        unrelated.citation_id = Some(2);
        let claims = lexical_pass(
            "Blueberry plants showed improved cold tolerance after salicylic acid treatment [7].",
            &[unrelated, matching],
        );
        assert_eq!(claims[0].citation_ids, vec![7]);
        assert_eq!(claims[0].cited_source_indices, vec![1]);
        assert!(claims[0].supported);
    }

    #[test]
    fn claim_text_keeps_the_full_sentence() {
        let claims = lexical_pass(
            "The same epigenetic shifts are linked to altered expression of genes that control meristem activity, suggesting a mechanistic link between methylation dynamics and developmental transitions [1][2].",
            &[source("The study discusses WOX family structure and stress response without this meristem claim.")],
        );

        assert_eq!(claims.len(), 1);
        assert!(claims[0].claim_text.contains(
            "The same epigenetic shifts are linked to altered expression of genes that control meristem activity"
        ));
        assert!(claims[0].claim_text.contains(
            "mechanistic link between methylation dynamics and developmental transitions"
        ));
        assert!(!claims[0].claim_text.contains("developmental tr..."));
    }

    #[test]
    fn a_decimal_point_does_not_end_a_sentence() {
        let sentences = split_sentences(
            "The pooled effect was 1.2 °C per 10 points [1]. Surface readings ran 8.5 °C apart!\nSee example.com for v2.1.",
        );

        assert_eq!(
            sentences,
            vec![
                "The pooled effect was 1.2 °C per 10 points [1].",
                "Surface readings ran 8.5 °C apart!",
                "See example.com for v2.1.",
            ]
        );
    }

    #[test]
    fn numbers_dates_and_negations_are_contradiction_prone() {
        assert!(is_contradiction_prone("Yields rose by 42 percent."));
        assert!(is_contradiction_prone("The trial began in March 2021."));
        assert!(is_contradiction_prone(
            "The treatment did not improve tolerance."
        ));
        assert!(is_contradiction_prone(
            "The sample didn't survive the freeze."
        ));
        assert!(is_contradiction_prone("Only the treated group recovered."));
        assert!(!is_contradiction_prone(
            "Salicylic acid treatment improved cold tolerance in blueberry plants."
        ));
    }

    #[test]
    fn strong_overlap_skips_the_judge_but_a_number_does_not() {
        let plain = LexicalClaim {
            sentence: "s".into(),
            claim_text: "s".into(),
            citation_ids: vec![1],
            cited_source_indices: vec![0],
            overlap_ratio: 0.8,
            matching_tokens: 6,
            supported: true,
            contradiction_prone: false,
        };
        assert!(!needs_judge(&plain));

        let with_number = LexicalClaim {
            contradiction_prone: true,
            ..plain.clone()
        };
        assert!(needs_judge(&with_number));

        let ambiguous = LexicalClaim {
            overlap_ratio: 0.40,
            matching_tokens: 3,
            ..plain.clone()
        };
        assert!(needs_judge(&ambiguous));

        let weak = LexicalClaim {
            overlap_ratio: 0.10,
            matching_tokens: 1,
            supported: false,
            ..plain
        };
        assert!(needs_judge(&weak));
    }

    /// Explorer answers point at lines in the folder. Those sentences are
    /// about the file on screen, so the sources this turn retrieved cannot
    /// judge them, even when one happens to share their words.
    #[test]
    fn a_sentence_with_a_line_reference_is_not_judged_against_sources() {
        let passage =
            source("The main function prints a greeting and exits with status zero after setup.");
        let claims = lexical_pass(
            "The main function prints a greeting and exits with status zero, see `src/main.rs:1-3` [1]. \
             The main function prints a greeting and exits with status zero after setup [1].",
            std::slice::from_ref(&passage),
        );
        assert_eq!(claims.len(), 1, "{claims:?}");
        assert!(!claims[0].sentence.contains("src/main.rs"));
    }
}
