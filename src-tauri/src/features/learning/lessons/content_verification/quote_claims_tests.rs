#![allow(clippy::unwrap_used)]
use super::*;
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use std::sync::Mutex;

const TABLE: &str = "| Practice | Effect |\n|---|---|\n| Keep descriptions action-oriented (\"Retrieve X given Y\") | Guides the model's call-or-not decision |\n| List all enum values explicitly | Prevents the model from inventing invalid values |\n";

fn source(id: &str, title: &str, text: String) -> LearningSourceDto {
    LearningSourceDto {
        id: id.into(),
        title: title.into(),
        url: None,
        excerpt: text,
        acquired_at: 1,
    }
}

fn sources() -> Vec<LearningSourceDto> {
    vec![
        source(
            "book",
            "The LLM Stack Book",
            format!(
                "{}{TABLE}{}",
                "Tool schemas, in brief. ".repeat(40),
                "More notes. ".repeat(40)
            ),
        ),
        source(
            "owasp",
            "Prompt Injection Prevention",
            "Validate and sanitize all user inputs before they reach the LLM.".into(),
        ),
    ]
}

/// The lesson cites the book as source index 0 and OWASP as index 1.
fn citations(references: &ReferenceCollection<'_>) -> Citations {
    Citations::new(
        HashMap::from([(
            0,
            references
                .cited_passages(&["List all enum values explicitly"])
                .unwrap(),
        )]),
        HashMap::from([(0, "book".to_owned()), (1, "owasp".to_owned())]),
    )
}

fn claim(statement: &str) -> Claim {
    Claim {
        quote: statement.into(),
        statement: statement.into(),
    }
}

#[test]
fn a_quotation_in_its_named_source_is_supported_by_the_matching_passage() {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources).unwrap();
    let statement = "Source 0 states: \u{201c}keep descriptions  ACTION-oriented (\u{201c}Retrieve X given Y\u{201d})\u{2026} Guides the model\u{2019}s call-or-not decision.\u{201d}";
    let finding = quote_claims::settle(&references, &citations(&references), 0, &claim(statement))
        .unwrap()
        .expect("the quotation is in the book");
    assert_eq!(finding.verdict, ClaimVerdict::Supported);
    assert!(finding.evidence.iter().all(|p| p.source_id == "book"));
    let quote = finding.supporting_quote.unwrap();
    assert!(quote.starts_with("Keep descriptions action-oriented"));
    assert!(finding.evidence.iter().any(|p| p.text.contains(&quote)));
    assert!(finding
        .evidence
        .iter()
        .any(|p| p.text.contains("Guides the model's call-or-not decision")));
}

#[test]
fn a_quotation_matches_only_the_source_it_names() {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources).unwrap();
    let citations = citations(&references);
    let owasp =
        "Source 1 says: \"Validate and sanitize all user inputs before they reach the LLM.\"";
    assert!(
        quote_claims::settle(&references, &citations, 3, &claim(owasp))
            .unwrap()
            .is_some()
    );
    let misattributed =
        "Source 0 says: \"Validate and sanitize all user inputs before they reach the LLM.\"";
    assert!(
        quote_claims::settle(&references, &citations, 3, &claim(misattributed))
            .unwrap()
            .is_none()
    );
}

#[test]
fn anything_but_a_whole_found_quotation_goes_to_the_model() {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources).unwrap();
    let citations = citations(&references);
    for statement in [
        // A changed word.
        "Source 0 states: \"List all enum values explicitly | Prevents the model from inventing any values\"",
        // An assertion after the quotation.
        "Source 0 states: \"List all enum values explicitly | Prevents the model from inventing invalid values\", so enums guarantee valid calls.",
        // Too short to be a quotation of its own.
        "Source 0 states: \"enum values\"",
        // No source named.
        "Experts say: \"List all enum values explicitly | Prevents the model from inventing invalid values\"",
        // Not a quotation.
        "Listing enum values prevents invalid values.",
    ] {
        assert!(
            quote_claims::settle(&references, &citations, 0, &claim(statement))
                .unwrap()
                .is_none(),
            "{statement}"
        );
    }
}

#[tokio::test]
async fn a_quoted_claim_is_settled_without_a_model_call() -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let offline = ScriptedModel {
        outputs: Mutex::new(Default::default()),
        prompts: Mutex::new(vec![]),
    };
    let statement = "The LLM Stack Book states: \"List all enum values explicitly | Prevents the model from inventing invalid values\"";
    let inventory = Inventory {
        units: vec![UnitClaims {
            index: 0,
            claims: vec![claim(statement)],
            non_factual_reason: String::new(),
        }],
    };
    let results = evidence_checks::check_with_citations(
        &offline,
        &references,
        &inventory,
        &[],
        &ClaimChecks::default(),
        &citations(&references),
    )
    .await?;
    assert_eq!(results[0].2.verdict, ClaimVerdict::Supported);
    assert!(offline.prompts.lock().unwrap().is_empty());
    Ok(())
}
