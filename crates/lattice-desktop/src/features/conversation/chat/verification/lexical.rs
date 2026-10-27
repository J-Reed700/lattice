//! Lexical grounding pre-filter: stemmed token overlap between each claim
//! sentence and the passages it cites. Cheap and deterministic, so it runs
//! first and decides which sentences are worth an LLM call. Overlap cannot
//! read meaning, so it never settles a cited claim alone: only a near-verbatim
//! copy that carries and cuts no number, date, negation or name skips the judge.

use lattice_core::application::contracts::sources::SourceDto;
pub(super) use lattice_core::application::services::evidence_retrieval::{
    best_windows, ScoredWindow,
};
use lattice_core::application::services::evidence_retrieval::{
    extract_normalized_tokens, passage_windows,
};
use lattice_core::shared::markdown_math::{
    formulas_as_words, inline_math_spans, map_prose, split_at_formulas,
};
use lattice_core::shared::text::{
    capitalised_words, has_digit, normalize_whitespace, padded_lowercase, MONTH_MARKERS,
    NEGATION_MARKERS,
};
use std::collections::{BTreeSet, HashMap, HashSet};

const MIN_CLAIM_CHARS: usize = 24;
pub(super) const MIN_CLAIM_TOKENS: usize = 5;
pub(super) const MIN_OVERLAP_RATIO: f32 = 0.32;
pub(super) const MIN_MATCHING_TOKENS: usize = 2;

/// A claim settled without the judge must share at least this many content
/// tokens with its passage. A shorter one copies too little for its wording to
/// say anything about its meaning.
pub(super) const MIN_COPIED_TOKENS: usize = 4;

/// Words per run in the near-verbatim test. Long enough that a run rarely
/// recurs by chance inside one window; short enough that a claim which drops,
/// swaps or adds one word in the middle of a passage sentence breaks every run
/// that crosses the change.
const COPIED_RUN_WORDS: usize = 4;

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
    /// Kept for tests and debugging; no decision reads it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) overlap_ratio: f32,
    pub(super) matching_tokens: usize,
    /// The lexical verdict: enough overlap to call this grounded.
    pub(super) supported: bool,
    /// Every run of four consecutive words in the claim occurs in one window
    /// of a cited passage: the claim is the passage's own wording, perhaps cut
    /// short, but not reordered, reworded or added to.
    pub(super) near_verbatim: bool,
    /// Overlap cannot settle it. The claim carries a number, date or negation,
    /// or the passage sentence it copies carries a negation, number, date or
    /// name that the claim left out.
    pub(super) contradiction_prone: bool,
}

/// Score every claim sentence in `response` against `sources`.
///
/// Returns one entry per sentence that cleared the claim filters; sentences
/// that are too short, are questions, or are boilerplate never appear.
pub(super) fn lexical_pass(response: &str, sources: &[SourceDto]) -> Vec<LexicalClaim> {
    // A diagram's labels and a picture's caption are not claims about the
    // sources: the check, and the judge reading what it escalates, see the
    // answer without its fenced blocks and pictures.
    let response = lattice_core::shared::markdown_media::strip_fences_and_images(response);
    let response = response.as_str();
    if response.trim().is_empty() || sources.is_empty() {
        return Vec::new();
    }

    let source_token_sets = collect_source_token_sets(sources);
    if source_token_sets.is_empty() {
        return Vec::new();
    }
    // Built only for sources some claim cites, once each.
    let mut wording: HashMap<usize, PassageWording> = HashMap::new();

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
        // A formula's names count, its TeX does not: `\frac` is no word of
        // the claim for a passage to miss.
        let claim_tokens = extract_normalized_tokens(&formulas_as_words(&claim_text));
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
        // A sentence that cites nothing is never backed, however many words it
        // shares with some source. Its overlap is still measured, for diagnostics.
        let supported = !citation_ids.is_empty()
            && matching_tokens >= MIN_MATCHING_TOKENS
            && overlap_ratio >= MIN_OVERLAP_RATIO;
        let copying = if citations_valid && !citation_ids.is_empty() {
            compare_wording(&claim_text, &cited_source_indices, sources, &mut wording)
        } else {
            Copying::default()
        };

        claims.push(LexicalClaim {
            sentence: claim_for_note(&sentence),
            contradiction_prone: is_contradiction_prone(&claim_text) || copying.drops_marker,
            claim_text: claim_for_note(&claim_text),
            citation_ids,
            cited_source_indices,
            overlap_ratio,
            matching_tokens,
            supported,
            near_verbatim: copying.near_verbatim,
        });
    }

    claims
}

