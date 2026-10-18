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
        assert_eq!(result.claims[1].quote, current);
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
}
