//! Identity, new information, re-litigation, the BLOCKED end state, and the
//! stored JSON forms.

use super::*;
use serde_json::json;

fn observation(entry: &ClaimLedgerEntry) -> Observation {
    let basis = entry.basis().unwrap();
    Observation {
        quote: entry.location().quote.clone(),
        fingerprint: basis.fingerprint.clone(),
        checker_model: basis.checker_model.clone(),
        window_tokens: basis.window_tokens,
        source_changes: Vec::new(),
        hint: None,
        attached_source_version_id: None,
        gate: None,
    }
}

fn extracted(quote: &str, statement: &str) -> ExtractedClaim {
    ExtractedClaim {
        location: location(quote),
        statement: statement.into(),
    }
}

#[test]
fn spans_name_a_single_place_or_none() {
    assert_eq!(span_of("a b a", "b"), Some((2, 3)));
    assert_eq!(span_of("a b a", "a"), None, "twice");
    assert_eq!(span_of("aaa", "aa"), None, "overlapping twice");
    assert_eq!(span_of("abc", ""), None);
    assert_eq!(span_of("abc", "x"), None);
    assert_eq!(span_of("é é!", "é!"), Some((3, 6)));
}

#[test]
fn identity_survives_a_reworded_statement_and_a_lightly_edited_sentence() {
    let old = claim("c1");
    let field = |_: usize, _: &str| Some(FIELD_TEXT);

    // Same quote, new wording of the statement.
    let same_quote = identities(&[&old], &[extracted(QUOTE, "Water's boiling point")], field);
    assert_eq!(same_quote.assigned, vec![Some(ClaimId("c1".into()))]);
    assert!(same_quote.orphaned.is_empty());

    // Same statement, different quote.
    let same_statement = identities(&[&old], &[extracted("Heat it.", old.statement())], field);
    assert_eq!(same_statement.assigned, vec![Some(ClaimId("c1".into()))]);

    // The extractor quoted part of the sentence: the spans overlap.
    let part = identities(
        &[&old],
        &[extracted("boils at 100 °C", "Water boils at 100 degrees")],
        field,
    );
    assert_eq!(part.assigned, vec![Some(ClaimId("c1".into()))]);
}

#[test]
fn identity_gives_each_prior_id_once_and_orphans_vanished_sentences() {
    let old = claim("c1");
    let other = ClaimLedgerEntry::extracted(
        ClaimId("c2".into()),
        machine(1),
        location("Then cool it."),
        "Cool it afterwards.".into(),
    )
    .unwrap();
    let field = |_: usize, _: &str| Some(FIELD_TEXT);
    let result = identities(
        &[&old, &other],
        &[
            extracted("Water boils", "Water boils"),
            extracted("boils at 100 °C at sea level", "Boiling point at sea level"),
        ],
        field,
    );
    // The larger overlap keeps c1; the smaller one is a new claim; c2's
    // sentence is not in the re-extraction.
    assert_eq!(result.assigned, vec![None, Some(ClaimId("c1".into()))]);
    assert_eq!(result.orphaned, vec![ClaimId("c2".into())]);

    // A prior quote that now occurs twice has no single place to overlap.
    let doubled = format!("{QUOTE} {QUOTE}");
    let ambiguous = identities(&[&old], &[extracted("boils at 100 °C", "x")], |_, _| {
        Some(doubled.as_str())
    });
    assert_eq!(ambiguous.assigned, vec![None]);
    assert_eq!(ambiguous.orphaned, vec![ClaimId("c1".into())]);
}