/// Whether this claim is worth an LLM call. Every cited claim is, except a
/// near-verbatim copy of its passage: every run of four of its words in one
/// window, at least [`MIN_COPIED_TOKENS`] content words, no number, date or
/// negation, and nothing cut from the copied sentence that is one, or a name.
/// A sentence with no citation is never escalated: it has no passages to be
/// judged against.
pub(super) fn needs_judge(claim: &LexicalClaim) -> bool {
    if claim.citation_ids.is_empty() {
        return false;
    }
    let settled = claim.supported
        && claim.near_verbatim
        && claim.matching_tokens >= MIN_COPIED_TOKENS
        && !claim.contradiction_prone;
    !settled
}

/// Checked only in the passage sentence a claim copies, never in the claim
/// itself, so a claim naming the same weekday as its source still counts as a
/// copy of it.
const WEEKDAY_MARKERS: &[&str] = &[
    " monday ",
    " tuesday ",
    " wednesday ",
    " thursday ",
    " friday ",
    " saturday ",
    " sunday ",
];

/// Checked the same way. "One" is left out on purpose: it is far more often a
/// pronoun ("a manager cannot grant one") than a count.
const NUMBER_WORDS: &[&str] = &[
    " two ",
    " three ",
    " four ",
    " five ",
    " six ",
    " seven ",
    " eight ",
    " nine ",
    " ten ",
    " eleven ",
    " twelve ",
    " twenty ",
    " thirty ",
    " forty ",
    " fifty ",
    " hundred ",
    " thousand ",
    " million ",
    " billion ",
    " twice ",
    " half ",
    " dozen ",
];

fn is_contradiction_prone(claim: &str) -> bool {
    if has_digit(claim) {
        return true;
    }

    let padded = padded_lowercase(claim);
    NEGATION_MARKERS
        .iter()
        .chain(MONTH_MARKERS.iter())
        .any(|marker| padded.contains(marker))
}

/// A cited passage's wording, for the near-verbatim test.
struct PassageWording {
    /// Every run of [`COPIED_RUN_WORDS`] words, per window. These are the
    /// windows overlap is scored on, so a claim must be found in one stretch
    /// of a long page, not assembled from runs scattered across it.
    window_runs: Vec<HashSet<String>>,
    /// Each sentence with its runs. Split line by line first, so a heading
    /// never joins the sentence under it and reads as a name inside it.
    sentences: Vec<(String, HashSet<String>)>,
}

impl PassageWording {
    /// The text the judge would read: the page, or its excerpt when the page
    /// came back empty.
    fn of(source: &SourceDto) -> Self {
        let text = if source.content.trim().is_empty() {
            source.excerpt.as_deref().unwrap_or_default()
        } else {
            source.content.as_str()
        };
        Self {
            window_runs: passage_windows(text)
                .iter()
                .map(|window| word_runs(&lowercase_words(window)))
                .collect(),
            sentences: text
                .split('\n')
                .flat_map(|line| split_sentences(&strip_emphasis_markers(line)))
                .map(|sentence| {
                    let runs = word_runs(&lowercase_words(&sentence));
                    (sentence, runs)
                })
                .collect(),
        }
    }
}

/// How a claim's wording stands against the passages it cites.
#[derive(Debug, Default)]
struct Copying {
    near_verbatim: bool,
    /// A sentence the claim copies carries a negation, number, date or name
    /// that the claim does not.
    drops_marker: bool,
}

fn compare_wording(
    claim: &str,
    cited_source_indices: &[usize],
    sources: &[SourceDto],
    cache: &mut HashMap<usize, PassageWording>,
) -> Copying {
    let mut copying = Copying::default();
    let claim_runs = word_runs(&lowercase_words(claim));
    if claim_runs.is_empty() {
        return copying;
    }
    for &idx in cited_source_indices {
        let Some(source) = sources.get(idx) else {
            continue;
        };
        let wording = cache
            .entry(idx)
            .or_insert_with(|| PassageWording::of(source));
        if !wording
            .window_runs
            .iter()
            .any(|runs| claim_runs.is_subset(runs))
        {
            continue;
        }
        copying.near_verbatim = true;
        copying.drops_marker |= wording
            .sentences
            .iter()
            .filter(|(_, runs)| !runs.is_disjoint(&claim_runs))
            .any(|(sentence, _)| drops_a_marker(claim, sentence));
    }
    copying
}

