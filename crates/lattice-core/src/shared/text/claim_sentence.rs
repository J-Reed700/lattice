//! The one sentence a claim stands on, found in its passage without a model
//! (claim sources design §18.2). The extractor names a passage; the claim's
//! statement restates one sentence of it, and that sentence is what the
//! review shows, a repair rewrites and confinement protects.

use super::{fold_diacritics, plain_text, sentences};

/// Two scores closer than this are equal: the same logarithms summed in
/// another order.
const SAME_SCORE: f64 = 1e-9;

/// The one sentence of `text`, among those overlapping `within`, that
/// `statement` restates; `None` when no sentence holds the greater part of
/// the statement's words.
///
/// Every sentence of [`sentences`] that overlaps `within` is a candidate,
/// whole even when `within` cuts it. A statement term weighs
/// `ln(1 + S / df)`, `S` being the number of candidates and `df` the number
/// holding the term (at least one), so a term every candidate holds weighs
/// least: no stop list, in any language. A candidate scores the summed
/// weight of the statement's distinct terms it holds; ties go to the one
/// holding more of the statement's adjacent term pairs in order, then to the
/// earlier one. The winner is the claim's sentence only when it holds at
/// least half of the statement's total weight; below that the statement
/// draws on several sentences or paraphrases them, and the caller keeps what
/// it had.
pub fn claim_sentence(
    text: &str,
    within: (usize, usize),
    statement: &str,
) -> Option<(usize, usize)> {
    let candidates: Vec<(usize, usize)> = sentences(text)
        .into_iter()
        .filter(|&(start, end)| start < within.1 && end > within.0)
        .collect();
    let wanted = terms(statement);
    if candidates.is_empty() || wanted.is_empty() {
        return None;
    }
    let held: Vec<Vec<String>> = candidates
        .iter()
        .map(|&(start, end)| text.get(start..end).map(terms).unwrap_or_default())
        .collect();

    let mut distinct: Vec<&str> = Vec::new();
    for term in &wanted {
        if !distinct.contains(&term.as_str()) {
            distinct.push(term);
        }
    }
    let count = candidates.len() as f64;
    let weights: Vec<f64> = distinct
        .iter()
        .map(|term| {
            let holding = held.iter().filter(|terms| holds(terms, term)).count();
            (1.0 + count / holding.max(1) as f64).ln()
        })
        .collect();
    let total: f64 = weights.iter().sum();
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    for pair in wanted.windows(2) {
        if let [first, second] = pair {
            let pair = (first.as_str(), second.as_str());
            if !pairs.contains(&pair) {
                pairs.push(pair);
            }
        }
    }

    let mut best: Option<(usize, f64, usize)> = None;
    for (index, terms) in held.iter().enumerate() {
        let score: f64 = distinct
            .iter()
            .zip(&weights)
            .filter(|(term, _)| holds(terms, term))
            .map(|(_, weight)| weight)
            .sum();
        let in_order = pairs
            .iter()
            .filter(|(first, second)| {
                terms.windows(2).any(|adjacent| match adjacent {
                    [a, b] => same_term(a, first) && same_term(b, second),
                    _ => false,
                })
            })
            .count();
        let better = match best {
            None => true,
            Some((_, top, top_pairs)) => {
                score > top + SAME_SCORE || (score > top - SAME_SCORE && in_order > top_pairs)
            }
        };
        if better {
            best = Some((index, score, in_order));
        }
    }
    let (index, score, _) = best?;
    if 2.0 * score + SAME_SCORE >= total {
        candidates.get(index).copied()
    } else {
        None
    }
}

