//! The check a drill's key passes: each fault where it arises and nowhere
//! else, and what holds for drills made by planting generated edits.

use proptest::prelude::*;

use super::*;

const CLEAN_CODE: &str =
    "def total(items):\n    return sum(items)\n\n\ndef label(name):\n    return name.strip()\n";

/// The clean code with two defects planted: `sum(items)` became
/// `sum(items[1:])` on line 2, and `name.strip()` lost its call on line 6.
const DRILL_CODE: &str =
    "def total(items):\n    return sum(items[1:])\n\n\ndef label(name):\n    return name.strip\n";

fn clean() -> Vec<DrillFile> {
    vec![file("main.py", CLEAN_CODE)]
}

fn key_issue(id: &str, pattern: &str, origin: Origin, span: LineSpan, fix: Edit) -> KeyIssue {
    KeyIssue {
        id: id.to_owned(),
        pattern: pattern.to_owned(),
        severity: Severity::Major,
        span,
        consequence: "It goes wrong.".to_owned(),
        fix,
        explanation: "Say it.".to_owned(),
        proof: Proof::Judged {
            reason: "The edit introduces it.".to_owned(),
        },
        origin,
    }
}

fn planted_issue(id: &str, span: LineSpan, fix: Edit) -> KeyIssue {
    key_issue(id, "correctness.off_by_one", Origin::Planted, span, fix)
}

fn drill_of(files: Vec<DrillFile>, key: Vec<KeyIssue>) -> Drill {
    Drill {
        id: "drill".to_owned(),
        material: Material::code("python"),
        title: "A report".to_owned(),
        files,
        key,
    }
}

fn drill(key: Vec<KeyIssue>) -> Drill {
    drill_of(vec![file("main.py", DRILL_CODE)], key)
}

/// Planted on line 2.
fn planted_slice() -> KeyIssue {
    planted_issue(
        "slice",
        span("main.py", 2, 2),
        edit("main.py", "sum(items[1:])", "sum(items)"),
    )
}

/// Planted on line 6.
fn planted_call() -> KeyIssue {
    key_issue(
        "call",
        "correctness.wrong_operator",
        Origin::Planted,
        span("main.py", 6, 6),
        edit("main.py", "name.strip", "name.strip()"),
    )
}

/// Found in review, on a line nothing was planted on.
fn review_signature() -> KeyIssue {
    key_issue(
        "signature",
        "correctness.boundary_and_empty_input",
        Origin::FoundInReview,
        span("main.py", 1, 1),
        edit("main.py", "def total(items):", "def total(items=()):"),
    )
}

/// Found in review, on the line the call was planted on.
fn review_return() -> KeyIssue {
    key_issue(
        "return",
        "correctness.null_dereference",
        Origin::FoundInReview,
        span("main.py", 6, 6),
        edit("main.py", "return name.strip", "return (name or '').strip"),
    )
}

fn valid() -> Drill {
    drill(vec![
        planted_slice(),
        planted_call(),
        review_signature(),
        review_return(),
    ])
}

fn refused(drill: &Drill) -> KeyFault {
    check_key(&clean(), drill).unwrap_err()
}

#[test]
fn a_drill_with_planted_and_found_in_review_issues_passes() {
    assert_eq!(check_key(&clean(), &valid()), Ok(()));
}

#[test]
fn a_found_in_review_issue_may_overlap_a_planted_span() {
    let drill = valid();
    assert!(overlaps(&drill.key[1].span, &drill.key[3].span));
    assert_eq!(check_key(&clean(), &drill), Ok(()));
}

#[test]
fn a_found_in_review_issues_fix_is_not_part_of_the_restore() {
    let mut drill = valid();
    drill.key[2].fix.after = "anything at all".to_owned();
    drill.key[3].fix.after = String::new();
    assert_eq!(check_key(&clean(), &drill), Ok(()));
}

#[test]
fn what_the_check_does_not_read_can_be_anything() {
    let mut drill = valid();
    drill.id = String::new();
    drill.title = String::new();
    for issue in &mut drill.key {
        issue.severity = Severity::Nit;
        issue.consequence = String::new();
        issue.explanation = String::new();
        issue.proof = Proof::Killed {
            test: "test_total".to_owned(),
        };
    }
    assert_eq!(check_key(&clean(), &drill), Ok(()));
}

#[test]
fn an_empty_key_is_refused() {
    assert_eq!(refused(&drill(vec![])), KeyFault::EmptyKey);
}