/// Whether `sentence`, which the claim copies, carries something the claim
/// left out: a negation, a number, a weekday or month, or a capitalised name.
/// The whole sentence is read, since the part that was cut may hold the "not".
fn drops_a_marker(claim: &str, sentence: &str) -> bool {
    let claim_padded = padded_lowercase(claim);
    let sentence_padded = padded_lowercase(sentence);
    if NEGATION_MARKERS
        .iter()
        .chain(MONTH_MARKERS)
        .chain(WEEKDAY_MARKERS)
        .chain(NUMBER_WORDS)
        .any(|marker| sentence_padded.contains(marker) && !claim_padded.contains(marker))
    {
        return true;
    }

    let claim_words: HashSet<String> = lowercase_words(claim).into_iter().collect();
    let dropped_number = lowercase_words(sentence)
        .into_iter()
        .any(|word| has_digit(&word) && !claim_words.contains(&word));
    dropped_number || capitalised_words(sentence).any(|word| !claim_words.contains(&word))
}

/// The words of `text`, lowercased, in order. Unlike the overlap tokens,
/// nothing is stemmed or dropped: "not", "only" and "the" all count, because
/// a copy is a copy of every word.
fn lowercase_words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Every run of [`COPIED_RUN_WORDS`] consecutive words, or the whole text as
/// one run when it is shorter than that.
fn word_runs(words: &[String]) -> HashSet<String> {
    if words.len() < COPIED_RUN_WORDS {
        return if words.is_empty() {
            HashSet::new()
        } else {
            HashSet::from([words.join(" ")])
        };
    }
    words
        .windows(COPIED_RUN_WORDS)
        .map(|run| run.join(" "))
        .collect()
}

/// Every sentence with the citations it answers to. A full stop inside a token
/// ("1.2 °C", "example.com") does not end a sentence. A sentence without a
/// marker of its own takes the next marker in its paragraph or list item, or
/// failing that the previous one; a line with no marker at all stays uncited.
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

/// Drops markdown emphasis so it cannot be read as sentence structure: a
/// bolded sentence closes after its full stop, and splitting there leaves
/// stray `**` in the claims. Asterisk runs always go; underscores only outside
/// a word, so `snake_case` survives. A formula keeps both.
fn strip_emphasis_markers(text: &str) -> String {
    map_prose(text, strip_prose_emphasis)
}

fn strip_prose_emphasis(text: &str) -> String {
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
    let formulas = inline_math_spans(text);

    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        // A formula is part of its sentence: its stops (`$n!$`) end nothing.
        if let Some(formula) = formulas.iter().find(|formula| formula.start == at) {
            current.push_str(text.get(formula.clone()).unwrap_or_default());
            while chars.next_if(|(next, _)| *next < formula.end).is_some() {}
            continue;
        }
        current.push(ch);
        let pauses = chars.peek().is_none_or(|(_, next)| !next.is_alphanumeric());
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
    if lattice_core::shared::text::contains_line_reference(s) {
        return false;
    }
    // The app's own notes, appended when the model hit a limit. The splitter
    // stops at a full stop, so compare the words, in either emphasis form.
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
        super::super::tool_loop::WINDOW_FULL_NOTE,
        super::super::tool_loop::OUT_OF_TIME_NOTE,
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

/// A formula's brackets (`$a[1]$`) are its own, not a citation.
fn strip_citation_markers(sentence: &str) -> String {
    map_prose(sentence, strip_prose_citation_markers)
}

fn strip_prose_citation_markers(sentence: &str) -> String {
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
/// A formula's brackets (`$a[1]$`) are not markers.
fn extract_sentence_citation_ids(sentence: &str) -> Vec<u32> {
    let prose: String = split_at_formulas(sentence)
        .into_iter()
        .map(|(piece, formula)| if formula { " " } else { piece })
        .collect();
    let chars: Vec<char> = prose.chars().collect();
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

/// Every passage label a text cites, read as the claim checker reads them —
/// a formula's brackets are not labels — and also in the grouped form
/// `[2, 3]`, which the claim checker leaves as no citation. Asking whether a
/// passage was used at all, a grouped label is a use.
pub(in crate::features::conversation::chat) fn cited_labels(text: &str) -> BTreeSet<u32> {
    let prose: String = split_at_formulas(text)
        .into_iter()
        .map(|(piece, formula)| if formula { " " } else { piece })
        .collect();
    prose
        .split('[')
        .skip(1)
        .filter_map(|opened| opened.split_once(']'))
        .filter_map(|(inside, _)| {
            inside
                .split(',')
                .map(|label| {
                    let label = label.trim();
                    (!label.is_empty() && label.bytes().all(|byte| byte.is_ascii_digit()))
                        .then(|| label.parse::<u32>().ok())
                        .flatten()
                })
                .collect::<Option<Vec<u32>>>()
        })
        .flatten()
        .collect()
}

/// The sentence of `text` that shares the most vocabulary with `claim`, shown
/// beside a verdict as the words it rests on. Taken from the passage, since a
/// one-word judge cannot quote.
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
mod tests;
