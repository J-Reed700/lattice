#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::bridge::ClaimParts;
use super::classes_tests::{checker, conversation, messages, Reply, Scripted};
use super::*;
use crate::application::services::model_contract::test_support::assert_round_trip;
use crate::application::services::model_contract::CORRECTION_LEAD;
use crate::domain::learning::claim_review::text_sha256;
use serde_json::{json, Value};

/// The part-wise judge's instructions as the replay's script holds them.
fn recorded_prompt() -> String {
    let script = include_str!("../../../../../../evals/grounding/claim-classes/replay_classes.py")
        .replace("\r\n", "\n");
    let opening = "BRIDGE_SYSTEM = \"\"\"";
    let start = script.find(opening).unwrap() + opening.len();
    let end = start + script[start..].find("\"\"\"").unwrap();
    script[start..end].to_owned()
}

fn passages() -> Vec<String> {
    vec!["Quillet is a lexer shipped with the toolchain.".into()]
}

fn reply(parts: Value, verdict: &str) -> Reply {
    Reply::Text(
        json!({"parts": parts, "verdict": verdict, "reason": "The passage states the first part."})
            .to_string(),
    )
}

fn stated(part: &str) -> Value {
    json!({"part": part, "kind": "stated", "passage": "passage-0"})
}

fn unplaced(part: &str, kind: &str) -> Value {
    json!({"part": part, "kind": kind, "passage": null})
}

fn judged(answered: Answered<ClassJudgment<BridgeOutcome>>) -> BridgeOutcome {
    match answered.judgments {
        ClassJudgment::Judged(outcome) => outcome,
        other => unreachable!("{other:?}"),
    }
}

fn part(text: &str, kind: PartKind) -> BridgePart {
    BridgePart {
        part: text.into(),
        kind,
        passage: (kind == PartKind::Stated).then_some(0),
    }
}

fn outcome(parts: Vec<BridgePart>) -> BridgeOutcome {
    BridgeOutcome {
        parts,
        reason: "Because.".into(),
    }
}

/// A stand-in for the application's rule layer: a figure is a specific.
fn names_a_figure(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
}

#[test]
fn the_prompt_is_the_prompt_of_record_by_its_text_and_by_its_digest() {
    assert_eq!(BRIDGE_SYSTEM, recorded_prompt());
    assert_eq!(
        text_sha256(BRIDGE_SYSTEM),
        "c8b758da6f11a83720add10eb0b4641855139bb84f2cee840103b676e6ce31fa"
    );
}

#[tokio::test]
async fn a_part_wise_check_sends_the_prompt_of_record_and_the_passages_under_their_ids() {
    let llm = Scripted::new(vec![reply(
        json!([stated("Quillet is a lexer.")]),
        "supported",
    )]);
    let outcome = judged(
        checker(&llm, CheckPolicy::Bridge)
            .check_bridge("Quillet is a lexer.", &passages())
            .await,
    );
    assert_eq!(
        outcome.parts,
        vec![BridgePart {
            part: "Quillet is a lexer.".into(),
            kind: PartKind::Stated,
            passage: Some(0)
        }]
    );
    assert_eq!(outcome.reason, "The passage states the first part.");
    let sent = llm.sent();
    assert_eq!(sent.len(), 1);
    let (system, user) = messages(&sent[0]);
    assert_eq!(system, BRIDGE_SYSTEM);
    assert_eq!(
        user,
        "Source passages:\n[passage-0]\nQuillet is a lexer shipped with the toolchain.\n\nClaim: Quillet is a lexer."
    );
    assert_eq!(sent[0].sampling, Some(SamplingOverride::deterministic()));
    let schema = sent[0].json_schema.as_ref().unwrap();
    assert_eq!(
        schema["properties"]["parts"]["items"]["properties"]["kind"]["enum"],
        json!([
            "stated",
            "common",
            "own",
            "specific_missing",
            "contradicted"
        ])
    );
}

#[test]
fn the_parts_decide_a_statement() {
    use Outcome::{AllCommon, Refuted, Stands, Unproven};
    use PartKind::{Common, SpecificMissing, Stated};
    let decision = |kinds: &[PartKind]| {
        outcome(kinds.iter().map(|kind| part("A part.", *kind)).collect()).outcome()
    };
    assert_eq!(decision(&[Stated, Stated]), Stands);
    assert_eq!(decision(&[Stated, Common]), Stands);
    assert_eq!(decision(&[Common, Stated, Common]), Stands);
    assert_eq!(decision(&[Stated, SpecificMissing]), Unproven);
    assert_eq!(decision(&[Stated, Common, SpecificMissing]), Unproven);
    // Nothing is stated: common knowledge alone is no source.
    assert_eq!(decision(&[Common, Common]), AllCommon);
    assert_eq!(decision(&[Stated, PartKind::Contradicted]), Refuted);
    assert_eq!(
        decision(&[SpecificMissing, PartKind::Contradicted]),
        Refuted
    );
}

