//! Tests for memory patch validation and application.

#![cfg(test)]

use super::*;
use std::collections::HashMap;

fn message(id: &str, sequence: i64, role: SourceRole, content: &str) -> SourceMessage {
    SourceMessage {
        id: id.to_string(),
        conversation_id: "c1".into(),
        sequence,
        role,
        content: content.to_string(),
        content_digest: compute_digest(content),
        status: "completed".into(),
    }
}

fn evidence(message_id: &str, quote: &str, purpose: &str) -> ProposedEvidence {
    ProposedEvidence {
        message_id: message_id.into(),
        quote: quote.into(),
        occurrence: None,
        purpose: purpose.into(),
    }
}

fn patch(add: Vec<ProposedItem>, transitions: Vec<ProposedTransition>) -> MemoryPatch {
    MemoryPatch {
        schema_version: MEMORY_SCHEMA_VERSION,
        add,
        transitions,
        processed_segment_ids: Vec::new(),
    }
}

fn constraint(candidate: &str, label: &str, evidence: Vec<ProposedEvidence>) -> ProposedItem {
    ProposedItem {
        candidate_id: candidate.into(),
        kind: "constraint".into(),
        label: label.into(),
        evidence,
    }
}

fn item(id: &str, kind: MemoryKind, sequence: i64, label: &str) -> MemoryItem {
    MemoryItem {
        id: MemoryId::from_string(id).unwrap(),
        conversation_id: "c1".into(),
        kind,
        state: MemoryState::Active,
        label: label.into(),
        evidence: vec![EvidenceSpan {
            message_id: "m1".into(),
            sequence,
            role: SourceRole::User,
            start_byte: 0,
            end_byte: 4,
            content_digest: compute_digest("word"),
            purpose: EvidencePurpose::Assertion,
        }],
        created_at_sequence: sequence,
        changed_at_sequence: sequence,
        superseded_by: None,
        revision: 1,
        review: MemoryReview::Supported,
        related_item_ids: Vec::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn a_quote_resolves_to_the_exact_source_bytes() {
    let content = "Do not deploy until I approve. Ship the café build → prod.";
    let messages = vec![message("m1", 1, SourceRole::User, content)];
    let sources = SourceIndex::new(&messages);

    let validated = validate_patch(
        &patch(
            vec![constraint(
                "c1",
                "Approval required",
                vec![evidence(
                    "m1",
                    "Do not deploy until I approve.",
                    "assertion",
                )],
            )],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .expect("valid patch");

    let span = &validated.additions[0].evidence[0];
    assert_eq!(
        span.resolve(content),
        Some("Do not deploy until I approve.")
    );

    // Multi-byte characters keep their offsets: the span is bytes, not chars.
    let unicode = validate_patch(
        &patch(
            vec![constraint(
                "c2",
                "Build target",
                vec![evidence("m1", "café build → prod", "assertion")],
            )],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .expect("valid patch");
    assert_eq!(
        unicode.additions[0].evidence[0].resolve(content),
        Some("café build → prod")
    );
}

#[test]
fn an_invented_quote_is_never_fuzzy_matched_into_a_real_source() {
    let messages = vec![message(
        "m1",
        1,
        SourceRole::User,
        "Do not deploy until I approve.",
    )];
    let sources = SourceIndex::new(&messages);

    // One word changed. Nothing about this is close enough to accept.
    let error = validate_patch(
        &patch(
            vec![constraint(
                "c1",
                "Invented",
                vec![evidence(
                    "m1",
                    "Do not deploy until we approve.",
                    "assertion",
                )],
            )],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "quote_not_found");

    // A foreign message id is not resolvable at all.
    let error = validate_patch(
        &patch(
            vec![constraint(
                "c1",
                "Foreign",
                vec![evidence("m99", "Do not deploy", "assertion")],
            )],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "unknown_source");
}

#[test]
fn an_assistant_approval_cannot_become_a_user_constraint() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Do not deploy."),
        message("m2", 2, SourceRole::Assistant, "Deployment approved."),
    ];
    let sources = SourceIndex::new(&messages);

    let error = validate_patch(
        &patch(
            vec![constraint(
                "c1",
                "Deployment allowed",
                vec![evidence("m2", "Deployment approved.", "assertion")],
            )],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "non_user_authority");
}

#[test]
fn an_assistant_quote_cannot_ride_along_as_assertion_authority() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Do not deploy."),
        message("m2", 2, SourceRole::Assistant, "Understood. Do not deploy."),
    ];
    let sources = SourceIndex::new(&messages);

    let error = validate_patch(
        &patch(
            vec![constraint(
                "c1",
                "Deployment prohibited",
                vec![
                    evidence("m1", "Do not deploy.", "assertion"),
                    evidence("m2", "Understood. Do not deploy.", "assertion"),
                ],
            )],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "non_user_authority");
}

#[test]
fn an_assistant_antecedent_can_explain_a_short_user_assertion() {
    let messages = vec![
        message(
            "m1",
            1,
            SourceRole::Assistant,
            "Should the deployment target option B?",
        ),
        message("m2", 2, SourceRole::User, "Yes, option B."),
    ];
    let sources = SourceIndex::new(&messages);

    let validated = validate_patch(
        &patch(
            vec![constraint(
                "c1",
                "Use deployment option B",
                vec![
                    evidence("m1", "Should the deployment target option B?", "antecedent"),
                    evidence("m2", "Yes, option B.", "assertion"),
                ],
            )],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .expect("assistant antecedent plus user assertion is valid");

    assert_eq!(validated.additions[0].evidence.len(), 2);
    assert_eq!(
        validated.additions[0].evidence[0].purpose,
        EvidencePurpose::Antecedent
    );
}

#[test]
fn a_sourced_assistant_passage_can_create_an_established_fact() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "What should I plant first?"),
        message(
            "m2",
            2,
            SourceRole::Assistant,
            "Broccoli is a cool-season crop [1] (https://example.edu/broccoli).",
        ),
    ];
    let sources = SourceIndex::new(&messages);

    let validated = validate_patch(
        &patch(
            vec![ProposedItem {
                candidate_id: "c1".into(),
                kind: "established_fact".into(),
                label: "Broccoli is a cool-season crop (example.edu)".into(),
                evidence: vec![evidence(
                    "m2",
                    "Broccoli is a cool-season crop [1] (https://example.edu/broccoli).",
                    "assertion",
                )],
            }],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .expect("a sourced assistant passage is the authority for an established fact");

    let addition = &validated.additions[0];
    assert_eq!(addition.kind, MemoryKind::EstablishedFact);
    assert_eq!(addition.evidence[0].role, SourceRole::Assistant);
    assert!(!addition.kind.is_mandatory());
}

#[test]
fn an_established_fact_without_an_assertion_is_not_one() {
    let messages = vec![message(
        "m2",
        2,
        SourceRole::Assistant,
        "Broccoli is a cool-season crop (https://example.edu/broccoli).",
    )];
    let sources = SourceIndex::new(&messages);

    let error = validate_patch(
        &patch(
            vec![ProposedItem {
                candidate_id: "c1".into(),
                kind: "established_fact".into(),
                label: "Broccoli is a cool-season crop".into(),
                evidence: vec![evidence(
                    "m2",
                    "Broccoli is a cool-season crop",
                    "antecedent",
                )],
            }],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "no_assertion");
}

#[test]
fn a_later_sourced_passage_can_correct_an_established_fact() {
    let messages = vec![message(
        "m3",
        5,
        SourceRole::Assistant,
        "Correction: potatoes need a warm season [2] (https://example.edu/potatoes).",
    )];
    let sources = SourceIndex::new(&messages);
    let existing = vec![item(
        "i1",
        MemoryKind::EstablishedFact,
        1,
        "Potatoes are a cool-season crop",
    )];

    let validated = validate_patch(
        &patch(
            vec![ProposedItem {
                candidate_id: "c1".into(),
                kind: "established_fact".into(),
                label: "Potatoes need a warm season (example.edu)".into(),
                evidence: vec![evidence(
                    "m3",
                    "potatoes need a warm season [2] (https://example.edu/potatoes).",
                    "assertion",
                )],
            }],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "superseded".into(),
                replacement_candidate_id: Some("c1".into()),
                evidence: vec![evidence(
                    "m3",
                    "Correction: potatoes need a warm season",
                    "transition",
                )],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .expect("a later sourced assistant passage may correct an established fact");

    assert_eq!(validated.transitions.len(), 1);

    // The carve-out ends there: an assistant passage still cannot retire a
    // user requirement.
    let constraint = vec![item("i2", MemoryKind::Constraint, 1, "Do not deploy")];
    let rejected = validate_patch(
        &patch(
            vec![],
            vec![ProposedTransition {
                item_id: "i2".into(),
                state: "resolved".into(),
                replacement_candidate_id: None,
                evidence: vec![evidence(
                    "m3",
                    "Correction: potatoes need a warm season",
                    "transition",
                )],
            }],
        ),
        &sources,
        &constraint,
        &[],
    )
    .unwrap_err();
    assert_eq!(rejected.code(), "non_user_authority");
}

#[test]
fn transitions_require_user_transition_evidence() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Do not deploy."),
        message("m2", 5, SourceRole::User, "Deployment is allowed now."),
        message("m3", 6, SourceRole::Assistant, "Deployment is allowed now."),
    ];
    let sources = SourceIndex::new(&messages);
    let existing = vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")];

    let wrong_purpose = validate_patch(
        &patch(
            vec![],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "resolved".into(),
                replacement_candidate_id: None,
                evidence: vec![evidence("m2", "Deployment is allowed now.", "assertion")],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .unwrap_err();
    assert_eq!(wrong_purpose.code(), "invalid_evidence_purpose");

    let assistant_transition = validate_patch(
        &patch(
            vec![],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "resolved".into(),
                replacement_candidate_id: None,
                evidence: vec![evidence("m3", "Deployment is allowed now.", "transition")],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .unwrap_err();
    assert_eq!(assistant_transition.code(), "non_user_authority");
}

#[test]
fn a_failed_assistant_output_is_not_evidence_but_a_pending_user_message_is() {
    let mut failed = message("m2", 2, SourceRole::Assistant, "Half an answer.");
    failed.status = "failed".into();
    let mut pending = message("m3", 3, SourceRole::User, "Do not send the email.");
    pending.status = "pending".into();
    assert!(!failed.is_eligible_source());
    // A generation failure must not erase what the user asked for.
    assert!(pending.is_eligible_source());
}

#[test]
fn an_ambiguous_repeated_quote_must_say_which_occurrence() {
    let content = "use Rust. Actually, use Rust.";
    let messages = vec![message("m1", 1, SourceRole::User, content)];
    let sources = SourceIndex::new(&messages);

    let error = validate_patch(
        &patch(
            vec![constraint(
                "c1",
                "Language",
                vec![evidence("m1", "use Rust", "assertion")],
            )],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "ambiguous_occurrence");

    // Naming the occurrence resolves it, and the offsets differ.
    let mut selected = evidence("m1", "use Rust", "assertion");
    selected.occurrence = Some(1);
    let validated = validate_patch(
        &patch(vec![constraint("c1", "Language", vec![selected])], vec![]),
        &sources,
        &[],
        &[],
    )
    .expect("valid patch");
    assert_eq!(validated.additions[0].evidence[0].start_byte, 20);
}

#[test]
fn a_transition_needs_later_user_evidence_than_what_it_retires() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Budget is $120."),
        message(
            "m2",
            2,
            SourceRole::User,
            "Correction: $80 for this project.",
        ),
        message("m0", 0, SourceRole::User, "Earlier unrelated text."),
    ];
    let sources = SourceIndex::new(&messages);
    let existing = vec![item("i1", MemoryKind::Decision, 1, "Budget $120")];

    // Evidence that predates the item cannot retire it.
    let error = validate_patch(
        &patch(
            vec![],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "resolved".into(),
                replacement_candidate_id: None,
                evidence: vec![evidence("m0", "Earlier unrelated text.", "transition")],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "transition_not_later");

    // A later user correction is accepted.
    let validated = validate_patch(
        &patch(
            vec![constraint(
                "c1",
                "Budget $80",
                vec![evidence(
                    "m2",
                    "Correction: $80 for this project.",
                    "assertion",
                )],
            )],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "superseded".into(),
                replacement_candidate_id: Some("c1".into()),
                evidence: vec![evidence("m2", "Correction: $80", "transition")],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .expect("valid patch");
    assert_eq!(validated.transitions.len(), 1);
}

#[test]
fn duplicate_additions_and_unlinked_supersessions_are_rejected() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Use a fixed seed."),
        message(
            "m2",
            2,
            SourceRole::User,
            "Restated: every output must use a fixed seed.",
        ),
    ];
    let sources = SourceIndex::new(&messages);
    let duplicate = ProposedItem {
        candidate_id: "second".into(),
        kind: "constraint".into(),
        label: "Every output must use a fixed seed".into(),
        evidence: vec![evidence(
            "m2",
            "Restated: every output must use a fixed seed.",
            "assertion",
        )],
    };
    let error = validate_patch(
        &patch(
            vec![
                ProposedItem {
                    candidate_id: "first".into(),
                    ..duplicate.clone()
                },
                duplicate,
            ],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "duplicate_addition");

    let existing = vec![item("i1", MemoryKind::Constraint, 1, "Use a fixed seed")];
    let error = validate_patch(
        &patch(
            vec![],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "superseded".into(),
                replacement_candidate_id: None,
                evidence: vec![evidence(
                    "m2",
                    "Restated: every output must use a fixed seed.",
                    "transition",
                )],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "supersession_without_replacement");
}

#[test]
fn omitting_an_item_leaves_it_active_and_unchanged() {
    let existing = vec![
        item("i1", MemoryKind::Constraint, 1, "Do not deploy"),
        item("i2", MemoryKind::Goal, 2, "Ship the importer"),
    ];
    let commit = apply_patch(
        "c1",
        &ValidatedPatch::default(),
        &existing,
        &HashMap::new(),
        &HashMap::new(),
        2,
        10,
    );
    assert!(commit.inserts.is_empty());
    assert!(commit.updates.is_empty());
}

#[test]
fn an_unsupported_retirement_is_ignored_instead_of_inventing_a_conflict() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Do not deploy."),
        message("m2", 5, SourceRole::User, "Looks good."),
    ];
    let sources = SourceIndex::new(&messages);
    let existing = vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")];

    let validated = validate_patch(
        &patch(
            vec![],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "resolved".into(),
                replacement_candidate_id: None,
                evidence: vec![evidence("m2", "Looks good.", "transition")],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .expect("spans are real even though the reading is not");

    let mut verdicts = HashMap::new();
    verdicts.insert("i1".to_string(), SemanticVerdict::Unsupported);
    let commit = apply_patch(
        "c1",
        &validated,
        &existing,
        &HashMap::new(),
        &verdicts,
        2,
        10,
    );

    // The restriction is untouched and an extractor hallucination does not
    // become a durable conflict.
    assert!(commit.updates.is_empty());
    assert!(commit.inserts.is_empty());
}

#[test]
fn an_ambiguous_retirement_preserves_both_sides_as_a_conflict() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Do not deploy."),
        message("m2", 5, SourceRole::User, "Maybe staging is okay."),
    ];
    let sources = SourceIndex::new(&messages);
    let existing = vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")];
    let validated = validate_patch(
        &patch(
            vec![],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "resolved".into(),
                replacement_candidate_id: None,
                evidence: vec![evidence("m2", "Maybe staging is okay.", "transition")],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .expect("ambiguous source spans are still valid evidence");

    let mut verdicts = HashMap::new();
    verdicts.insert("i1".to_string(), SemanticVerdict::Ambiguous);
    let commit = apply_patch(
        "c1",
        &validated,
        &existing,
        &HashMap::new(),
        &verdicts,
        2,
        10,
    );

    assert!(commit.updates.is_empty());
    assert_eq!(commit.inserts.len(), 1);
    let conflict = &commit.inserts[0];
    assert_eq!(conflict.kind, MemoryKind::UnresolvedChange);
    assert_eq!(conflict.review, MemoryReview::Ambiguous);
    assert_eq!(conflict.related_item_ids[0].as_str(), "i1");
}

#[test]
fn a_supported_retirement_links_the_replacement_it_actually_persisted() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Use Python."),
        message("m2", 4, SourceRole::User, "Actually use Rust."),
    ];
    let sources = SourceIndex::new(&messages);
    let existing = vec![item("i1", MemoryKind::Decision, 1, "Use Python")];

    let validated = validate_patch(
        &patch(
            vec![ProposedItem {
                candidate_id: "c1".into(),
                kind: "decision".into(),
                label: "Use Rust".into(),
                evidence: vec![evidence("m2", "Actually use Rust.", "assertion")],
            }],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "superseded".into(),
                replacement_candidate_id: Some("c1".into()),
                evidence: vec![evidence("m2", "Actually use Rust.", "transition")],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .expect("valid patch");

    let mut verdicts = HashMap::new();
    verdicts.insert("i1".to_string(), SemanticVerdict::Supported);
    let commit = apply_patch(
        "c1",
        &validated,
        &existing,
        &HashMap::new(),
        &verdicts,
        2,
        10,
    );

    let replacement = &commit.inserts[0];
    let retired = &commit.updates[0];
    assert_eq!(retired.state, MemoryState::Superseded);
    assert_eq!(retired.superseded_by.as_ref(), Some(&replacement.id));
}

#[test]
fn a_rejected_replacement_cannot_retire_the_item_it_would_replace() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Never deploy on Friday."),
        message("m2", 4, SourceRole::User, "Deploy on Friday from now on."),
    ];
    let sources = SourceIndex::new(&messages);
    let existing = vec![item(
        "i1",
        MemoryKind::Constraint,
        1,
        "Never deploy on Friday",
    )];
    let validated = validate_patch(
        &patch(
            vec![ProposedItem {
                candidate_id: "new-rule".into(),
                kind: "constraint".into(),
                label: "Deploy on Friday".into(),
                evidence: vec![evidence("m2", "Deploy on Friday from now on.", "assertion")],
            }],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "superseded".into(),
                replacement_candidate_id: Some("new-rule".into()),
                evidence: vec![evidence(
                    "m2",
                    "Deploy on Friday from now on.",
                    "transition",
                )],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .expect("valid structural patch");

    let additions = HashMap::from([("new-rule".to_string(), SemanticVerdict::Unsupported)]);
    let transitions = HashMap::from([("i1".to_string(), SemanticVerdict::Supported)]);
    let commit = apply_patch("c1", &validated, &existing, &additions, &transitions, 2, 4);

    assert!(commit.inserts.is_empty());
    assert!(commit.updates.is_empty());
}

#[test]
fn a_same_response_candidate_can_be_superseded_by_a_later_candidate() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Budget is $120."),
        message("m2", 4, SourceRole::User, "Correction: budget is $80."),
    ];
    let sources = SourceIndex::new(&messages);
    let validated = validate_patch(
        &patch(
            vec![
                ProposedItem {
                    candidate_id: "old-budget".into(),
                    kind: "decision".into(),
                    label: "Budget $120".into(),
                    evidence: vec![evidence("m1", "Budget is $120.", "assertion")],
                },
                ProposedItem {
                    candidate_id: "new-budget".into(),
                    kind: "decision".into(),
                    label: "Budget $80".into(),
                    evidence: vec![evidence("m2", "Correction: budget is $80.", "assertion")],
                },
            ],
            vec![ProposedTransition {
                item_id: "old-budget".into(),
                state: "superseded".into(),
                replacement_candidate_id: Some("new-budget".into()),
                evidence: vec![evidence("m2", "Correction: budget is $80.", "transition")],
            }],
        ),
        &sources,
        &[],
        &[],
    )
    .expect("same-response candidate target is valid");

    assert!(matches!(
        &validated.transitions[0].target,
        ValidatedTransitionTarget::Candidate(id) if id == "old-budget"
    ));
    let addition_verdicts = HashMap::from([
        ("old-budget".to_string(), SemanticVerdict::Supported),
        ("new-budget".to_string(), SemanticVerdict::Supported),
    ]);
    let transition_verdicts =
        HashMap::from([("old-budget".to_string(), SemanticVerdict::Supported)]);
    let commit = apply_patch(
        "c1",
        &validated,
        &[],
        &addition_verdicts,
        &transition_verdicts,
        1,
        4,
    );

    assert!(commit.updates.is_empty());
    let old = commit
        .inserts
        .iter()
        .find(|item| item.label == "Budget $120")
        .expect("old value remains readable as history");
    let new = commit
        .inserts
        .iter()
        .find(|item| item.label == "Budget $80")
        .expect("replacement is stored");
    assert_eq!(old.state, MemoryState::Superseded);
    assert_eq!(old.superseded_by.as_ref(), Some(&new.id));
    assert_eq!(new.state, MemoryState::Active);
}

#[test]
fn self_replacement_and_conflicting_transitions_are_rejected() {
    let messages = vec![
        message("m1", 1, SourceRole::User, "Do not deploy."),
        message("m2", 5, SourceRole::User, "Deploy staging only."),
    ];
    let sources = SourceIndex::new(&messages);
    let existing = vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")];

    let transition = |state: &str| ProposedTransition {
        item_id: "i1".into(),
        state: state.into(),
        replacement_candidate_id: None,
        evidence: vec![evidence("m2", "Deploy staging only.", "transition")],
    };
    let error = validate_patch(
        &patch(vec![], vec![transition("resolved"), transition("resolved")]),
        &sources,
        &existing,
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), "conflicting_transitions");

    let error = validate_patch(
        &patch(
            vec![],
            vec![ProposedTransition {
                item_id: "i1".into(),
                state: "superseded".into(),
                replacement_candidate_id: Some("i1".into()),
                evidence: vec![evidence("m2", "Deploy staging only.", "transition")],
            }],
        ),
        &sources,
        &existing,
        &[],
    )
    .unwrap_err();
    // Named as its own replacement, but "i1" is not a candidate in this
    // response, so the unknown-replacement check fires first. Either way it
    // never commits.
    assert_eq!(error.code(), "unknown_replacement");
}

#[test]
fn an_incomplete_coverage_report_is_detected() {
    let messages = vec![message("m1", 1, SourceRole::User, "Do not deploy.")];
    let sources = SourceIndex::new(&messages);
    let mut input = patch(vec![], vec![]);
    input.processed_segment_ids = vec!["m1:s0".into()];

    let error = validate_patch(
        &input,
        &sources,
        &[],
        &["m1:s0".to_string(), "m1:s1".to_string()],
    )
    .unwrap_err();
    assert_eq!(error.code(), "incomplete_coverage");
}

#[test]
fn limits_are_enforced_before_anything_is_allocated() {
    let messages = vec![message("m1", 1, SourceRole::User, "Do not deploy.")];
    let sources = SourceIndex::new(&messages);

    let many: Vec<_> = (0..MAX_OPERATIONS_PER_RESPONSE + 1)
        .map(|i| {
            constraint(
                &format!("c{i}"),
                "x",
                vec![evidence("m1", "Do not deploy.", "assertion")],
            )
        })
        .collect();
    assert_eq!(
        validate_patch(&patch(many, vec![]), &sources, &[], &[])
            .unwrap_err()
            .code(),
        "too_many_operations"
    );

    let long = "x".repeat(MAX_QUOTE_BYTES + 1);
    assert_eq!(
        validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "x",
                    vec![evidence("m1", &long, "assertion")]
                )],
                vec![]
            ),
            &sources,
            &[],
            &[]
        )
        .unwrap_err()
        .code(),
        "quote_too_long"
    );

    assert_eq!(
        parse_patch(&"x".repeat(MAX_PROPOSAL_BYTES + 1))
            .unwrap_err()
            .code(),
        "response_too_large"
    );
}

#[test]
fn parsing_accepts_one_fence_and_nothing_looser() {
    let body = r#"{"schema_version":1,"add":[],"transitions":[],"processed_segment_ids":[]}"#;
    assert!(parse_patch(body).is_ok());
    assert!(parse_patch(&format!("```json\n{body}\n```")).is_ok());
    // Commentary around the JSON is not repaired into acceptance.
    assert!(parse_patch(&format!("Here you go:\n{body}")).is_err());
    // An unknown field means the model answered a different schema.
    assert!(parse_patch(r#"{"schema_version":1,"notes":"hi"}"#).is_err());
    // A newer patch version is refused rather than partially understood.
    let messages: Vec<SourceMessage> = Vec::new();
    let sources = SourceIndex::new(&messages);
    let mut future = patch(vec![], vec![]);
    future.schema_version = MEMORY_SCHEMA_VERSION + 1;
    assert_eq!(
        validate_patch(&future, &sources, &[], &[])
            .unwrap_err()
            .code(),
        "unsupported_schema_version"
    );
}

#[test]
fn an_edited_source_stops_resolving_rather_than_re_pointing() {
    let original = "Do not deploy until I approve.";
    let messages = vec![message("m1", 1, SourceRole::User, original)];
    let sources = SourceIndex::new(&messages);
    let validated = validate_patch(
        &patch(
            vec![constraint(
                "c1",
                "Approval required",
                vec![evidence("m1", "Do not deploy", "assertion")],
            )],
            vec![],
        ),
        &sources,
        &[],
        &[],
    )
    .unwrap();
    let span = &validated.additions[0].evidence[0];

    assert!(span.resolve(original).is_some());
    // Same offsets, different text: the digest check refuses to hand back
    // bytes the user never wrote.
    assert_eq!(span.resolve("You may deploy without approval."), None);
}

#[test]
fn mandatory_kinds_are_the_ones_that_must_never_be_evicted() {
    for kind in [
        MemoryKind::Constraint,
        MemoryKind::Goal,
        MemoryKind::Decision,
        MemoryKind::UnresolvedChange,
    ] {
        assert!(kind.is_mandatory(), "{kind:?} must be mandatory");
    }
    for kind in [
        MemoryKind::UserFact,
        MemoryKind::Preference,
        MemoryKind::OpenQuestion,
    ] {
        assert!(!kind.is_mandatory(), "{kind:?} must be optional");
    }
}

#[test]
fn an_unknown_stored_schema_is_not_read_as_empty_memory() {
    let mut state = ConversationMemoryState::empty("c1");
    assert!(state.is_schema_supported());
    state.schema_version = MEMORY_SCHEMA_VERSION + 1;
    assert!(!state.is_schema_supported());

    let snapshot = MemorySnapshot {
        state,
        transcript_revision: 1,
        active_items: vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")],
        summary: None,
        summary_through_sequence: 0,
        latest_sequence: 1,
    };
    assert!(!snapshot.is_usable());
}

#[test]
fn a_refold_is_needed_only_while_the_summary_ends_before_the_window_gap() {
    let refold = SummaryRefold {
        window_start_sequence: 9,
        through_sequence: 12,
    };
    // Messages 5 to 8 were dropped and nothing covers them.
    assert!(refold.is_needed(4));
    // An earlier refold already reached the window: asking again would only
    // change the prompt's prefix a second time.
    assert!(!refold.is_needed(8));
    assert!(!refold.is_needed(12));
}
