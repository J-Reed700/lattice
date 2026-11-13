//! Move claim quotes through text edits without changing assertions: a claim
//! on a sentence that stands is carried as it is, a claim whose sentence was
//! rewritten or cut leaves, and the text the edit wrote is listed beside, to be
//! read from its own words. This is bookkeeping, never an approval: coverage
//! and fidelity audit the complete revised section before any factual receipts
//! can be reused.
use super::*;
use lattice_core::domain::learning::claim_review::span_of;
use lattice_core::shared::text::segmentation::{align, Alignment, Fate, Span};
use std::collections::BTreeSet;

/// The text of each field of a section the extractor reads, by field.
pub(super) fn fields(content: &Value) -> BTreeMap<String, String> {
    let input = inputs(std::slice::from_ref(content));
    let mut fields = BTreeMap::<String, String>::new();
    for passage in input
        .first()
        .and_then(|v| v["passages"].as_array())
        .into_iter()
        .flatten()
    {
        if let (Some(path), Some(text)) = (passage["field"].as_str(), passage["text"].as_str()) {
            fields.entry(path.into()).or_default().push_str(text);
        }
    }
    fields
}

/// What an edit wrote into one field of a section.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Changed {
    pub(super) field: String,
    /// The spans of the field's new text that took the place of sentences or
    /// stand among them as new ones.
    pub(super) spans: Vec<Span>,
}

/// A saved inventory carried across an edit.
pub(super) struct Relocated {
    /// The saved inventory with the claims whose sentences stand, as they were.
    pub(super) inventory: UnitClaims,
    /// The text the edit wrote.
    pub(super) changed: Vec<Changed>,
}

/// `claim` after the edit: its quote where its sentence stands, or `None` when
/// the sentence was rewritten or cut. The claim is left as it is when its quote
/// is not in exactly one field of the old section, is in it twice, or still
/// stands in its field.
fn moved(
    claim: &Claim,
    (old, new): (&BTreeMap<String, String>, &BTreeMap<String, String>),
    alignments: &mut BTreeMap<String, Alignment>,
) -> Option<Claim> {
    let stays = || Some(claim.clone());
    if claim.quote.trim().is_empty() {
        return stays();
    }
    // Require an unambiguous original field; never move to a different
    // field (including an unendorsed assessment alternative).
    let mut homes = old.iter().filter(|(_, text)| text.contains(&claim.quote));
    let Some((path, was)) = homes.next() else {
        return stays();
    };
    if homes.next().is_some() || span_of(was, &claim.quote).is_none() {
        return stays();
    }
    // A field with no text left has no passages.
    let now = new.get(path).map_or("", String::as_str);
    if now.contains(&claim.quote) {
        return stays();
    }
    // An older pass's quote may be its whole passage: the claim's sentence in
    // it is what was kept, rewritten or cut.
    let (quote, span) = claim_ledger::narrow(&claim.quote, None, &claim.statement, was);
    let Some((start, end)) = span.or_else(|| was.find(&quote).map(|at| (at, at + quote.len())))
    else {
        return stays();
    };
    let alignment = alignments
        .entry(path.clone())
        .or_insert_with(|| align(was, now));
    match alignment.fate(start..end) {
        Fate::Kept(span) => match now.get(span) {
            Some(text) if !text.trim().is_empty() => Some(Claim {
                quote: text.to_owned(),
                statement: claim.statement.clone(),
            }),
            _ => stays(),
        },
        Fate::Rewritten(_) | Fate::Removed => None,
    }
}

/// The text the edit wrote, by field: the non-empty new spans of the hunks of
/// the fields' alignments, what a rewrite or an insertion put there.
fn written(
    (old, new): (&BTreeMap<String, String>, &BTreeMap<String, String>),
    alignments: &mut BTreeMap<String, Alignment>,
) -> Vec<Changed> {
    let fields: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    let mut changed = Vec::new();
    for field in fields {
        let was = old.get(field).map_or("", String::as_str);
        let now = new.get(field).map_or("", String::as_str);
        let alignment = alignments
            .entry(field.clone())
            .or_insert_with(|| align(was, now));
        let spans: Vec<Span> = alignment
            .hunks()
            .iter()
            .map(|(_, written)| written.clone())
            .filter(|written| !written.is_empty())
            .collect();
        if !spans.is_empty() {
            changed.push(Changed {
                field: field.clone(),
                spans,
            });
        }
    }
    changed
}