#[test]
fn a_part_called_common_that_names_a_specific_counts_as_one_no_passage_gives() {
    let parts = vec![
        part("Quillet is a lexer.", PartKind::Stated),
        part("It shipped in 1999.", PartKind::Common),
        part("A lexer turns characters into tokens.", PartKind::Common),
    ];
    let (guarded, changed) = outcome(parts).guarded(names_a_figure);
    assert_eq!(changed, vec!["It shipped in 1999.".to_owned()]);
    assert_eq!(guarded.parts[1].kind, PartKind::SpecificMissing);
    assert_eq!(guarded.parts[2].kind, PartKind::Common);
    assert_eq!(guarded.outcome(), Outcome::Unproven);

    // A stated part is the passage's own word and is left alone.
    let (stated, changed) =
        outcome(vec![part("It shipped in 1999.", PartKind::Stated)]).guarded(names_a_figure);
    assert!(changed.is_empty());
    assert_eq!(stated.outcome(), Outcome::Stands);
}

/// The five planted rows of design 4.5, as the model answered them, through
/// the reading, the guard and the decision.
#[tokio::test]
async fn the_planted_rows_come_out_as_the_design_measured_them() {
    use Outcome::{Refuted as Contradicted, Stands as Supported, Unproven as Unsupported};
    let rows = [
        (
            "The study found that OCR-based RAG improved retrieval recall by 23% over the vision-based pipeline on degraded documents.",
            json!([unplaced("OCR-based RAG improved retrieval recall by 23% over the vision-based pipeline on degraded documents", "specific_missing")]),
            Unsupported,
        ),
        (
            "The survey reports that modular document parsing pipelines were introduced by Google in 2019.",
            json!([{"part": "modular document parsing pipelines were introduced by Google in 2019", "kind": "contradicted", "passage": "passage-0"}]),
            Contradicted,
        ),
        (
            "Vision-language models were first proposed in 2017 and reached human parity on document parsing in 2021.",
            json!([
                unplaced("first proposed in 2017", "specific_missing"),
                unplaced("reached human parity on document parsing in 2021", "specific_missing"),
            ]),
            Unsupported,
        ),
        (
            "Tesseract is an OCR engine written by Microsoft in 2019 that reads pixels into text.",
            json!([
                stated("Tesseract is an OCR engine"),
                {"part": "written by Microsoft", "kind": "contradicted", "passage": "passage-0"},
                {"part": "in 2019", "kind": "contradicted", "passage": "passage-0"},
                unplaced("reads pixels into text", "common"),
            ]),
            Contradicted,
        ),
        (
            "Tesseract is an OCR engine, and OCR turns the pixels of a page image into text.",
            json!([
                stated("Tesseract is an OCR engine"),
                unplaced("OCR turns the pixels of a page image into text", "common"),
            ]),
            Supported,
        ),
    ];
    for (statement, parts, expected) in rows {
        let llm = Scripted::new(vec![reply(parts, "unsupported")]);
        let read = judged(
            checker(&llm, CheckPolicy::Bridge)
                .check_bridge(statement, &passages())
                .await,
        );
        let (guarded, _) = read.guarded(names_a_figure);
        assert_eq!(guarded.outcome(), expected, "{statement}");
    }
}

#[tokio::test]
async fn the_verdict_the_model_writes_beside_the_parts_is_not_read() {
    let llm = Scripted::new(vec![reply(
        json!([
            stated("Quillet is a lexer"),
            unplaced("it was written in 2004", "specific_missing")
        ]),
        "supported",
    )]);
    let read = judged(
        checker(&llm, CheckPolicy::Bridge)
            .check_bridge("Quillet is a lexer written in 2004.", &passages())
            .await,
    );
    assert_eq!(read.outcome(), Outcome::Unproven);
}

