//! Turning free text into an FTS5 `MATCH` expression.
//!
//! Both lexical entry points — [`BM25Search`] for direct search and
//! [`SqliteTextSearch`] for the chat path — ran a byte-for-byte copy of this
//! logic. They now share it, so a query behaves the same whichever path asked.
//!
//! Two rules matter more than the rest:
//!
//! * **Every term is emitted as a quoted FTS5 string.** A bareword is only
//!   legal for some inputs (FTS5 rejects one that starts with a digit) and it
//!   puts user text where `NEAR`, `*`, `:`, `-`, `AND`/`OR`/`NOT` and
//!   parentheses are operators. Quoting with `"` doubled is the one form that
//!   is always a literal.
//! * **A term is kept for having any alphanumeric character, in any script.**
//!   The previous rule required an ASCII letter, which silently emptied the
//!   lexical branch for Cyrillic, Greek, Japanese and Chinese queries, and for
//!   anything numeric: `4012`, `429`, `2026`.
//!
//! [`BM25Search`]: super::bm25::BM25Search
//! [`SqliteTextSearch`]: super::text_search::SqliteTextSearch

use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
use std::collections::HashSet;

/// Minimum length, in characters, of a term written in a script that separates
/// words with spaces. Short English words carry almost no retrieval signal and
/// the porter tokenizer indexes a great many of them.
const MIN_TERM_CHARS: usize = 3;

/// Cap on the terms in one `MATCH` expression, including stems.
const MAX_TERMS: usize = 16;

/// The trigram tokenizer emits no token for a shorter string, so a shorter term
/// cannot use the trigram index and would only widen the query for nothing.
const TRIGRAM_MIN_CHARS: usize = 3;

/// Which chunk index a normalized query has to run against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsIndex {
    /// `chunks_fts`, tokenized `porter unicode61 remove_diacritics 2`.
    Words,
    /// `chunks_trigram`, tokenized `trigram`.
    ///
    /// `unicode61` does not segment CJK: it indexes an unbroken run of Han or
    /// Kana as a single token, so a query for two characters of a ten-character
    /// run matches nothing at all. Overlapping three-character windows are the
    /// standard FTS5 answer, and they are only worth their index size for the
    /// queries that need them.
    Trigram,
}

impl FtsIndex {
    /// The virtual table this index lives in.
    pub fn table(self) -> &'static str {
        match self {
            FtsIndex::Words => "chunks_fts",
            FtsIndex::Trigram => "chunks_trigram",
        }
    }
}

/// A `MATCH` expression and the index it addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsQuery {
    pub index: FtsIndex,
    pub match_expression: String,
}

/// Normalize free-text input into an FTS5 query, or `None` when the input has
/// no searchable term and the lexical branch should simply be skipped.
///
/// FTS5 treats whitespace as implicit AND. For natural language we prefer OR
/// semantics to preserve recall. Input that already uses FTS5 operators is
/// passed through unchanged so a deliberate phrase or boolean query still
/// works — except when it contains CJK, which has to reach the trigram index.
pub fn normalize(query: &str) -> Option<FtsQuery> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    if !contains_cjk(trimmed) && looks_like_explicit_fts_syntax(trimmed) {
        return Some(FtsQuery {
            index: FtsIndex::Words,
            match_expression: trimmed.to_string(),
        });
    }

    strict_tokenize(trimmed)
}

/// The fallback [`normalize`] retries with: never passes input through, so it
/// cannot reproduce a syntax error raised by the caller's own operators.
pub fn strict_tokenize(query: &str) -> Option<FtsQuery> {
    let index = if contains_cjk(query) {
        FtsIndex::Trigram
    } else {
        FtsIndex::Words
    };

    let terms = selected_terms(query, index);
    if terms.is_empty() {
        return None;
    }

    let match_expression = terms
        .iter()
        .map(|term| quote(term))
        .collect::<Vec<_>>()
        .join(" OR ");
    Some(FtsQuery {
        index,
        match_expression,
    })
}

/// Whether an error message is FTS5 rejecting the `MATCH` expression, as
/// opposed to any other database failure. Both callers see the message through
/// a different error type, so they classify the string rather than the value.
pub fn is_syntax_error_message(message: &str) -> bool {
    message.contains("fts5: syntax error")
        || message.contains("malformed MATCH expression")
        || message.contains("unterminated string")
        // Prose that merely contains a `*` or a quote is taken for
        // hand-written FTS syntax and passed through raw, where
        // `rules - The Silo` parses as a column filter on a column named
        // `The`. That is as much the query's fault as a syntax error, and
        // without the retry the keyword half of the search was lost.
        || message.contains("no such column")
}

