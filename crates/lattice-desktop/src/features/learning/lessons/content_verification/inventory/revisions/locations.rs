//! Move quotation anchors through text edits without changing assertions.
//! This is bookkeeping, never an approval: coverage and fidelity audit the
//! complete revised section before any factual receipts can be reused.
use super::*;

fn fields(content: &Value) -> BTreeMap<String, String> {
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

fn anchor(old: &str, new: &str, quote: &str) -> Option<String> {
    let mut occurrences = old.match_indices(quote);
    let (start, _) = occurrences.next()?;
    if occurrences.next().is_some() {
        return None;
    }
    let start = old.get(..start)?.chars().count();
    let end = start + quote.chars().count();
    let old: Vec<_> = old.chars().collect();
    let new: Vec<_> = new.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(old.len().min(new.len()) - prefix)
        .take_while(|(a, b)| a == b)
        .count();
    // Any boundary inside the changed interval expands to include the edit.
    // Unicode character offsets keep the resulting quotation lossless.
    let mapped_start = if start <= prefix {
        start
    } else if start >= old.len() - suffix {
        new.len() - (old.len() - start)
    } else {
        prefix
    };
    let mapped_end = if end <= prefix {
        end
    } else if end >= old.len() - suffix {
        new.len() - (old.len() - end)
    } else {
        new.len() - suffix
    };
    let quote: String = new.get(mapped_start..mapped_end)?.iter().collect();
    (!quote.trim().is_empty() && quote.chars().count() <= 1600).then_some(quote)
}

pub(super) fn relocate(before: &Basis, current: &Value) -> UnitClaims {
    let old = fields(&before.content);
    let new = fields(current);
    let mut inventory = before.inventory.clone();
    for claim in &mut inventory.claims {
        // Require an unambiguous original field; never move to a different
        // field (including an unendorsed assessment alternative).
        let mut locations = old.iter().filter(|(_, text)| text.contains(&claim.quote));
        let Some((path, text)) = locations.next() else {
            continue;
        };
        if locations.next().is_some() {
            continue;
        }
        let Some(current) = new.get(path) else {
            continue;
        };
        if current.contains(&claim.quote) {
            continue;
        }
        if let Some(quote) = anchor(text, current, &claim.quote) {
            claim.quote = quote;
        }
    }
    inventory
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn a_single_fact_edit_does_not_require_neighboring_claim_relocations() {
        let original = "🧪 Sample A has two units. Sample B has three units.";
        let current = "🧪 Sample A has four units. Sample B has three units.";
        let before = Basis {
            content: json!({"body":original}),
            inventory: UnitClaims {
                index: 0,
                non_factual_reason: String::new(),
                claims: vec![
                    Claim {
                        quote: original.into(),
                        statement: "Sample A has two units.".into(),
                    },
                    Claim {
                        quote: original.into(),
                        statement: "Sample B has three units.".into(),
                    },
                ],
            },
        };
        let content = vec![json!({"body":current})];
        let relocated = relocate(&before, &content[0]);
        let revision = json!({"changes":[{"claimId":0,"passageId":"unit-0-passage-0","statement":"Sample A has four units."}],"nonFactualReason":""});
        let result = apply(
            &revision.to_string(),
            &relocated,
            &inputs(&content),
            &content,
        )
        .unwrap();
        assert_eq!(
            result.claims[1].statement,
            before.inventory.claims[1].statement
        );
        assert_eq!(result.claims[1].quote, "Sample B has three units.");
        assert_eq!(result.claims[0].statement, "Sample A has four units.");
    }

    #[test]
    fn offsets_survive_unicode_insertions_and_changed_chunk_boundaries() {
        let old = format!(
            "{}\nTarget: café is a word.\n{}",
            "🧪 ".repeat(300),
            "Later. ".repeat(200)
        );
        let quote = "Target: café is a word.\nLater. Later. ";
        let new = old
            .replacen("🧪", "A longer prefix 🧬", 1)
            .replace("café", "résumé");
        let relocated = anchor(&old, &new, quote).unwrap();
        assert!(new.contains(&relocated));
        assert!(relocated.contains("résumé"));
    }

    #[test]
    fn relocation_never_changes_or_drops_assertions_even_if_they_are_now_false() {
        let before = Basis {
            content: json!({"body":"Sample A has two units."}),
            inventory: UnitClaims {
                index: 0,
                non_factual_reason: String::new(),
                claims: vec![Claim {
                    quote: "Sample A has two units.".into(),
                    statement: "Sample A has two units.".into(),
                }],
            },
        };
        let result = relocate(&before, &json!({"body":"Sample A has four units."}));
        assert_eq!(
            result.claims[0].statement,
            before.inventory.claims[0].statement
        );
        assert_eq!(result.claims[0].quote, "Sample A has four units.");
        // The unchanged assertion must fail the independent fidelity check;
        // the location mapper cannot silently correct or approve it.
        assert!(anchor("Repeated. Repeated.", "Changed.", "Repeated.").is_none());
        assert!(anchor("Original.", &"A".repeat(1601), "Original.").is_none());
    }

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
            left(&result),
            [
                ("Alpha holds the line.", "Alpha holds the line."),
                ("Charlie rests in the shade.", "Charlie rests in the shade.")
            ]
        );
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
        let revised = "Alpha holds the line. Charlie rests in the shade. Delta sprints across the field. Echo waits by the gate.";
        let result = relocate(&before, &json!({"body":revised}));
        assert_eq!(
            left(&result),
            [
                ("Alpha holds the line.", "Alpha holds the line."),
                ("Charlie rests in the shade.", "Charlie rests in the shade."),
                (
                    "Delta sprints across the field.",
                    "Delta runs across the field."
                ),
                ("Echo waits by the gate.", "Echo waits by the gate.")
            ]
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
        assert_eq!(left(&result), [(delta, delta), (charlie, charlie)]);

        // Delta's sentence is rewritten: Bravo's and Charlie's, untouched, are
        // their own sentences now.
        let result = relocate(
            &before,
            &json!({"body":"Alpha holds the line. Bravo stands firm today. Charlie rests in the shade. Delta sprints across the field."}),
        );
        assert_eq!(
            left(&result),
            [
                (bravo, bravo),
                ("Delta sprints across the field.", delta),
                (charlie, charlie)
            ]
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
            &json!({"body":"Delta runs home. Foxtrot swims on."}),
        );
        assert_eq!(left(&result), [echo]);

        // In two fields: a claim never moves to another field.
        let mut before = section("", &[echo]);
        before.content =
            json!({"body":"Echo waits here. Delta runs home.","explanation":"Echo waits here."});
        let result = relocate(
            &before,
            &json!({"body":"Delta runs home.","explanation":"Foxtrot swims on."}),
        );
        assert_eq!(left(&result), [echo]);
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
        assert_eq!(left(&result), [alpha]);
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
        let result = relocate(&before, &json!({"body":revised}));
        let quotes: Vec<&str> = result.claims.iter().map(|c| c.quote.as_str()).collect();
        assert_eq!(
            quotes,
            [
                "🧪 Cafe\u{301} stays open.",
                "Closing time is nine.",
                "Staff lock every door.",
                "Guests leave quietly.",
                "Guests leave quietly."
            ]
        );
        assert!(quotes.iter().all(|quote| revised.contains(quote)));
        assert_eq!(result.claims[2].statement, "Staff lock the door.");
    }

    #[test]
    fn a_sentence_rewritten_past_what_the_inventory_keeps_is_quoted_by_its_first_1600_characters() {
        let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
        let before = section(
            "Alpha holds the line. Bravo stands firm today.",
            &[("Alpha holds the line.", "Alpha holds the line.")],
        );
        let revised = format!("{long} Bravo stands firm today.");
        let result = relocate(&before, &json!({"body":revised}));
        let quote = &result.claims[0].quote;
        assert_eq!(quote.chars().count(), 1_600);
        assert!(long.starts_with(quote.as_str()));
    }
}