#[test]
fn a_path_listed_twice_in_the_clean_files_is_refused() {
    let twice = vec![file("main.py", CLEAN_CODE), file("main.py", CLEAN_CODE)];
    assert_eq!(
        check_key(&twice, &valid()),
        Err(KeyFault::DuplicateCleanPath {
            path: "main.py".to_owned()
        })
    );
}

#[test]
fn a_path_listed_twice_in_the_drills_files_is_refused() {
    let mut drill = valid();
    drill.files.push(file("main.py", DRILL_CODE));
    assert_eq!(
        refused(&drill),
        KeyFault::DuplicateDrillPath {
            path: "main.py".to_owned()
        }
    );
}

#[test]
fn files_with_other_paths_or_in_another_order_are_refused() {
    let cases = [
        (
            "another order",
            vec![file("a.py", "x\n"), file("b.py", "y\n")],
            vec![file("b.py", "y\n"), file("a.py", "x\n")],
        ),
        (
            "a file too many",
            vec![file("a.py", "x\n")],
            vec![file("a.py", "x\n"), file("b.py", "y\n")],
        ),
        (
            "a file too few",
            vec![file("a.py", "x\n"), file("b.py", "y\n")],
            vec![file("a.py", "x\n")],
        ),
        (
            "another path",
            vec![file("main.py", CLEAN_CODE)],
            vec![file("other.py", DRILL_CODE)],
        ),
    ];
    for (name, clean_files, drill_files) in cases {
        let mut drill = valid();
        drill.files = drill_files;
        assert_eq!(
            check_key(&clean_files, &drill),
            Err(KeyFault::PathsDiffer),
            "{name}"
        );
    }
}

#[test]
fn two_issues_with_one_id_are_refused() {
    let mut drill = valid();
    drill.key[1].id = "slice".to_owned();
    assert_eq!(
        refused(&drill),
        KeyFault::DuplicateIssueId {
            id: "slice".to_owned()
        }
    );
    let mut later = valid();
    later.key[3].id = "signature".to_owned();
    assert_eq!(
        refused(&later),
        KeyFault::DuplicateIssueId {
            id: "signature".to_owned()
        }
    );
}

#[test]
fn an_issue_whose_pattern_is_not_in_the_catalogue_is_refused() {
    for unknown in ["nothing.here", "Correctness.off_by_one", ""] {
        let mut drill = valid();
        drill.key[1].pattern = unknown.to_owned();
        assert_eq!(
            refused(&drill),
            KeyFault::UnknownPattern { issue: 1 },
            "{unknown:?}"
        );
    }
}

#[test]
fn an_issue_whose_pattern_does_not_apply_to_the_material_is_refused() {
    let mut drill = valid();
    drill.key[2].pattern = "errors.unwrap_on_fallible".to_owned();
    assert_eq!(
        refused(&drill),
        KeyFault::PatternNotForMaterial { issue: 2 }
    );
    let mut rust = valid();
    rust.material = Material::code("rust");
    assert_eq!(refused(&rust), KeyFault::PatternNotForMaterial { issue: 3 });
}

#[test]
fn a_code_pattern_in_a_passage_drill_and_a_passage_pattern_in_a_code_drill_are_refused() {
    let mut passage = valid();
    passage.material = Material::Passage;
    assert_eq!(
        refused(&passage),
        KeyFault::PatternNotForMaterial { issue: 0 }
    );

    let mut code = valid();
    code.key[1].pattern = "fidelity.misquoted_figure".to_owned();
    assert_eq!(refused(&code), KeyFault::PatternNotForMaterial { issue: 1 });
}

const CLEAN_NOTE: &str = "The trial enrolled 140 adults.\nThe drug cut relapses by 14 percent.\n";
const PLANTED_NOTE: &str = "The trial enrolled 140 adults.\nThe drug cut relapses by 40 percent.\n";

fn note_issue(pattern: &str, severity: Severity, proof: Proof) -> KeyIssue {
    KeyIssue {
        id: "figure".to_owned(),
        pattern: pattern.to_owned(),
        severity,
        span: span("explanation.md", 2, 2),
        consequence: "A reader would expect nearly half of relapses to be avoided.".to_owned(),
        fix: edit(
            "explanation.md",
            "The drug cut relapses by 40 percent.",
            "The drug cut relapses by 14 percent.",
        ),
        explanation: "The source gives 14 percent, not 40.".to_owned(),
        proof,
        origin: Origin::Planted,
    }
}