#[tokio::test]
async fn a_reply_is_read_where_it_is_unambiguous() {
    let llm = Scripted::new(vec![Reply::Text(
        json!({
            "parts": [
                {"part": " Quillet is a lexer ", "kind": "Stated", "passage": "[passage-0]"},
                {"part": "A lexer turns characters into tokens", "kind": "common", "passage": "passage-9"},
            ],
            "verdict": "supported_with_common",
        })
        .to_string(),
    )]);
    let read = judged(
        checker(&llm, CheckPolicy::Bridge)
            .check_bridge(
                "Quillet is a lexer that turns characters into tokens.",
                &passages(),
            )
            .await,
    );
    assert_eq!(read.parts[0].part, "Quillet is a lexer");
    assert_eq!(read.parts[0].kind, PartKind::Stated);
    assert_eq!(read.parts[0].passage, Some(0));
    // A passage that was not shown is no place for a part that needs none.
    assert_eq!(read.parts[1].kind, PartKind::Common);
    assert_eq!(read.parts[1].passage, None);
    assert_eq!(read.reason, "");
}

#[tokio::test]
async fn a_reply_that_cannot_be_read_is_asked_for_once_more_and_then_left_unjudged() {
    let good = || reply(json!([stated("Quillet is a lexer.")]), "supported");
    let llm = Scripted::new(vec![Reply::Text("not json".into()), good()]);
    let answered = checker(&llm, CheckPolicy::Bridge)
        .check_bridge("Quillet is a lexer.", &passages())
        .await;
    assert!(matches!(answered.judgments, ClassJudgment::Judged(_)));
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    let again = conversation(&sent[1]);
    assert!(
        again.contains(CORRECTION_LEAD)
            && again.contains("not json")
            && again.contains("the claim's parts"),
        "{again}"
    );

    let bad = || {
        vec![
            Reply::Text("{}".into()),
            // No part.
            reply(json!([]), "supported"),
            // A kind that is not one.
            reply(json!([unplaced("A part.", "plausible")]), "supported"),
            // A part said to be stated, and not where.
            reply(json!([unplaced("A part.", "stated")]), "supported"),
            // A part stated by a passage that was never shown.
            reply(
                json!([{"part": "A part.", "kind": "stated", "passage": "passage-3"}]),
                "supported",
            ),
            // Nothing in it.
            reply(json!([unplaced("  ", "common")]), "supported"),
            Reply::Cut(
                json!({"parts": [stated("A part.")], "verdict": "supported", "reason": "Fine."})
                    .to_string(),
            ),
        ]
    };
    for first in bad() {
        let second = bad().pop().unwrap();
        let llm = Scripted::new(vec![first, second]);
        let judgment = checker(&llm, CheckPolicy::Bridge)
            .check_bridge("A part.", &passages())
            .await
            .judgments;
        assert!(matches!(judgment, ClassJudgment::Unusable), "{judgment:?}");
        assert_eq!(llm.sent().len(), 2);
    }
}

#[tokio::test]
async fn an_outage_stays_an_error_and_another_policy_makes_no_call() {
    let llm = Scripted::new(vec![Reply::Outage]);
    let answered = checker(&llm, CheckPolicy::Bridge)
        .check_bridge("A part.", &passages())
        .await;
    assert!(
        matches!(answered.judgments, ClassJudgment::Failed(_)),
        "{:?}",
        answered.judgments
    );
    assert_eq!(llm.sent().len(), 1);

    let llm = Scripted::new(vec![reply(json!([stated("A part.")]), "supported")]);
    let wrong = checker(&llm, CheckPolicy::Strict)
        .check_bridge("A part.", &passages())
        .await;
    assert!(matches!(wrong.judgments, ClassJudgment::Unusable));
    assert!(llm.sent().is_empty());
}

#[tokio::test]
async fn the_output_tokens_of_both_asks_are_counted() {
    let good = json!({"parts": [stated("A part.")], "verdict": "supported", "reason": "Fine."});
    let llm = Scripted::new(vec![
        Reply::Counted("not json".into(), 40),
        Reply::Counted(good.to_string(), 90),
    ]);
    let answered = checker(&llm, CheckPolicy::Bridge)
        .check_bridge("A part.", &passages())
        .await;
    assert!(matches!(answered.judgments, ClassJudgment::Judged(_)));
    assert_eq!(answered.output_tokens, 130);
}

