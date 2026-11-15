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
    let same_quote = identities(
        &[&old],
        &[],
        &[extracted(QUOTE, "Water's boiling point")],
        field,
    );
    assert_eq!(same_quote.assigned, vec![Some(ClaimId("c1".into()))]);
    assert!(same_quote.orphaned.is_empty());

    // Same statement, different quote.
    let same_statement = identities(
        &[&old],
        &[],
        &[extracted("Heat it.", old.statement())],
        field,
    );
    assert_eq!(same_statement.assigned, vec![Some(ClaimId("c1".into()))]);

    // The extractor quoted part of the sentence: the spans overlap.
    let part = identities(
        &[&old],
        &[],
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
        &[],
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

    // A prior quote that now occurs twice falls back to its stored span.
    let doubled = format!("{QUOTE} {QUOTE}");
    let ambiguous = identities(
        &[&old],
        &[],
        &[extracted("boils at 100 °C", "x")],
        |_, _| Some(doubled.as_str()),
    );
    assert_eq!(ambiguous.assigned, vec![Some(ClaimId("c1".into()))]);
    assert!(ambiguous.orphaned.is_empty());
}

#[test]
fn a_narrowed_claim_keeps_its_id_only_by_its_statement_or_quote() {
    let on_sentence = |id: &str, statement: &str| {
        ClaimLedgerEntry::extracted(
            ClaimId(id.into()),
            machine(1),
            location(QUOTE),
            statement.into(),
        )
        .unwrap()
    };
    let x = on_sentence("x", "Water boils at sea level.");
    let y = on_sentence("y", "Water boils at 100 degrees.");
    let narrowed = [ClaimId("x".into())];
    let field = |_: usize, _: &str| Some(FIELD_TEXT);
    let part = [extracted("boils at 100 °C", "It is hot.")];

    // Another statement on the sentence takes the id the narrowed claim gave up.
    let result = identities(&[&x, &y], &narrowed, &part, field);
    assert_eq!(result.assigned, vec![Some(ClaimId("y".into()))]);
    assert_eq!(result.orphaned, vec![ClaimId("x".into())]);

    // Control: with nothing narrowed, ledger order gives it to x.
    let result = identities(&[&x, &y], &[], &part, field);
    assert_eq!(result.assigned, vec![Some(ClaimId("x".into()))]);
    assert_eq!(result.orphaned, vec![ClaimId("y".into())]);

    // Its own statement word for word keeps it.
    let same_statement = [extracted("Heat it.", x.statement())];
    let result = identities(&[&x, &y], &narrowed, &same_statement, field);
    assert_eq!(result.assigned, vec![Some(ClaimId("x".into()))]);

    // Its own quote keeps it.
    let same_quote = [extracted(QUOTE, "It is hot.")];
    let result = identities(&[&x, &y], &narrowed, &same_quote, field);
    assert_eq!(result.assigned, vec![Some(ClaimId("x".into()))]);
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
fn a_verified_claim_is_checked_again_after_its_citation_is_repaired_and_only_then() {
    let mut entry = verified_claim("c1");
    let mut elsewhere = observation(&entry);
    elsewhere.fingerprint = local_check(None, "v2").fingerprint;
    assert_eq!(
        new_information(&entry, &elsewhere),
        None,
        "reselected evidence"
    );

    entry
        .append(
            machine(3),
            ClaimEvent::CitationRepaired {
                cited: vec!["v1".into()],
                source_version_id: "v2".into(),
                quote: "At sea level, water boils at 100 °C.".into(),
                added_source: false,
            },
        )
        .unwrap();
    assert_eq!(
        new_information(&entry, &elsewhere),
        Some(NewInformation::EvidenceChanged),
        "the repaired citation is what the next check reads"
    );

    entry.append(machine(4), checked(None, "v2")).unwrap();
    let mut later = observation(&entry);
    later.fingerprint = local_check(None, "v3").fingerprint;
    assert_eq!(
        new_information(&entry, &later),
        None,
        "the check decided the repair; the claim is verified again"
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
    // A statement the knowledge judge passed is the machine's, and resolves.
    let mut common = claim("c4");
    common
        .append(
            machine(2),
            ClaimEvent::Checked(Check {
                result: CheckResult::KnownCommon,
                evidence: vec![],
                ..local_check(None, "v1")
            }),
        )
        .unwrap();
    assert_eq!(
        end_state([&verified, &confirmed, &removed, &common]),
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
        ClaimCounts::tally([&verified, &confirmed, &removed, &common, &unchecked, &reopened]),
        ClaimCounts {
            verified: 2,
            confirmed: 1,
            queued: 1,
            open: 1,
            unverified: 0,
            removed_by_you: 1,
            edited_by_you: 0,
            rewritten: 0,
        }
    );
}

#[test]
fn a_lesson_whose_failed_claims_the_machine_cut_is_ready() {
    let research = || ClaimEvent::Researched {
        queries: vec!["boiling point sea level".into()],
        sources_added: vec!["v2".into()],
        passages_found: 1,
        round_ms_per_claim: None,
    };
    // A statement the knowledge judge calls false is cut at once.
    let mut false_at_once = claim("c1");
    false_at_once
        .append(machine(2), checked(Some(VerdictKind::Contradicted), "v1"))
        .unwrap();
    // A sourced statement is judged, researched and rechecked, rewritten once
    // and rechecked, then cut (claim classes design §3.2).
    let mut after_a_rewrite = claim("c2");
    for (at, event) in [
        (2, checked(Some(VerdictKind::Unsupported), "v1")),
        (3, research()),
        (4, checked(Some(VerdictKind::Unsupported), "v2")),
        (5, rewrite(1, false, RepairOutcome::Rewritten)),
        (6, checked(Some(VerdictKind::Unsupported), "v3")),
    ] {
        after_a_rewrite.append(machine(at), event).unwrap();
    }
    let verified = verified_claim("c3");
    assert_eq!(
        end_state([&verified, &false_at_once, &after_a_rewrite]),
        LessonReviewState::Verifying,
        "a failed claim is the machine's until it cuts it"
    );

    false_at_once
        .append(machine(3), rewrite(1, false, RepairOutcome::Removed))
        .unwrap();
    after_a_rewrite
        .append(machine(7), rewrite(2, true, RepairOutcome::Removed))
        .unwrap();
    for cut in [&false_at_once, &after_a_rewrite] {
        assert!(
            matches!(
                cut.state(),
                ClaimState::Removed {
                    by: Actor::Machine,
                    ..
                }
            ),
            "{:?}",
            cut.state()
        );
    }
    let claims = [&verified, &false_at_once, &after_a_rewrite];
    assert_eq!(end_state(claims), LessonReviewState::Ready);
    let counts = ClaimCounts::tally(claims);
    assert_eq!((counts.verified, counts.rewritten), (1, 2));
    assert_eq!(counts.queued + counts.open, 0);
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
        classified(Class::Reasoning, ClassifiedBy::Model),
        classified(Class::Common, ClassifiedBy::Evidence),
        ClaimEvent::CitationDropped {
            cited: vec!["v1".into()],
        },
        ClaimEvent::RepairAttempted {
            attempt: 1,
            escalated: false,
            before: QUOTE.into(),
            after: None,
            outcome: RepairOutcome::Rewritten,
            goal: Some(Goal::AsTheFieldHasIt),
        },
        ClaimEvent::Stepped {
            policy: "acceptance-v1".into(),
            step: Step::Cut,
            outcome: acceptance::Outcome::CannotCut,
            then: Then::End(acceptance::End::Removed { section_goes: true }),
            reason: "The sentence cannot be cut.".into(),
            rewrite_ahead: false,
        },
        ClaimEvent::Checked(Check {
            result: CheckResult::Consistent,
            ..local_check(None, "v1")
        }),
        ClaimEvent::Checked(Check {
            result: CheckResult::KnownCommon,
            evidence: vec![],
            ..local_check(None, "v1")
        }),
        ClaimEvent::Checked(Check {
            result: CheckResult::NotFalse,
            evidence: vec![],
            ..local_check(None, "v1")
        }),
        ClaimEvent::Stopped {
            reason: StopReason {
                code: StopCode::Retired("model_rejected".into()),
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
        ClaimState::Consistent { at: 5 },
        ClaimState::GeneralKnowledge { at: 6 },
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
    assert_eq!(
        serde_json::to_value(ClaimState::Consistent { at: 5 }).unwrap(),
        json!({"state": "consistent", "at": 5})
    );
    assert_eq!(
        serde_json::to_value(CheckResult::KnownCommon).unwrap(),
        json!("known_common")
    );
    assert_eq!(
        serde_json::to_value(CheckResult::NotFalse).unwrap(),
        json!("not_false")
    );
    for (by, name) in [
        (ClassifiedBy::Model, "model"),
        (ClassifiedBy::Rule, "rule"),
        (ClassifiedBy::Writer, "writer"),
        (ClassifiedBy::Evidence, "evidence"),
    ] {
        assert_eq!(
            serde_json::to_value(classified(Class::Sourced, by)).unwrap(),
            json!({"kind": "classified", "class": "sourced", "by": name})
        );
    }
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

#[test]
fn a_stop_code_reads_and_writes_the_string_it_was_saved_as() {
    for code in StopCode::KNOWN {
        let stored = serde_json::to_value(&code).unwrap();
        assert_eq!(stored, json!(code.as_str()));
        assert_eq!(serde_json::from_value::<StopCode>(stored).unwrap(), code);
    }
    let retired: StopCode = serde_json::from_value(json!("model_rejected")).unwrap();
    assert_eq!(retired, StopCode::Retired("model_rejected".into()));
    assert_eq!(
        serde_json::to_value(&retired).unwrap(),
        json!("model_rejected")
    );
}