fn sourced(planted: SourceVerdict) -> Proof {
    Proof::Sourced {
        quote: "relapses fell by 14 percent".to_owned(),
        planted,
        reason: "The source states 14.".to_owned(),
    }
}

fn note(issue: KeyIssue) -> Drill {
    Drill {
        id: "note".to_owned(),
        material: Material::Passage,
        title: "A study note".to_owned(),
        files: vec![file("explanation.md", PLANTED_NOTE)],
        key: vec![issue],
    }
}

fn check_note(drill: &Drill) -> Result<(), KeyFault> {
    check_key(&[file("explanation.md", CLEAN_NOTE)], drill)
}

#[test]
fn a_passage_drill_with_a_sourced_issue_passes() {
    let drill = note(note_issue(
        "fidelity.misquoted_figure",
        Severity::Major,
        sourced(SourceVerdict::Contradicted),
    ));
    assert_eq!(check_note(&drill), Ok(()));
}

#[test]
fn a_contradicted_figure_entity_direction_or_definition_is_at_least_major() {
    for id in [
        "fidelity.misquoted_figure",
        "fidelity.swapped_entities",
        "fidelity.wrong_attribution",
        "reasoning.reversed_relation",
        "definition.wrong_definition",
        "definition.conflated_terms",
    ] {
        for severity in [Severity::Minor, Severity::Nit] {
            let drill = note(note_issue(
                id,
                severity,
                sourced(SourceVerdict::Contradicted),
            ));
            assert_eq!(
                check_note(&drill),
                Err(KeyFault::SeverityBelowFloor {
                    issue: 0,
                    floor: Severity::Major
                }),
                "{id} {severity:?}"
            );
        }
        for severity in [Severity::Major, Severity::Critical] {
            let drill = note(note_issue(
                id,
                severity,
                sourced(SourceVerdict::Contradicted),
            ));
            assert_eq!(check_note(&drill), Ok(()), "{id} {severity:?}");
        }
    }
}

#[test]
fn an_unsupported_issue_is_at_least_minor() {
    for id in [
        "scope.dropped_qualifier",
        "reasoning.unsupported_mechanism",
        "fidelity.misquoted_figure",
    ] {
        let nit = note(note_issue(
            id,
            Severity::Nit,
            sourced(SourceVerdict::Unsupported),
        ));
        assert_eq!(
            check_note(&nit),
            Err(KeyFault::SeverityBelowFloor {
                issue: 0,
                floor: Severity::Minor
            }),
            "{id}"
        );
        let minor = note(note_issue(
            id,
            Severity::Minor,
            sourced(SourceVerdict::Unsupported),
        ));
        assert_eq!(check_note(&minor), Ok(()), "{id}");
    }
}

#[test]
fn a_contradicted_scope_or_other_reasoning_issue_has_the_minor_floor() {
    for id in [
        "scope.overgeneralisation",
        "reasoning.correlation_as_cause",
        "reasoning.non_sequitur",
    ] {
        let nit = note(note_issue(
            id,
            Severity::Nit,
            sourced(SourceVerdict::Contradicted),
        ));
        assert_eq!(
            check_note(&nit),
            Err(KeyFault::SeverityBelowFloor {
                issue: 0,
                floor: Severity::Minor
            }),
            "{id}"
        );
        let minor = note(note_issue(
            id,
            Severity::Minor,
            sourced(SourceVerdict::Contradicted),
        ));
        assert_eq!(check_note(&minor), Ok(()), "{id}");
    }
}

#[test]
fn the_floor_is_a_function_of_the_pattern_and_the_proof() {
    let figure = pattern("fidelity.misquoted_figure").unwrap();
    let scope = pattern("scope.dropped_qualifier").unwrap();
    let off_by_one = pattern("correctness.off_by_one").unwrap();
    assert_eq!(
        severity_floor(figure, &sourced(SourceVerdict::Contradicted)),
        Some(Severity::Major)
    );
    assert_eq!(
        severity_floor(figure, &sourced(SourceVerdict::Unsupported)),
        Some(Severity::Minor)
    );
    assert_eq!(
        severity_floor(scope, &sourced(SourceVerdict::Contradicted)),
        Some(Severity::Minor)
    );
    let judged = Proof::Judged {
        reason: "It does.".to_owned(),
    };
    assert_eq!(severity_floor(figure, &judged), None);
    assert_eq!(severity_floor(off_by_one, &judged), None);
}