#[test]
fn every_mix_of_parts_has_the_outcome_the_design_gives_it() {
    use PartKind::{Common, Contradicted, SpecificMissing, Stated};
    let rows: &[(&[PartKind], Outcome)] = &[
        (&[Stated], Outcome::Stands),
        (&[Common], Outcome::AllCommon),
        (&[SpecificMissing], Outcome::Unproven),
        (&[Contradicted], Outcome::Refuted),
        (&[Stated, Common], Outcome::Stands),
        (&[Stated, SpecificMissing], Outcome::Unproven),
        (&[Stated, Contradicted], Outcome::Refuted),
        (&[Common, SpecificMissing], Outcome::Unproven),
        (&[Common, Contradicted], Outcome::Refuted),
        (&[SpecificMissing, Contradicted], Outcome::Refuted),
        (&[Stated, Common, SpecificMissing], Outcome::Unproven),
        (&[Stated, Common, Contradicted], Outcome::Refuted),
        (&[Stated, SpecificMissing, Contradicted], Outcome::Refuted),
        (&[Common, SpecificMissing, Contradicted], Outcome::Refuted),
        (
            &[Stated, Common, SpecificMissing, Contradicted],
            Outcome::Refuted,
        ),
    ];
    for (kinds, expected) in rows {
        let parts: Vec<BridgePart> = kinds.iter().map(|kind| part("A part.", *kind)).collect();
        assert_eq!(outcome(parts).outcome(), *expected, "{kinds:?}");
    }
}

#[test]
fn a_cited_statement_whose_parts_are_all_common_is_all_common_whatever_the_old_rule_said() {
    let (guarded, changed) = outcome(vec![
        part("A lexer turns characters into tokens.", PartKind::Common),
        part("A token is a unit of text.", PartKind::Common),
    ])
    .guarded(names_a_figure);
    assert!(changed.is_empty());
    assert_eq!(guarded.outcome(), Outcome::AllCommon);
}

#[test]
fn a_common_part_that_names_a_specific_is_unproven_once_guarded() {
    let parts = vec![
        part("Quillet is a lexer.", PartKind::Stated),
        part("It shipped in 1999.", PartKind::Common),
    ];
    assert_eq!(outcome(parts.clone()).outcome(), Outcome::Stands);
    let (guarded, changed) = outcome(parts).guarded(names_a_figure);
    assert_eq!(changed, vec!["It shipped in 1999.".to_owned()]);
    assert_eq!(guarded.outcome(), Outcome::Unproven);

    let (alone, _) =
        outcome(vec![part("It has 12 rules.", PartKind::Common)]).guarded(names_a_figure);
    assert_eq!(alone.outcome(), Outcome::Unproven);
}

#[test]
fn only_what_the_passages_state_grounds_a_vetoed_statement() {
    use PartKind::{Common, Contradicted, Own, SpecificMissing, Stated};
    let rows: &[(&[PartKind], Outcome)] = &[
        (&[Stated], Outcome::Stands),
        (&[Stated, Own], Outcome::Stands),
        (&[Stated, Common], Outcome::Refuted),
        (&[Common], Outcome::Refuted),
        (&[Stated, SpecificMissing], Outcome::Refuted),
        (&[Stated, Contradicted], Outcome::Refuted),
        (&[], Outcome::Refuted),
    ];
    for (kinds, expected) in rows {
        let parts: Vec<BridgePart> = kinds.iter().map(|kind| part("A part.", *kind)).collect();
        assert_eq!(outcome(parts).ground_outcome(), *expected, "{kinds:?}");
    }
}

const OWN_FIGURES: &str = "The lesson's own figures are the quantities its own example, scenario or exercise sets, and the results calculated from them.";

#[test]
fn the_part_wise_judge_is_told_what_the_lesson_s_own_figures_are() {
    assert!(BRIDGE_SYSTEM.contains(OWN_FIGURES), "{BRIDGE_SYSTEM}");
}

/// The judgment a scripted model's reply with `parts` comes to, however
/// many times it is asked.
async fn judgment_of(parts: Value) -> Answered<ClassJudgment<BridgeOutcome>> {
    let llm = Scripted::new(vec![
        reply(parts.clone(), "supported_with_common"),
        reply(parts, "supported_with_common"),
    ]);
    checker(&llm, CheckPolicy::Bridge)
        .check_bridge("A rule set of 48 checks has 12 that fail.", &passages())
        .await
}

#[tokio::test]
async fn a_part_that_is_the_lesson_s_own_figure_stands_beside_a_stated_one() {
    let answered = judgment_of(json!([
        stated("Quillet is a lexer."),
        unplaced("12 of 48 is 25%", "own")
    ]))
    .await;
    assert!(
        matches!(answered.judgments, ClassJudgment::Judged(_)),
        "{:?}",
        answered.judgments
    );
    let read = judged(answered);
    let (guarded, changed) = read.guarded(names_a_figure);
    assert!(changed.is_empty());
    assert_eq!(guarded.outcome(), Outcome::Stands);
}