/// The saved inventory of `before` across the edit to `current`: the claims of
/// the sentences that stand, which cannot be carried unless their quotes are
/// still in the section, and the text the edit wrote.
pub(super) fn relocate(before: &Basis, current: &Value) -> Relocated {
    let (old, new) = (fields(&before.content), fields(current));
    let mut alignments = BTreeMap::new();
    let mut inventory = before.inventory.clone();
    inventory.claims = before
        .inventory
        .claims
        .iter()
        .filter_map(|claim| moved(claim, (&old, &new), &mut alignments))
        .filter(|claim| claim_ledger::stands_in_part(current, &claim.quote))
        .collect();
    let changed = written((&old, &new), &mut alignments);
    Relocated { inventory, changed }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A section whose body is `body`, with a claim for each `(quote,
    /// statement)` of `claims`.
    fn section(body: &str, claims: &[(&str, &str)]) -> Basis {
        Basis {
            content: json!({"body":body}),
            inventory: UnitClaims {
                index: 0,
                non_factual_reason: String::new(),
                claims: claims
                    .iter()
                    .map(|(quote, statement)| Claim {
                        quote: (*quote).into(),
                        statement: (*statement).into(),
                    })
                    .collect(),
            },
        }
    }

    /// The quote and the statement of each claim of `unit`.
    fn left(unit: &UnitClaims) -> Vec<(&str, &str)> {
        unit.claims
            .iter()
            .map(|claim| (claim.quote.as_str(), claim.statement.as_str()))
            .collect()
    }

    /// A piece of the text of `field`.
    fn at(field: &str, text: &str) -> (String, String) {
        (field.to_owned(), text.to_owned())
    }

    /// What the edit to `current` wrote: each span of the new text, with its
    /// field.
    fn wrote(result: &Relocated, current: &Value) -> Vec<(String, String)> {
        let now = fields(current);
        let mut wrote = Vec::new();
        for changed in &result.changed {
            let text = now.get(&changed.field).map_or("", String::as_str);
            for span in &changed.spans {
                let piece = text.get(span.clone()).unwrap_or_default();
                wrote.push(at(&changed.field, piece));
            }
        }
        wrote
    }

    #[test]
    fn a_single_fact_edit_does_not_require_neighboring_claim_relocations() {
        let original = "🧪 Sample A has two units. Sample B has three units.";
        let current = json!({"body":"🧪 Sample A has four units. Sample B has three units."});
        let before = section(
            original,
            &[
                (original, "Sample A has two units."),
                (original, "Sample B has three units."),
            ],
        );

        let result = relocate(&before, &current);

        // The neighbour is carried on its own sentence as it was saved. The
        // edited fact's claim leaves, and its new sentence is what was written.
        assert_eq!(
            left(&result.inventory),
            [("Sample B has three units.", "Sample B has three units.")]
        );
        assert_eq!(
            wrote(&result, &current),
            [at("/body", "🧪 Sample A has four units.")]
        );
    }

    #[test]
    fn relocation_never_changes_an_assertion_even_if_it_is_now_false() {
        let (two, three) = ("Sample A has two units.", "Sample B has three units.");
        let before = section(&format!("{two} {three}"), &[(two, two), (three, three)]);
        let current = json!({"body":format!("Sample A has four units. {three}")});

        let result = relocate(&before, &current);

        // A kept sentence's claim is carried as it was saved, whatever the edit
        // made of the words around it. A rewritten sentence's claim leaves with
        // the words it stated, and the new text is listed as written.
        assert_eq!(left(&result.inventory), [(three, three)]);
        assert_eq!(
            wrote(&result, &current),
            [at("/body", "Sample A has four units.")]
        );
    }

    #[test]
    fn a_claim_whose_sentence_was_cut_leaves_the_inventory_and_the_others_keep_theirs() {
        let sentences = [
            "Alpha holds the line.",
            "Bravo stands firm today.",
            "Charlie rests in the shade.",
        ];
        let before = section(&sentences.join(" "), &sentences.map(|s| (s, s)));
        let result = relocate(
            &before,
            &json!({"body":"Alpha holds the line. Charlie rests in the shade."}),
        );
        assert_eq!(
            left(&result.inventory),
            [
                ("Alpha holds the line.", "Alpha holds the line."),
                ("Charlie rests in the shade.", "Charlie rests in the shade.")
            ]
        );
        assert!(result.changed.is_empty(), "{:?}", result.changed);
    }

    #[test]
    fn two_edits_in_one_field_are_told_apart() {
        let sentences = [
            "Alpha holds the line.",
            "Bravo stands firm today.",
            "Charlie rests in the shade.",
            "Delta runs across the field.",
            "Echo waits by the gate.",
        ];
        let before = section(&sentences.join(" "), &sentences.map(|s| (s, s)));
        // Sentence 2 is cut and sentence 4 rewritten.
        let revised = json!({"body":"Alpha holds the line. Charlie rests in the shade. Delta sprints across the field. Echo waits by the gate."});
        let result = relocate(&before, &revised);
        assert_eq!(
            left(&result.inventory),
            [
                ("Alpha holds the line.", "Alpha holds the line."),
                ("Charlie rests in the shade.", "Charlie rests in the shade."),
                ("Echo waits by the gate.", "Echo waits by the gate.")
            ]
        );
        assert_eq!(
            wrote(&result, &revised),
            [at("/body", "Delta sprints across the field.")]
        );
    }

    #[test]
    fn an_older_passes_passage_wide_quote_is_narrowed_to_its_sentence_before_its_fate_is_decided() {
        let old = "Alpha holds the line. Bravo stands firm today. Charlie rests in the shade. Delta runs across the field.";
        let bravo = "Bravo stands firm today.";
        let delta = "Delta runs across the field.";
        let charlie = "Charlie rests in the shade.";
        // As a pass that quoted whole passages stored them.
        let before = section(old, &[(old, bravo), (old, delta), (old, charlie)]);

        // Bravo's sentence is cut: its claim goes, the others take their own.
        let result = relocate(
            &before,
            &json!({"body":"Alpha holds the line. Charlie rests in the shade. Delta runs across the field."}),
        );
        assert_eq!(
            left(&result.inventory),
            [(delta, delta), (charlie, charlie)]
        );

        // Delta's sentence is rewritten: its claim goes, and Bravo's and
        // Charlie's, untouched, are their own sentences now.
        let revised = json!({"body":"Alpha holds the line. Bravo stands firm today. Charlie rests in the shade. Delta sprints across the field."});
        let result = relocate(&before, &revised);
        assert_eq!(
            left(&result.inventory),
            [(bravo, bravo), (charlie, charlie)]
        );
        assert_eq!(
            wrote(&result, &revised),
            [at("/body", "Delta sprints across the field.")]
        );
    }

    #[test]
    fn a_quote_found_twice_or_in_two_fields_is_left_as_it_is() {
        // Twice in the field: which of the two the claim stood on is not known.
        let echo = ("Echo waits here.", "Echo waits here.");
        let before = section(
            "Echo waits here. Delta runs home. Echo waits here.",
            &[echo],
        );
        let result = relocate(
            &before,
            &json!({"body":"Echo waits here. Delta sprints home."}),
        );
        assert_eq!(left(&result.inventory), [echo]);

        // In two fields: a claim never moves to another field.
        let mut before = section("", &[echo]);
        before.content =
            json!({"body":"Echo waits here. Delta runs home.","explanation":"Echo waits here."});
        let result = relocate(
            &before,
            &json!({"body":"Echo waits here. Delta sprints home.","explanation":"Foxtrot swims on."}),
        );
        assert_eq!(left(&result.inventory), [echo]);
    }

    #[test]
    fn a_claim_whose_quote_stands_nowhere_in_the_edited_section_cannot_be_carried() {
        // Twice in the field, and in none of the new text: nothing can carry it.
        let echo = ("Echo waits here.", "Echo waits here.");
        let before = section(
            "Echo waits here. Delta runs home. Echo waits here.",
            &[echo],
        );
        let revised = json!({"body":"Delta runs home. Foxtrot swims on."});

        let result = relocate(&before, &revised);

        assert!(result.inventory.claims.is_empty());
        assert_eq!(wrote(&result, &revised), [at("/body", "Foxtrot swims on.")]);
    }

    #[test]
    fn a_field_left_without_text_takes_its_claims_with_it() {
        let alpha = ("Alpha holds the line.", "Alpha holds the line.");
        let bravo = ("Bravo stands firm today.", "Bravo stands firm today.");
        let mut before = section("", &[alpha, bravo]);
        before.content =
            json!({"body":"Alpha holds the line.","explanation":"Bravo stands firm today."});
        let result = relocate(
            &before,
            &json!({"body":"Alpha holds the line.","explanation":""}),
        );
        assert_eq!(left(&result.inventory), [alpha]);
        assert!(result.changed.is_empty(), "{:?}", result.changed);
    }

    #[test]
    fn the_text_an_edit_wrote_is_listed_by_field_whether_it_rewrote_a_sentence_or_added_one() {
        let alpha = ("Alpha holds the line.", "Alpha holds the line.");
        let bravo = ("Bravo stands firm today.", "Bravo stands firm today.");
        let mut before = section("", &[alpha, bravo]);
        before.content =
            json!({"body":"Alpha holds the line.","explanation":"Bravo stands firm today."});
        let revised = json!({"body":"Alpha holds the line. Delta runs home.","explanation":"Bravo stands firm now."});

        let result = relocate(&before, &revised);

        assert_eq!(left(&result.inventory), [alpha]);
        assert_eq!(
            wrote(&result, &revised),
            [
                at("/body", "Delta runs home."),
                at("/explanation", "Bravo stands firm now.")
            ]
        );
    }

    #[test]
    fn offsets_stay_on_char_boundaries_after_an_emoji_and_a_combining_accent() {
        let sentences = [
            "🧪 Cafe\u{301} stays open.",
            "Résumé files are kept.",
            "Closing time is nine.",
            "Staff lock the door.",
            "Guests leave quietly.",
        ];
        let old = sentences.join(" ");
        let mut claims: Vec<_> = sentences.iter().map(|s| (*s, *s)).collect();
        // A claim that an older pass quoted by its whole passage.
        claims.push((old.as_str(), "Guests leave quietly."));
        let before = section(&old, &claims);
        let revised = "🧪 Cafe\u{301} stays open. Closing time is nine. Staff lock every door. Guests leave quietly.";
        let current = json!({"body":revised});

        let result = relocate(&before, &current);

        let quotes: Vec<&str> = result
            .inventory
            .claims
            .iter()
            .map(|c| c.quote.as_str())
            .collect();
        assert_eq!(
            quotes,
            [
                "🧪 Cafe\u{301} stays open.",
                "Closing time is nine.",
                "Guests leave quietly.",
                "Guests leave quietly."
            ]
        );
        assert!(quotes.iter().all(|quote| revised.contains(quote)));
        assert_eq!(
            wrote(&result, &current),
            [at("/body", "Staff lock every door.")]
        );
    }

    #[test]
    fn a_sentence_rewritten_to_more_than_the_inventory_keeps_is_listed_whole_as_written() {
        let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
        let before = section(
            "Alpha holds the line. Bravo stands firm today.",
            &[("Alpha holds the line.", "Alpha holds the line.")],
        );
        let revised = json!({"body":format!("{long} Bravo stands firm today.")});

        let result = relocate(&before, &revised);

        assert!(result.inventory.claims.is_empty());
        assert_eq!(wrote(&result, &revised), [at("/body", &long)]);
    }
}