#[test]
fn a_consistency_issue_proven_by_a_judge_has_no_floor() {
    let drill = note(note_issue(
        "consistency.self_contradiction",
        Severity::Nit,
        Proof::Judged {
            reason: "The two sentences disagree.".to_owned(),
        },
    ));
    assert_eq!(check_note(&drill), Ok(()));
}

#[test]
fn an_issue_whose_fix_does_not_apply_alone_is_refused_with_the_reason() {
    let cases = [
        (
            edit("main.py", "no such text", "x"),
            EditFault::NotFound { edit: 0 },
        ),
        (
            edit("main.py", "return", "x"),
            EditFault::Ambiguous { edit: 0, count: 2 },
        ),
        (edit("main.py", "", "return"), EditFault::Empty { edit: 0 }),
        (
            edit("other.py", "return", "x"),
            EditFault::UnknownFile { edit: 0 },
        ),
    ];
    for (fix, fault) in cases {
        let mut drill = valid();
        drill.key[0].fix = fix;
        assert_eq!(
            refused(&drill),
            KeyFault::FixDoesNotApply { issue: 0, fault }
        );
    }
}

#[test]
fn an_issue_whose_span_is_not_the_lines_of_its_fix_is_refused() {
    let cases = [
        ("a line too early", span("main.py", 1, 1)),
        ("a line too late", span("main.py", 3, 3)),
        ("a span too wide", span("main.py", 2, 3)),
        ("a span too narrow", span("main.py", 2, 1)),
        ("another file", span("other.py", 2, 2)),
    ];
    for (name, wrong) in cases {
        let mut drill = valid();
        drill.key[0].span = wrong;
        assert_eq!(
            refused(&drill),
            KeyFault::SpanMismatch {
                issue: 0,
                expected: span("main.py", 2, 2)
            },
            "{name}"
        );
    }
}

#[test]
fn a_span_over_several_lines_is_the_lines_of_the_fix() {
    let clean_files = vec![file("main.py", "a\nb\nc\nd\n")];
    let drill = drill_of(
        vec![file("main.py", "a\nB\nC\nd\n")],
        vec![planted_issue(
            "two-lines",
            span("main.py", 2, 3),
            edit("main.py", "B\nC", "b\nc"),
        )],
    );
    assert_eq!(check_key(&clean_files, &drill), Ok(()));
}

#[test]
fn fixes_that_overlap_cannot_be_replaced_together_and_are_named_by_position_in_the_key() {
    let clean_files = vec![file("main.py", "abcdef\n")];
    let drill = drill_of(
        clean_files.clone(),
        vec![
            key_issue(
                "f",
                "correctness.off_by_one",
                Origin::FoundInReview,
                span("main.py", 1, 1),
                edit("main.py", "f", "g"),
            ),
            planted_issue("abc", span("main.py", 1, 1), edit("main.py", "abc", "X")),
            planted_issue("cde", span("main.py", 1, 1), edit("main.py", "cde", "Y")),
        ],
    );
    assert_eq!(
        check_key(&clean_files, &drill),
        Err(KeyFault::FixesDoNotApplyTogether {
            fault: EditFault::Overlaps {
                first: 1,
                second: 2
            }
        })
    );
}

#[test]
fn fixes_that_restore_other_code_than_the_clean_code_are_refused() {
    let mut drill = valid();
    drill.key[1].fix.after = "name.strip(".to_owned();
    assert_eq!(
        refused(&drill),
        KeyFault::RestoreDiffers {
            path: "main.py".to_owned()
        }
    );
}

#[test]
fn a_change_the_key_does_not_account_for_is_refused() {
    let mut unexplained = valid();
    unexplained.files = vec![file(
        "main.py",
        &DRILL_CODE.replace("def label(name):", "def label(nam):"),
    )];
    assert_eq!(
        refused(&unexplained),
        KeyFault::RestoreDiffers {
            path: "main.py".to_owned()
        }
    );
    let mut left_out = valid();
    left_out.key.remove(1);
    assert_eq!(
        refused(&left_out),
        KeyFault::RestoreDiffers {
            path: "main.py".to_owned()
        }
    );
}

#[test]
fn the_restore_names_the_first_file_that_differs() {
    let clean_files = vec![file("a.py", "x = 1\n"), file("b.py", "y = 1\n")];
    let drill = drill_of(
        vec![file("a.py", "x = 2\n"), file("b.py", "y = 2\n")],
        vec![planted_issue(
            "x",
            span("a.py", 1, 1),
            edit("a.py", "x = 2", "x = 1"),
        )],
    );
    assert_eq!(
        check_key(&clean_files, &drill),
        Err(KeyFault::RestoreDiffers {
            path: "b.py".to_owned()
        })
    );
}