#[tokio::test]
async fn parts_that_are_all_common_or_the_lesson_s_own_are_all_common() {
    let answered = judgment_of(json!([
        unplaced("A lexer turns characters into tokens.", "common"),
        unplaced("12 of 48 is 25%", "own")
    ]))
    .await;
    assert!(
        matches!(answered.judgments, ClassJudgment::Judged(_)),
        "{:?}",
        answered.judgments
    );
    let (guarded, changed) = judged(answered).guarded(names_a_figure);
    assert!(changed.is_empty());
    assert_eq!(guarded.outcome(), Outcome::AllCommon);
}

#[tokio::test]
async fn the_passages_ground_a_statement_with_a_part_of_the_lesson_s_own() {
    let answered = judgment_of(json!([
        stated("Quillet is a lexer."),
        unplaced("12 of 48 is 25%", "own")
    ]))
    .await;
    assert!(
        matches!(answered.judgments, ClassJudgment::Judged(_)),
        "{:?}",
        answered.judgments
    );
    let (guarded, _) = judged(answered).guarded(names_a_figure);
    assert_eq!(guarded.ground_outcome(), Outcome::Stands);
}

/// The page of `words` words, as one passage.
fn page_of(words: usize) -> Vec<String> {
    vec!["word ".repeat(words)]
}

/// The most words of one passage that `fits` holds.
fn edge(fits: impl Fn(&[String]) -> bool) -> usize {
    let sizes: Vec<usize> = (0..40_000).collect();
    sizes
        .partition_point(|&words| fits(&page_of(words)))
        .saturating_sub(1)
}

#[test]
fn a_part_wise_check_fits_the_window_only_beside_its_answer() {
    let llm = Scripted::new(vec![]);
    let asked = |answer: u32| {
        ClaimChecker::new(
            &llm,
            SamplingOverride::deterministic(),
            answer,
            CheckPolicy::Bridge,
        )
    };
    let (short, long) = (asked(512), asked(600));
    assert!(short.bridge_fits("S.", &passages()));
    assert!(!short.bridge_fits("S.", &["word ".repeat(40_000)]));

    // The page that just fits beside a 512-token answer is refused beside the
    // 600 tokens the part-wise judge may write.
    let words = edge(|page| short.bridge_fits("S.", page));
    assert!(words > 0);
    assert!(short.bridge_fits("S.", &page_of(words)));
    assert!(!long.bridge_fits("S.", &page_of(words)));
}

#[test]
fn the_part_wise_check_holds_less_than_the_consistency_check_and_more_than_the_strict_one() {
    let llm = Scripted::new(vec![]);
    let part_wise = ClaimChecker::new(
        &llm,
        SamplingOverride::deterministic(),
        600,
        CheckPolicy::Bridge,
    );
    let consistency = checker(&llm, CheckPolicy::Consistency);
    let strict = checker(&llm, CheckPolicy::Strict);

    let beside_part_wise = edge(|page| part_wise.bridge_fits("S.", page));
    let beside_consistency = edge(|page| consistency.consistency_fits("S.", page));
    let beside_strict = edge(|page| strict.located_fits("S.", page));

    assert!(
        beside_strict < beside_part_wise && beside_part_wise < beside_consistency,
        "{beside_strict} < {beside_part_wise} < {beside_consistency}"
    );
}

#[tokio::test]
async fn a_reason_past_600_characters_is_kept_whole() {
    let reason = "The passage states the first part. "
        .repeat(20)
        .trim()
        .to_string();
    assert!(reason.chars().count() > 600);
    let llm = Scripted::new(vec![Reply::Text(
        json!({"parts": [stated("Quillet is a lexer.")], "verdict": "supported", "reason": reason})
            .to_string(),
    )]);
    let outcome = judged(
        checker(&llm, CheckPolicy::Bridge)
            .check_bridge("Quillet is a lexer.", &passages())
            .await,
    );
    assert_eq!(outcome.reason, reason);
}

#[test]
fn the_part_wise_contract_reads_what_it_renders() {
    assert_round_trip::<ClaimParts>(&outcome(vec![
        BridgePart {
            part: "Quillet is a lexer".into(),
            kind: PartKind::Stated,
            passage: Some(0),
        },
        part("It tokenizes.", PartKind::Common),
        part("It ships with the toolchain.", PartKind::Own),
        part("It lexes 4 languages.", PartKind::SpecificMissing),
        BridgePart {
            part: "It is a parser".into(),
            kind: PartKind::Contradicted,
            passage: Some(1),
        },
    ]));
}