/// An FTS5 string literal: the one form no user input can escape from.
fn quote(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

fn looks_like_explicit_fts_syntax(query: &str) -> bool {
    query.contains('"')
        || query.contains('*')
        || query.contains(" OR ")
        || query.contains(" AND ")
        || query.contains(" NOT ")
        || query.contains(" NEAR ")
        || query.contains(" NEAR(")
}

/// Han, Kana, Hangul and the CJK extensions: the scripts `unicode61` runs
/// together into one token because they are written without spaces.
fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3040..=0x30FF     // Hiragana, Katakana
        | 0x31F0..=0x31FF   // Katakana phonetic extensions
        | 0x3400..=0x4DBF   // CJK unified ideographs extension A
        | 0x4E00..=0x9FFF   // CJK unified ideographs
        | 0xF900..=0xFAFF   // CJK compatibility ideographs
        | 0xAC00..=0xD7AF   // Hangul syllables
        | 0x20000..=0x2FA1F // CJK unified ideographs extensions B onwards
    )
}

fn contains_cjk(text: &str) -> bool {
    text.chars().any(is_cjk)
}

/// Split on everything that is not alphanumeric, lowercasing as we go.
///
/// `char::is_alphanumeric` rather than `is_ascii_alphanumeric`, so a Cyrillic
/// or Han run survives tokenization instead of being treated as punctuation.
fn tokenize(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            terms.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    terms
}

/// Whether a token is worth putting in the query.
///
/// The English minimum length is a property of English, not of text: two Han
/// characters are a word, and a bare number is often the most selective thing
/// in the query (`ERR-4012`, `429`, the year in "before the 2026 change").
fn keep_term(term: &str, index: FtsIndex) -> bool {
    if !term.chars().any(char::is_alphanumeric) {
        return false;
    }
    let length = term.chars().count();
    if index == FtsIndex::Trigram {
        return length >= TRIGRAM_MIN_CHARS;
    }
    if contains_cjk(term) || term.chars().any(char::is_numeric) {
        return length >= 1;
    }
    length >= MIN_TERM_CHARS
}

/// The porter stemmer only knows English, so it is only applied to a term that
/// could be an English word. Running it over transliterated or accented input
/// invents terms that are in no index.
fn stem_term(term: &str) -> Option<String> {
    if !term.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    static EN_STEMMER: Lazy<Stemmer> = Lazy::new(|| Stemmer::create(Algorithm::English));
    Some(EN_STEMMER.stem(term).to_string())
}

/// Shannon entropy over the term's characters.
///
/// Counted in characters, not bytes: with a byte denominator the per-character
/// probabilities of a non-ASCII term do not sum to one, so its entropy came out
/// two to three times too low and every non-Latin term lost the salience cut to
/// its ASCII neighbours. ASCII terms are unaffected — there, bytes are
/// characters.
fn term_entropy(term: &str) -> f32 {
    use std::collections::HashMap;

    let length = term.chars().count();
    if length == 0 {
        return 0.0;
    }

    let mut counts: HashMap<char, usize> = HashMap::new();
    for ch in term.chars() {
        *counts.entry(ch).or_insert(0) += 1;
    }

    let denom = length as f32;
    counts.values().fold(0.0_f32, |acc, count| {
        let p = (*count as f32) / denom;
        if p <= f32::EPSILON {
            acc
        } else {
            acc - p * p.log2()
        }
    })
}

/// A term carrying a number of at least two characters: `2026`, `429`, `4012`,
/// `v2`.
///
/// These are exempt from the relative-salience cut below. Character entropy is
/// a proxy for how informative a *word* is, and a poor one for a number:
/// `2026` scores 1.82 where `workspace` scores 4.62, so in "before the 2026
/// change, how long were daily workspace recovery points retained?" the cut
/// removed the one term that tells the superseded policy from the current one.
/// A single stray digit is still left to the cut, where it scores zero.
fn carries_a_number(term: &str) -> bool {
    term.chars().any(char::is_numeric) && term.chars().count() >= 2
}

fn term_salience(term: &str) -> f32 {
    let entropy = term_entropy(term);
    let length_factor = ((term.chars().count() as f32) + 1.0).ln();
    entropy * (0.65 + 0.35 * length_factor)
}