#[test]
fn a_key_of_only_found_in_review_issues_needs_the_drill_to_be_the_clean_code() {
    let in_clean_code = drill_of(clean(), vec![review_signature()]);
    assert_eq!(check_key(&clean(), &in_clean_code), Ok(()));
    let in_other_code = drill(vec![review_signature()]);
    assert_eq!(
        refused(&in_other_code),
        KeyFault::RestoreDiffers {
            path: "main.py".to_owned()
        }
    );
}

#[test]
fn a_found_in_review_issue_gets_the_checks_on_the_issue_alone() {
    let mut span_off = valid();
    span_off.key[2].span = span("main.py", 3, 3);
    assert_eq!(
        refused(&span_off),
        KeyFault::SpanMismatch {
            issue: 2,
            expected: span("main.py", 1, 1)
        }
    );
    let mut pattern_off = valid();
    pattern_off.key[2].pattern = "nothing.here".to_owned();
    assert_eq!(refused(&pattern_off), KeyFault::UnknownPattern { issue: 2 });
    let mut fix_off = valid();
    fix_off.key[2].fix.before = "no such text".to_owned();
    assert_eq!(
        refused(&fix_off),
        KeyFault::FixDoesNotApply {
            issue: 2,
            fault: EditFault::NotFound { edit: 0 }
        }
    );
}

#[test]
fn the_first_fault_is_reported_in_the_documented_order() {
    let twice = vec![file("main.py", CLEAN_CODE), file("main.py", CLEAN_CODE)];
    assert_eq!(check_key(&twice, &drill(vec![])), Err(KeyFault::EmptyKey));

    let mut repeated_id = valid();
    repeated_id.key[1].id = "slice".to_owned();
    assert_eq!(
        check_key(&twice, &repeated_id),
        Err(KeyFault::DuplicateCleanPath {
            path: "main.py".to_owned()
        })
    );

    let mut id_and_pattern = repeated_id.clone();
    id_and_pattern.key[2].pattern = "nothing.here".to_owned();
    assert_eq!(
        refused(&id_and_pattern),
        KeyFault::DuplicateIssueId {
            id: "slice".to_owned()
        }
    );

    let mut pattern_and_fix = valid();
    pattern_and_fix.key[0].fix.before = "no such text".to_owned();
    pattern_and_fix.key[3].pattern = "nothing.here".to_owned();
    assert_eq!(
        refused(&pattern_and_fix),
        KeyFault::UnknownPattern { issue: 3 }
    );

    let mut fix_and_span = valid();
    fix_and_span.key[0].span = span("main.py", 1, 1);
    fix_and_span.key[3].fix.before = "no such text".to_owned();
    assert_eq!(
        refused(&fix_and_span),
        KeyFault::FixDoesNotApply {
            issue: 3,
            fault: EditFault::NotFound { edit: 0 }
        }
    );

    let mut span_and_restore = valid();
    span_and_restore.key[2].span = span("main.py", 2, 2);
    span_and_restore.key[1].fix.after = "x".to_owned();
    assert_eq!(
        refused(&span_and_restore),
        KeyFault::SpanMismatch {
            issue: 2,
            expected: span("main.py", 1, 1)
        }
    );
}