#[test]
fn new_information_names_each_cause_and_nothing_else() {
    let entry = queued_claim("c1", VerdictKind::Unsupported);
    let same = observation(&entry);
    assert_eq!(new_information(&entry, &same), None);

    let mut sentence = same.clone();
    sentence.quote = "Water boils at 99.97 °C at sea level.".into();
    assert_eq!(
        new_information(&entry, &sentence),
        Some(NewInformation::SentenceChanged {
            quote_sha256: text_sha256(&sentence.quote)
        })
    );

    let mut deleted = same.clone();
    deleted.source_changes = vec![
        SourceChange::Deleted("unrelated".into()),
        SourceChange::Replaced {
            from: "v1".into(),
            to: "v1b".into(),
        },
    ];
    assert_eq!(
        new_information(&entry, &deleted),
        Some(NewInformation::SourceChanged(SourceChange::Replaced {
            from: "v1".into(),
            to: "v1b".into()
        }))
    );
    let mut uncited = same.clone();
    uncited.source_changes = vec![SourceChange::Deleted("unrelated".into())];
    assert_eq!(new_information(&entry, &uncited), None);

    let mut model = same.clone();
    model.checker_model = "gemma-4-12b".into();
    let mut window = same.clone();
    window.window_tokens *= 2;
    for changed in [model, window] {
        assert_eq!(
            new_information(&entry, &changed),
            Some(NewInformation::CheckerChanged)
        );
    }

    let mut evidence = same.clone();
    evidence.fingerprint = local_check(None, "v2").fingerprint;
    assert_eq!(
        new_information(&entry, &evidence),
        Some(NewInformation::EvidenceChanged)
    );

    let mut attached = same.clone();
    attached.attached_source_version_id = Some("v7".into());
    attached.hint = Some("steam tables".into());
    assert_eq!(
        new_information(&entry, &attached),
        Some(NewInformation::UserSource("v7".into()))
    );
    let mut hint = same;
    hint.hint = Some("steam tables".into());
    assert_eq!(
        new_information(&entry, &hint),
        Some(NewInformation::UserHint("steam tables".into()))
    );

    let unchecked = claim("c2");
    let mut first = observation(&entry);
    first.quote = QUOTE.into();
    assert_eq!(
        new_information(&unchecked, &first),
        Some(NewInformation::EvidenceChanged)
    );
}

#[test]
fn only_the_sentence_or_a_bound_source_reopens_a_confirmation() {
    let bound = HumanBinding::new(QUOTE, vec!["v1".into()]);
    let cases = [
        (
            NewInformation::SentenceChanged {
                quote_sha256: text_sha256("Water boils at 99.97 °C."),
            },
            Some(ReopenCause::SentenceChanged),
        ),
        (
            NewInformation::SentenceChanged {
                quote_sha256: text_sha256(QUOTE),
            },
            None,
        ),
        (
            NewInformation::SourceChanged(SourceChange::Deleted("v1".into())),
            Some(ReopenCause::SourceDeleted("v1".into())),
        ),
        (
            NewInformation::SourceChanged(SourceChange::Replaced {
                from: "v1".into(),
                to: "v2".into(),
            }),
            Some(ReopenCause::SourceReplaced {
                from: "v1".into(),
                to: "v2".into(),
            }),
        ),
        (
            NewInformation::SourceChanged(SourceChange::Deleted("v9".into())),
            None,
        ),
        (NewInformation::CheckerChanged, None),
        (NewInformation::EvidenceChanged, None),
        (NewInformation::UserSource("v9".into()), None),
        (NewInformation::UserHint("hint".into()), None),
        (NewInformation::DecisionChanged, None),
    ];
    for (change, expected) in cases {
        assert_eq!(relitigate(&bound, &change), expected, "{change:?}");
    }

    // A machine verification reopens only when its own source changes.
    let verified = verified_claim("c1");
    let deleted = NewInformation::SourceChanged(SourceChange::Deleted("v1".into()));
    assert_eq!(
        reopen_cause(verified.state(), &deleted),
        Some(ReopenCause::SourceDeleted("v1".into()))
    );
    let sentence = NewInformation::SentenceChanged {
        quote_sha256: text_sha256("other"),
    };
    assert_eq!(reopen_cause(verified.state(), &sentence), None);
}