/// The terms of `text`, in order: markdown, link syntax and `[n]` markers
/// removed, diacritics folded, lowercased, cut into maximal runs of
/// alphanumerics and `_`. A `.`, `,` or `'` between two alphanumerics stays
/// inside its run (`3,000`, `2.5`, `e.g`, `don't`). A run shorter than two
/// characters is dropped unless it is a number.
fn terms(text: &str) -> Vec<String> {
    let folded = fold_diacritics(&plain_text(text))
        .to_lowercase()
        .replace('\u{2019}', "'");
    let chars: Vec<char> = folded.chars().collect();
    let mut terms = Vec::new();
    let mut run = String::new();
    for (at, &ch) in chars.iter().enumerate() {
        let alphanumeric = |index: Option<usize>| {
            index
                .and_then(|index| chars.get(index))
                .is_some_and(|c| c.is_alphanumeric())
        };
        let joins = matches!(ch, '.' | ',' | '\'')
            && !run.is_empty()
            && alphanumeric(at.checked_sub(1))
            && alphanumeric(Some(at + 1));
        if ch.is_alphanumeric() || ch == '_' || joins {
            run.push(ch);
        } else if !run.is_empty() {
            keep(&mut terms, std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        keep(&mut terms, run);
    }
    terms
}

fn keep(terms: &mut Vec<String>, run: String) {
    if run.chars().count() >= 2 || run.chars().all(char::is_numeric) {
        terms.push(run);
    }
}

fn holds(terms: &[String], term: &str) -> bool {
    terms.iter().any(|held| same_term(held, term))
}

/// Equal, or both at least four characters long and one the other plus at
/// most two more (`schema`/`schemas`, `list`/`lists`, `call`/`called`).
fn same_term(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (a_len, b_len) = (a.chars().count(), b.chars().count());
    let (short, long, short_len, long_len) = if a_len <= b_len {
        (a, b, a_len, b_len)
    } else {
        (b, a, b_len, a_len)
    };
    short_len >= 4 && long_len - short_len <= 2 && long.starts_with(short)
}

#[cfg(test)]
mod tests {
    use super::{claim_sentence, terms};

    fn located<'a>(text: &'a str, statement: &str) -> Option<&'a str> {
        claim_sentence(text, (0, text.len()), statement).map(|(start, end)| &text[start..end])
    }

    #[test]
    fn a_statement_finds_its_sentence_not_the_one_naming_its_subject() {
        let text = "In autumn, **monarch butterflies** migrate up to 3,000 miles [1]. They winter in Mexico.";
        assert_eq!(
            located(text, "Monarch butterflies winter in Mexico"),
            Some("They winter in Mexico.")
        );
    }

    #[test]
    fn a_lead_line_and_a_bullet_are_located_apart() {
        let text = "Before a model can call a tool, it needs a structured description of what the tool does, its arguments, and its return values. Most APIs describe a tool with a JSON object that has five key fields:\n\n- **`type`**: Should always be `\"function\"` [5].\n- **`name`**: A unique identifier the model uses to call the tool [5].\n- **`description`**: What the tool does, written for the model to read.\n- **`parameters`**: A JSON schema of the arguments the tool accepts.\n- **`strict`**: Whether the arguments must match the schema exactly.";
        assert_eq!(
            located(
                text,
                "Before a model can call a tool, it needs a structured description of what the tool does, its arguments and its return values."
            ),
            Some("Before a model can call a tool, it needs a structured description of what the tool does, its arguments, and its return values.")
        );
        assert_eq!(
            located(
                text,
                "The type field of a tool definition should always be \"function\"."
            ),
            Some("- **`type`**: Should always be `\"function\"` [5].")
        );
    }

    #[test]
    fn a_history_statement_drawn_from_three_sentences_keeps_its_passage() {
        let text = "The Peace of Westphalia was signed in 1648 in the cities of Osnabrück and Münster. It ended the Thirty Years' War in the Holy Roman Empire. It also closed the Eighty Years' War between Spain and the Dutch Republic. Historians often date the modern order of sovereign states from these treaties.";
        assert_eq!(
            located(
                text,
                "Signed in 1648, the Peace of Westphalia ended the Thirty Years' War and is often seen by historians as the start of the order of sovereign states."
            ),
            None
        );
    }

    #[test]
    fn a_statute_subsection_is_located_in_a_law_passage() {
        let text = "Section 12. Notice to end a tenancy\n(1) A landlord must give a tenant at least two months' written notice before ending a periodic tenancy.\n(2) A tenant must give a landlord at least one month's written notice before ending a periodic tenancy.\n(3) A notice under this section must be in writing and must state the date on which the tenancy ends.";
        assert_eq!(
            located(
                text,
                "A tenant must give the landlord at least one month's written notice to end a periodic tenancy."
            ),
            Some("(2) A tenant must give a landlord at least one month's written notice before ending a periodic tenancy.")
        );
    }

    #[test]
    fn inflections_separators_and_diacritics_match_as_the_passage_writes_them() {
        assert_eq!(
            terms("Schemas, lists: 3,000 rows in 2.5 s at the café, e.g. don’t a 7."),
            [
                "schemas", "lists", "3,000", "rows", "in", "2.5", "at", "the", "cafe", "e.g",
                "don't", "7"
            ]
        );
        let text = "The service checks every request against one of two schemas. Results come back as lists of records. The archive holds 3,000 photographs taken over 2.5 decades. The café on the square opened in the old station.";
        assert_eq!(
            located(text, "The service checks each request against a schema"),
            Some("The service checks every request against one of two schemas.")
        );
        assert_eq!(
            located(text, "Results are returned as a list of records"),
            Some("Results come back as lists of records.")
        );
        assert_eq!(
            located(text, "The archive holds 3,000 photographs from 2.5 decades"),
            Some("The archive holds 3,000 photographs taken over 2.5 decades.")
        );
        assert_eq!(
            located(text, "The cafe on the square opened in a station"),
            Some("The café on the square opened in the old station.")
        );
    }

    #[test]
    fn a_sentence_cut_by_a_passage_boundary_is_taken_whole_and_locating_is_idempotent() {
        let text = "Enzymes speed up reactions. Each enzyme binds its substrate at an active site whose shape matches that substrate closely. Temperature changes the shape.";
        let statement = "An enzyme binds its substrate at an active site shaped to match it";
        let cut = text.find("active site").unwrap();
        let span = claim_sentence(text, (0, cut), statement).unwrap();
        assert_eq!(
            &text[span.0..span.1],
            "Each enzyme binds its substrate at an active site whose shape matches that substrate closely."
        );
        assert_eq!(claim_sentence(text, span, statement), Some(span));
    }
}