#[test]
fn a_plant_that_split_one_clean_line_in_two_passes() {
    let clean_files = vec![file("main.py", "a b\n")];
    let plants = [edit("main.py", "a", "x\n"), edit("main.py", "b", "y")];
    let (files, spans) = apply(&clean_files, &plants).unwrap();
    assert_eq!(files[0].text, "x\n y\n");
    let key: Vec<KeyIssue> = plants
        .iter()
        .zip(spans)
        .enumerate()
        .map(|(position, (plant, span))| {
            planted_issue(
                &format!("split-{position}"),
                span,
                edit(&plant.path, &plant.after, &plant.before),
            )
        })
        .collect();
    let drill = drill_of(files, key);
    let fixes: Vec<Edit> = drill.key.iter().map(|issue| issue.fix.clone()).collect();
    assert_eq!(
        apply(&drill.files, &fixes).unwrap_err(),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
    assert_eq!(check_key(&clean_files, &drill), Ok(()));
}

#[test]
fn a_pure_deletion_is_refused_because_its_reverse_has_an_empty_before() {
    let clean_files = vec![file("main.py", "a\nb\nc\n")];
    let (files, spans) = apply(&clean_files, &[edit("main.py", "b\n", "")]).unwrap();
    assert_eq!(files[0].text, "a\nc\n");
    let drill = drill_of(
        files,
        vec![planted_issue(
            "deleted",
            spans[0].clone(),
            edit("main.py", "", "b\n"),
        )],
    );
    assert_eq!(
        check_key(&clean_files, &drill),
        Err(KeyFault::FixDoesNotApply {
            issue: 0,
            fault: EditFault::Empty { edit: 0 }
        })
    );
}

#[test]
fn a_deletion_that_keeps_a_neighbouring_line_passes() {
    let clean_files = vec![file("main.py", "a\nb\nc\n")];
    let (files, spans) = apply(&clean_files, &[edit("main.py", "b\nc", "c")]).unwrap();
    assert_eq!(files[0].text, "a\nc\n");
    let drill = drill_of(
        files,
        vec![planted_issue(
            "deleted",
            spans[0].clone(),
            edit("main.py", "c", "b\nc"),
        )],
    );
    assert_eq!(check_key(&clean_files, &drill), Ok(()));
}

/// A drill made by planting the generated edits into one clean file: each edit
/// a planted issue, its fix the reverse edit, its span where the plant landed.
fn planted(generated: &[Row], final_break: bool) -> (Vec<DrillFile>, Drill) {
    let (original, edits) = build(generated, final_break, marked);
    let clean_files = vec![original];
    let (files, spans) = apply(&clean_files, &edits).unwrap();
    let key = edits
        .iter()
        .zip(spans)
        .enumerate()
        .map(|(position, (plant, span))| {
            planted_issue(
                &format!("issue-{position}"),
                span,
                edit(&plant.path, &plant.after, &plant.before),
            )
        })
        .collect();
    (clean_files, drill_of(files, key))
}

proptest! {
    #[test]
    fn a_drill_made_by_planting_edits_passes(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
    ) {
        let (clean_files, drill) = planted(&generated, final_break);
        let expected = if drill.key.is_empty() {
            Err(KeyFault::EmptyKey)
        } else {
            Ok(())
        };
        prop_assert_eq!(check_key(&clean_files, &drill), expected);
    }

    #[test]
    fn a_shifted_span_is_a_span_mismatch(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
        pick in any::<proptest::sample::Index>(),
    ) {
        let (clean_files, mut drill) = planted(&generated, final_break);
        prop_assume!(!drill.key.is_empty());
        let position = pick.index(drill.key.len());
        let expected = drill.key[position].span.clone();
        drill.key[position].span.start += 1;
        drill.key[position].span.end += 1;
        prop_assert_eq!(
            check_key(&clean_files, &drill),
            Err(KeyFault::SpanMismatch { issue: position, expected })
        );
    }

    #[test]
    fn a_changed_byte_of_a_fixs_before_is_a_fix_that_does_not_apply(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
        pick in any::<proptest::sample::Index>(),
    ) {
        let (clean_files, mut drill) = planted(&generated, final_break);
        prop_assume!(!drill.key.is_empty());
        let position = pick.index(drill.key.len());
        drill.key[position].fix.before.replace_range(0..1, "[");
        prop_assert_eq!(
            check_key(&clean_files, &drill),
            Err(KeyFault::FixDoesNotApply {
                issue: position,
                fault: EditFault::NotFound { edit: 0 },
            })
        );
    }

    #[test]
    fn a_changed_byte_of_a_fixs_after_is_a_restore_that_differs(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
        pick in any::<proptest::sample::Index>(),
    ) {
        let (clean_files, mut drill) = planted(&generated, final_break);
        prop_assume!(!drill.key.is_empty());
        let position = pick.index(drill.key.len());
        drill.key[position].fix.after.replace_range(0..1, "M");
        prop_assert_eq!(
            check_key(&clean_files, &drill),
            Err(KeyFault::RestoreDiffers { path: "main.py".to_owned() })
        );
    }

    #[test]
    fn a_planted_change_the_key_leaves_out_is_a_restore_that_differs(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
        pick in any::<proptest::sample::Index>(),
    ) {
        let (clean_files, mut drill) = planted(&generated, final_break);
        prop_assume!(!drill.key.is_empty());
        drill.key.remove(pick.index(drill.key.len()));
        let expected = if drill.key.is_empty() {
            KeyFault::EmptyKey
        } else {
            KeyFault::RestoreDiffers { path: "main.py".to_owned() }
        };
        prop_assert_eq!(check_key(&clean_files, &drill), Err(expected));
    }
}