#[test]
fn a_lesson_is_ready_only_when_every_claim_is_resolved() {
    let verified = verified_claim("c1");
    let mut confirmed = queued_claim("c2", VerdictKind::Contradicted);
    confirmed
        .append(user(7), act(confirm(), Some("Lab-measured.")))
        .unwrap();
    let mut removed = queued_claim("c3", VerdictKind::Unsupported);
    removed
        .append(user(7), act(UserAction::Remove, None))
        .unwrap();
    let mut general = claim("c4");
    general
        .append(
            machine(2),
            checked(Some(VerdictKind::GeneralKnowledgeProposed), "v1"),
        )
        .unwrap();

    assert_eq!(
        end_state([&verified, &confirmed, &removed, &general]),
        LessonReviewState::NeedsReview,
        "a general-knowledge proposal blocks until accepted"
    );
    general
        .append(user(3), act(UserAction::GeneralKnowledge, None))
        .unwrap();
    assert_eq!(
        end_state([&verified, &confirmed, &removed, &general]),
        LessonReviewState::Ready
    );
    let unchecked = claim("c5");
    assert_eq!(
        end_state([&verified, &unchecked]),
        LessonReviewState::Verifying
    );
    let mut reopened = verified_claim("c6");
    reopened
        .append(
            machine(3),
            ClaimEvent::Reopened {
                cause: ReopenCause::SourceDeleted("v1".into()),
            },
        )
        .unwrap();
    assert_eq!(
        end_state([&verified, &reopened]),
        LessonReviewState::NeedsReview
    );
    assert_eq!(
        ClaimCounts::tally([&verified, &confirmed, &removed, &general, &unchecked, &reopened]),
        ClaimCounts {
            verified: 1,
            confirmed: 1,
            general_knowledge: 1,
            queued: 1,
            open: 1,
            removed_by_you: 1,
            edited_by_you: 0,
            rewritten: 0,
        }
    );
}

#[test]
fn fingerprints_ignore_order_and_change_with_model_or_policy() {
    let (a, b) = (seen("v1"), seen("v2"));
    let forward = EvidenceFingerprint::of("strict-v1", MODEL, &[a.clone(), b.clone()]);
    let backward = EvidenceFingerprint::of("strict-v1", MODEL, &[b.clone(), a.clone()]);
    assert_eq!(forward, backward);
    assert_ne!(
        forward,
        EvidenceFingerprint::of("strict-v1", "other", &[a.clone(), b.clone()])
    );
    assert_ne!(
        forward,
        EvidenceFingerprint::of("strict-v2", MODEL, &[a, b])
    );
}