/// Rank the query's terms by salience and keep the band near the best one.
fn select_informative_terms(terms: Vec<String>, max_terms: usize, index: FtsIndex) -> Vec<String> {
    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for term in terms {
        if keep_term(&term, index) && seen.insert(term.clone()) {
            deduped.push(term);
        }
    }
    if deduped.is_empty() {
        return Vec::new();
    }

    let mut ranked: Vec<(String, f32)> = deduped
        .into_iter()
        .map(|term| {
            let score = term_salience(&term);
            (term, score)
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let best_score = ranked.first().map(|(_, score)| *score).unwrap_or(0.0);
    if best_score <= f32::EPSILON {
        return ranked
            .into_iter()
            .take(max_terms)
            .map(|(term, _)| term)
            .collect();
    }

    let mut selected: Vec<String> = ranked
        .iter()
        .filter_map(|(term, score)| {
            (carries_a_number(term) || *score >= best_score * 0.42).then_some(term.clone())
        })
        .take(max_terms)
        .collect();
    if selected.is_empty() {
        selected = ranked
            .into_iter()
            .take(max_terms.min(3))
            .map(|(term, _)| term)
            .collect();
    }
    selected
}

fn selected_terms(query: &str, index: FtsIndex) -> Vec<String> {
    let mut seen = HashSet::new();
    let deduped: Vec<String> = tokenize(query)
        .into_iter()
        .filter(|term| !term.is_empty() && seen.insert(term.clone()))
        .collect();

    let mut selected = select_informative_terms(deduped, MAX_TERMS, index);
    if selected.is_empty() || index == FtsIndex::Trigram {
        // Trigram matching is on raw character windows; an English stem is not
        // a substring of the inflected form often enough to be worth a term.
        return selected;
    }

    let mut seen: HashSet<String> = selected.iter().cloned().collect();
    for term in selected.clone() {
        let Some(stem) = stem_term(&term) else {
            continue;
        };
        if stem != term && stem.chars().count() >= MIN_TERM_CHARS && seen.insert(stem.clone()) {
            selected.push(stem);
            if selected.len() >= MAX_TERMS {
                break;
            }
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms_of(query: &str) -> Vec<String> {
        let built = normalize(query).expect("query produced no terms");
        built
            .match_expression
            .split(" OR ")
            .map(|term| term.trim_matches('"').to_string())
            .collect()
    }

    #[test]
    fn empty_and_punctuation_only_queries_skip_the_lexical_branch() {
        for query in ["", "   ", "???", "-- ...!!", "()"] {
            assert_eq!(normalize(query), None, "{query:?} should yield no query");
        }
    }

    #[test]
    fn every_term_is_emitted_as_a_quoted_string() {
        let built = normalize("immigration and customs enforcement").expect("terms");
        for part in built.match_expression.split(" OR ") {
            assert!(part.starts_with('"') && part.ends_with('"'), "{part}");
        }
    }

    /// The point of quoting: no input can leave the literal and become syntax.
    #[test]
    fn fts5_operators_in_the_input_stay_inside_the_literal() {
        let hostile = "alpha NEAR(beta gamma) col:value -minus (paren) bravo* AND NOT charlie";
        let built = strict_tokenize(hostile).expect("terms");
        for part in built.match_expression.split(" OR ") {
            let inner = part
                .strip_prefix('"')
                .and_then(|p| p.strip_suffix('"'))
                .unwrap_or_else(|| panic!("unquoted term {part}"));
            assert!(
                inner.chars().all(char::is_alphanumeric),
                "term {inner:?} carries syntax"
            );
        }
    }

    #[test]
    fn an_embedded_quote_is_doubled_rather_than_closing_the_literal() {
        assert_eq!(quote("say\"hi"), "\"say\"\"hi\"");
    }

    #[test]
    fn cyrillic_and_greek_queries_reach_the_word_index() {
        for query in ["политика хранения", "πολιτική διατήρησης"]
        {
            let built = normalize(query).expect("terms");
            assert_eq!(built.index, FtsIndex::Words);
            assert!(!built.match_expression.is_empty());
        }
    }

    #[test]
    fn accented_latin_survives_normalization() {
        let terms = terms_of("¿Hay alguna línea en español disponible los sábados?");
        assert!(terms.iter().any(|t| t == "español"));
        assert!(terms.iter().any(|t| t == "línea" || t == "sábados"));
    }

    #[test]
    fn a_digits_only_query_is_searchable() {
        let built = normalize("4012").expect("digits are a term");
        assert_eq!(built.match_expression, "\"4012\"");
        assert_eq!(normalize("2027 03").expect("terms").index, FtsIndex::Words);
    }

    #[test]
    fn mixed_alphanumeric_identifiers_keep_their_numeric_part() {
        let terms = terms_of("ERR-4012");
        assert!(terms.contains(&"err".to_string()), "{terms:?}");
        assert!(terms.contains(&"4012".to_string()), "{terms:?}");

        let version = terms_of("v2.3.1");
        assert!(version.contains(&"v2".to_string()), "{version:?}");
    }

    /// The regression this rule exists for: the year is the only thing telling
    /// a superseded policy from the current one.
    #[test]
    fn a_year_survives_a_sentence_full_of_words() {
        let terms = terms_of(
            "Before the 2026 change, how long were daily workspace recovery points retained?",
        );
        assert!(terms.contains(&"2026".to_string()), "{terms:?}");
    }

    #[test]
    fn japanese_and_chinese_queries_are_routed_to_the_trigram_index() {
        for query in ["日本語サポート時間", "保留政策是什么"] {
            let built = normalize(query).expect("terms");
            assert_eq!(built.index, FtsIndex::Trigram, "{query}");
            assert!(built.match_expression.starts_with('"'));
        }
    }

    /// A trigram term shorter than one trigram cannot use the index, so the
    /// branch is skipped rather than run against something that cannot match.
    #[test]
    fn a_one_or_two_character_cjk_query_skips_the_lexical_branch() {
        for query in ["日", "日本", "設定"] {
            assert_eq!(normalize(query), None, "{query} should skip the branch");
        }
    }

    #[test]
    fn a_mixed_query_keeps_the_ascii_terms_on_the_trigram_index() {
        let built = normalize("日本語サポート hours").expect("terms");
        assert_eq!(built.index, FtsIndex::Trigram);
        assert!(built.match_expression.contains("日本語サポート"));
        assert!(built.match_expression.contains("hours"));
    }

    #[test]
    fn explicit_phrase_syntax_is_preserved() {
        let built = normalize("\"blueberry anthocyanin\"").expect("terms");
        assert_eq!(built.match_expression, "\"blueberry anthocyanin\"");
        assert_eq!(built.index, FtsIndex::Words);
    }

    /// A quoted CJK phrase still has to reach the index that can match it.
    #[test]
    fn explicit_syntax_does_not_strand_a_cjk_query_on_the_word_index() {
        let built = normalize("\"日本語サポート\"").expect("terms");
        assert_eq!(built.index, FtsIndex::Trigram);
    }

    #[test]
    fn plain_punctuation_is_not_mistaken_for_fts_syntax() {
        let terms = terms_of("Immigration and Customs Enforcement (ICE): Operations expanded.");
        for expected in ["immigration", "customs", "enforcement", "operations"] {
            assert!(terms.contains(&expected.to_string()), "{terms:?}");
        }
    }

    #[test]
    fn english_terms_still_carry_their_stems() {
        let terms = terms_of("retention policies for workspaces");
        assert!(terms.contains(&"policies".to_string()), "{terms:?}");
        assert!(terms.contains(&"polici".to_string()), "{terms:?}");
    }

    /// The minimum-length rule is unchanged for space-separated scripts: this
    /// path has no stoplist, so a three-letter word still competes on salience,
    /// but a shorter one never enters the query at all.
    #[test]
    fn a_word_below_the_minimum_length_is_still_dropped() {
        let terms = terms_of("the cat sat on retention policies");
        assert!(!terms.contains(&"on".to_string()), "{terms:?}");
        assert!(terms.contains(&"retention".to_string()), "{terms:?}");
    }

    /// A single stray digit is not an identifier and still loses the cut.
    #[test]
    fn a_lone_digit_does_not_survive_beside_real_terms() {
        let terms = terms_of("section 7 of the retention policy for workspaces");
        assert!(!terms.contains(&"7".to_string()), "{terms:?}");
    }

    #[test]
    fn syntax_errors_are_recognized_from_the_message() {
        assert!(is_syntax_error_message(
            "error returned from database: (code: 1) fts5: syntax error near \"Operations\""
        ));
        assert!(!is_syntax_error_message("database is locked"));
    }

    /// Entropy over characters, not bytes: two terms with the same character
    /// makeup must score the same whatever their encoding costs.
    #[test]
    fn entropy_does_not_punish_multibyte_terms() {
        assert!((term_entropy("abcd") - term_entropy("абвг")).abs() < 1e-5);
        assert!((term_entropy("abcd") - 2.0).abs() < 1e-5);
    }
}
