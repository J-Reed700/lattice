use super::rewrite::{preserve_valid_citations, strip_new_citations};
use super::*;
use crate::features::learning::outline_linking::{receipt_key, CitationReceipts};

#[test]
fn citation_regressions_are_reverted_only_for_unchanged_claims_with_receipted_evidence() {
    let quote = "The `for` loop visits each item in the collection.";
    let source = LearningSourceDto {
        id: "source".into(),
        title: "Guide".into(),
        url: None,
        excerpt: quote.into(),
        acquired_at: 0,
    };
    let sources = std::slice::from_ref(&source);
    let hash = &outline_linking::source_hashes(sources)[0];
    let model = "test";
    let receipt = |kind: ItemKind, text: &str| {
        (
            receipt_key(model, kind, text, hash, quote),
            CitationReceipt {
                covers: true,
                reason: "Covers the item.".into(),
                checked_at: 0,
            },
        )
    };
    let receipts: CitationReceipts = [
        receipt(ItemKind::Summary, "A stable claim about loops."),
        receipt(ItemKind::Lesson, "Explain collection iteration."),
    ]
    .into_iter()
    .collect();
    let before = json!({"summary":{"text":"A stable claim about loops.","sourceIndex":0,"quote":quote},"outcomes":[{"text":"An unverified existing citation.","sourceIndex":0,"quote":quote}],"lessons":[{"objective":"Explain collection iteration.","sourceIndex":0,"quote":quote}]});
    let mut after = before.clone();
    after["summary"]["quote"] = json!("The for loop visits each item in the collection.");
    after["outcomes"][0]["sourceIndex"] = Value::Null;
    after["outcomes"][0]["quote"] = json!("");
    after["lessons"][0]["objective"] = json!("Explain how iteration skips every other item.");
    after["lessons"][0]["sourceIndex"] = Value::Null;
    after["lessons"][0]["quote"] = json!("");
    preserve_valid_citations(&before, &mut after, sources, &receipts, model);
    // Unchanged text with a receipted citation: the mangled quote is restored.
    assert_eq!(after["summary"]["quote"], quote);
    // Unchanged text whose old citation has no receipt: stays dropped.
    assert_eq!(after["outcomes"][0]["sourceIndex"], Value::Null);
    // Changed text never inherits the old citation.
    assert_eq!(after["lessons"][0]["sourceIndex"], Value::Null);
    assert_eq!(after["lessons"][0]["quote"], "");
}

#[test]
fn a_rewrite_keeps_the_citations_it_was_given_and_never_introduces_one() {
    let before = json!({"summary":{"text":"Kept.","sourceIndex":0,"quote":"An existing quote."},"outcomes":[{"text":"Plain.","sourceIndex":null,"quote":""}],"lessons":[{"objective":"Plain.","sourceIndex":null,"quote":""}]});
    let mut after = before.clone();
    after["outcomes"][0]["sourceIndex"] = json!(0);
    after["outcomes"][0]["quote"] = json!("A quote the rewrite added.");
    after["lessons"][0]["sourceIndex"] = json!(1);
    after["lessons"][0]["quote"] = json!("Another added quote.");
    strip_new_citations(&before, &mut after);
    assert_eq!(after["summary"]["quote"], "An existing quote.");
    assert_eq!(after["outcomes"][0]["sourceIndex"], Value::Null);
    assert_eq!(after["outcomes"][0]["quote"], "");
    assert_eq!(after["lessons"][0]["sourceIndex"], Value::Null);
    assert_eq!(after["lessons"][0]["quote"], "");
}

#[test]
fn a_located_passage_past_a_quotations_ceiling_is_cited_by_the_chunk_it_was_built_around() {
    use crate::features::learning::reference_collection::ReferencePassage;
    use lattice_core::application::services::claim_verification::ClaimVerdict;
    let excerpt = "A process records each observation with a stable identifier. ".repeat(30);
    let source = LearningSourceDto {
        id: "source".into(),
        title: "Guide".into(),
        url: None,
        excerpt: excerpt.clone(),
        acquired_at: 0,
    };
    let passage = |start: usize, end: usize| ReferencePassage {
        source_id: "source".into(),
        text: excerpt[start..end].to_owned(),
        start_byte: start,
        end_byte: end,
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
    };
    let located = passage(0, excerpt.len());
    let chunk = passage(305, 610);
    let found = super::evidence::locate(
        &located.text,
        ClaimVerdict::Supported,
        std::slice::from_ref(&located),
        std::slice::from_ref(&chunk),
        std::slice::from_ref(&source),
    );
    assert_eq!(
        found,
        Some(("source".to_owned(), chunk.text.trim().to_owned()))
    );
}