#[test]
fn stored_forms_round_trip_and_tag_the_events_kind_column() {
    let entry = queued_claim("c1", VerdictKind::Contradicted);
    let mut events: Vec<ClaimEvent> = entry.events().iter().map(|e| e.event.clone()).collect();
    events.extend([
        ClaimEvent::Researched {
            queries: vec!["q".into()],
            sources_added: vec!["v2".into()],
            passages_found: 3,
            round_ms_per_claim: None,
        },
        ClaimEvent::Executed {
            observation_id: "obs-1".into(),
            passed: true,
            evidence: "exit 0".into(),
        },
        act(confirm(), Some("note")),
        ClaimEvent::EvidenceAttached {
            source_version_id: "v3".into(),
            provenance: Provenance::Link {
                url: "https://example.org".into(),
            },
            result: AttachOutcome::Checked(local_check(Some(VerdictKind::NoSource), "v3")),
        },
        ClaimEvent::Superseded {
            successor: None,
            by: Actor::Machine,
        },
        ClaimEvent::BornOfRepair {
            parent: ClaimId("c0".into()),
            attempts_left: 1,
        },
        ClaimEvent::Reopened {
            cause: ReopenCause::SourceReplaced {
                from: "v1".into(),
                to: "v2".into(),
            },
        },
        ClaimEvent::Undone { event_ordinal: 4 },
        ClaimEvent::Stopped {
            reason: StopReason {
                code: "model_rejected".into(),
                message: "llama.cpp rejected this request twice".into(),
                fix: "Lower the context window in Settings.".into(),
                at: 9,
                request: Some(RequestFacts {
                    prompt_tokens_exact: Some(118_994),
                    prompt_tokens_estimated: None,
                    window_tokens: 131_072,
                    output_reserved: 8_192,
                    server_kind: "llama_cpp".into(),
                    server_message_sanitized: Some("(500 server_error) […]".into()),
                    attempts: 2,
                    step: "verify · pass 2".into(),
                }),
            },
        },
    ]);
    for event in events {
        let stored = serde_json::to_value(&event).unwrap();
        assert_eq!(stored["kind"], json!(event.kind_name()));
        assert_eq!(serde_json::from_value::<ClaimEvent>(stored).unwrap(), event);
    }

    let states = [
        ClaimState::Unchecked,
        entry.state().clone(),
        ClaimState::Reopened {
            cause: ReopenCause::VerifierDispute,
            previous: Box::new(ClaimState::Confirmed {
                at: 3,
                note: None,
                bound: HumanBinding::new(QUOTE, vec![]),
            }),
        },
        ClaimState::Queued {
            kind: VerdictKind::Disputed {
                verifier: Box::new(VerdictKind::Contradicted),
            },
            reason: "r".into(),
        },
    ];
    for state in states {
        let stored = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<ClaimState>(&stored).unwrap(), state);
    }
    assert_eq!(
        serde_json::to_value(entry.state()).unwrap(),
        json!({"state": "queued", "kind": "contradicted", "reason": "compared v1"})
    );
    for (value, column) in [
        (UnitKind::AnswerKey.as_str(), "answer_key"),
        (LessonReviewState::NeedsReview.as_str(), "needs_review"),
        (SourceAddedBy::Verifier.as_str(), "verifier"),
        (Actor::User.as_str(), "user"),
    ] {
        assert_eq!(value, column);
    }
    assert_eq!(
        UnitKind::from_column("answer_key"),
        Some(UnitKind::AnswerKey)
    );
    assert_eq!(LessonReviewState::from_column("bogus"), None);
}

#[test]
fn the_tally_counts_only_the_users_actions_as_resolved_by_you() {
    // The machine's: a rewrite whose sentence found no successor, and a
    // removal by repair.
    let mut rewritten = claim("c1");
    rewritten
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    rewritten
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    rewritten
        .append(
            machine(4),
            ClaimEvent::Superseded {
                successor: None,
                by: Actor::Machine,
            },
        )
        .unwrap();
    let mut removed_by_machine = claim("c2");
    removed_by_machine
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    removed_by_machine
        .append(machine(3), rewrite(1, false, RepairOutcome::Removed))
        .unwrap();
    // The user's: a removal and an edit.
    let mut removed_by_you = queued_claim("c3", VerdictKind::Unsupported);
    removed_by_you
        .append(user(7), act(UserAction::Remove, None))
        .unwrap();
    let mut edited_by_you = queued_claim("c4", VerdictKind::Unsupported);
    edited_by_you
        .append(
            user(7),
            act(
                UserAction::Edit {
                    before: QUOTE.into(),
                    after: "Water boils at 100 °C at one atmosphere.".into(),
                },
                None,
            ),
        )
        .unwrap();
    edited_by_you
        .append(
            machine(8),
            ClaimEvent::Superseded {
                successor: None,
                by: Actor::User,
            },
        )
        .unwrap();
    let counts = ClaimCounts::tally([
        &rewritten,
        &removed_by_machine,
        &removed_by_you,
        &edited_by_you,
    ]);
    assert_eq!(counts.rewritten, 2);
    assert_eq!(counts.removed_by_you, 1);
    assert_eq!(counts.edited_by_you, 1);
    assert_eq!(counts.resolved_by_you(), 2);

    // A Superseded event stored before it said who edited is the machine's.
    let stored: ClaimEvent =
        serde_json::from_value(json!({ "kind": "superseded", "successor": null })).unwrap();
    assert_eq!(
        stored,
        ClaimEvent::Superseded {
            successor: None,
            by: Actor::Machine,
        }
    );
}
