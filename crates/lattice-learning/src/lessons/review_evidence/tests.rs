#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::dto::LearningSourceDto;
use crate::lessons::course_generation_tests::{length_bounds, RequestLog};
use crate::lessons::structured_call::{is_draft_request, task_messages};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::shape::Pointers;
use lattice_core::application::services::model_contract::test_support::samples::schema_of;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use std::sync::atomic::{AtomicUsize, Ordering};

const KETTLE: &str = "The kettle boils when the water reaches its limit.";

const CRANE: &str = "The harbour crane lifts crate one before the tide turns.";

const KILN: &str = "The kiln fires every bowl once the damper is shut.";

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

/// Every message of the request, system and user, one after another.
fn prompt_of(sent: &Sent) -> String {
    sent.request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn task(sent: &Sent) -> Value {
    task_of(&prompt_of(sent)).unwrap()
}

fn instructions(sent: &Sent) -> String {
    task_messages(&sent.request).0.to_owned()
}

/// The asks the model was sent, a draft step left out.
fn asks(model: &ScriptedModel) -> Vec<Sent> {
    model
        .sent()
        .into_iter()
        .filter(|sent| !is_draft_request(&sent.request))
        .collect()
}

fn proposed(texts: &[&str]) -> Vec<ReviewFinding> {
    texts
        .iter()
        .enumerate()
        .map(|(at, text)| ReviewFinding::reviewer(u32::try_from(at + 1).unwrap(), None, *text))
        .collect()
}

fn proposal(text: &str) -> Vec<ReviewFinding> {
    proposed(&[text])
}

fn placed(findings: Vec<ReviewFinding>) -> Vec<(String, Option<FindingSite>)> {
    findings.into_iter().map(|f| (f.note, f.site)).collect()
}

fn source(id: &str, excerpt: &str) -> LearningSourceDto {
    LearningSourceDto {
        id: id.into(),
        title: format!("Reference {id}"),
        url: None,
        excerpt: excerpt.into(),
        acquired_at: 1,
    }
}

fn kettle() -> Vec<LearningSourceDto> {
    vec![source("source-0", KETTLE)]
}

fn requirements() -> Value {
    json!({"deliverable":"Record the resulting state."})
}

/// One section and one question: `Section 1` and `Question 1`.
fn lesson() -> Value {
    json!({"blocks":[{"title":"Part","body":"Read the instructions."}],"questions":[{"prompt":"Is the lid shut?"}]})
}

/// A candidate of `count` sections and one question.
fn sections(count: usize) -> Value {
    let blocks: Vec<Value> = (0..count)
        .map(|at| json!({"title":format!("Part {at}"),"body":"Read the instructions."}))
        .collect();
    json!({"blocks":blocks,"questions":[{"prompt":"Is the lid shut?"}]})
}

async fn run(
    model: &ScriptedModel,
    issues: &[ReviewFinding],
    beside: Beside<'_>,
    candidate: &Value,
    sources: &[LearningSourceDto],
) -> Result<Checked> {
    let references = ReferenceCollection::lexical(sources)?;
    check(
        model,
        issues,
        beside,
        &requirements(),
        candidate,
        &references,
        None,
    )
    .await
}

/// A model that answers each ask of the check with `answer(prompt, number of earlier asks)`, and a draft step with prose.
fn scripted(answer: impl Fn(&str, usize) -> String + Send + Sync + 'static) -> ScriptedModel {
    let asked = AtomicUsize::new(0);
    ScriptedModel::new().on_request::<ReviewEvidence>(move |sent, _| {
        if is_draft_request(&sent.request) {
            return reply("The findings are weighed.".to_owned());
        }
        let earlier = asked.fetch_add(1, Ordering::SeqCst);
        reply(answer(&prompt_of(sent), earlier))
    })
}

/// A model that answers every ask with what `decide` gives fields for.
fn answering(decide: impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static) -> ScriptedModel {
    scripted(move |prompt, _| scripted_entries(prompt, &decide))
}

/// A model that answers the first ask with `first` and every later one with `again`.
fn answering_twice(
    first: impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static,
    again: impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static,
) -> ScriptedModel {
    scripted(move |prompt, earlier| {
        if earlier == 0 {
            scripted_entries(prompt, &first)
        } else {
            scripted_entries(prompt, &again)
        }
    })
}

fn texts_of(sent: &Sent) -> Vec<String> {
    task(sent)["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["proposal"].as_str().unwrap().to_owned())
        .collect()
}

fn passages_of(sent: &Sent) -> Vec<String> {
    task(sent)["referencePassages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["text"].as_str().unwrap().to_owned())
        .collect()
}

fn withheld(_: &Shows) -> Option<Value> {
    Some(decided(
        "not_established",
        "external_fact",
        &[],
        &[],
        "none",
        "none",
    ))
}

/// An actionable internal conflict citing and placed at `Section 1`.
fn grounded(shows: &Shows) -> Option<Value> {
    let part = shows.part("Section 1");
    Some(decided(
        "actionable",
        "internal_conflict",
        &[part.clone()],
        &[],
        &part,
        "none",
    ))
}

#[tokio::test]
async fn every_matching_reference_is_offered_while_the_window_has_room() -> Result<()> {
    let sources: Vec<_> = (0..30)
        .map(|i| {
            source(
                &format!("source-{i}"),
                &format!("The harbour crane lifts crate {i} before the tide turns."),
            )
        })
        .collect();
    let model = answering(withheld);
    let issues = proposal("The harbour crane lifts no crate before the tide turns.");
    run(
        &model,
        &issues,
        Beside::NONE,
        &json!({"blocks":[],"questions":[]}),
        &sources,
    )
    .await?;
    // Not the former eight: every passage that matches and fits.
    assert_eq!(passages_of(&asks(&model)[0]).len(), 30);
    Ok(())
}

#[tokio::test]
async fn a_complete_answer_in_any_order_is_accepted() -> Result<()> {
    let model = scripted(|prompt, _| {
        let text = scripted_entries(prompt, |shows| {
            if shows.proposal == "Proposal 0." {
                grounded(shows)
            } else {
                withheld(shows)
            }
        });
        let mut answer: Value = serde_json::from_str(&text).unwrap();
        answer["entries"].as_array_mut().unwrap().reverse();
        answer.to_string()
    });
    let issues = proposed(&["Proposal 0.", "Proposal 1."]);
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &[]).await?;
    assert_eq!(checked.established.len(), 1);
    assert_eq!(checked.established[0].note, "Proposal 0.");
    assert_eq!(asks(&model).len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_finding_left_out_is_asked_for_again_alone_and_the_others_stand() -> Result<()> {
    let model = answering_twice(
        |shows| {
            (shows.proposal != "The rope is never tied.")
                .then(|| grounded(shows))
                .flatten()
        },
        grounded,
    );
    let issues = proposed(&["The kettle is described twice.", "The rope is never tied."]);
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &[]).await?;
    assert_eq!(
        placed(checked.established),
        [
            (issues[0].note.clone(), Some(FindingSite::Section(0))),
            (issues[1].note.clone(), Some(FindingSite::Section(0))),
        ]
    );
    let sent = asks(&model);
    assert_eq!(sent.len(), 2);
    assert_eq!(texts_of(&sent[0]).len(), 2);
    assert_eq!(texts_of(&sent[1]), ["The rope is never tied."]);
    Ok(())
}

#[tokio::test]
async fn a_decision_that_cites_nothing_is_corrected_for_that_finding_alone() -> Result<()> {
    let first = |shows: &Shows| {
        if shows.proposal == "The kettle is described twice." {
            Some(decided(
                "actionable",
                "external_fact",
                &[],
                &[],
                "none",
                "none",
            ))
        } else {
            grounded(shows)
        }
    };
    let again = |shows: &Shows| {
        Some(decided(
            "actionable",
            "external_fact",
            &[],
            &[shows.reference(0)],
            "none",
            "none",
        ))
    };
    let model = answering_twice(first, again);
    let issues = proposed(&["The kettle is described twice.", "The rope is never tied."]);
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &kettle()).await?;
    assert_eq!(
        placed(checked.established),
        [
            (issues[0].note.clone(), Some(FindingSite::Lesson)),
            (issues[1].note.clone(), Some(FindingSite::Section(0))),
        ]
    );
    let sent = asks(&model);
    assert_eq!(sent.len(), 2);
    assert_eq!(texts_of(&sent[1]), ["The kettle is described twice."]);
    assert!(prompt_of(&sent[1]).contains("cites at least one reference passage in referenceIds"));
    Ok(())
}

#[tokio::test]
async fn a_finding_never_decided_is_not_established_and_the_others_are() -> Result<()> {
    let model = answering(|shows| {
        (shows.proposal != "The kettle is described twice.")
            .then(|| grounded(shows))
            .flatten()
    });
    let issues = proposed(&["The kettle is described twice.", "The rope is never tied."]);
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &[]).await?;
    assert_eq!(
        placed(checked.established),
        [(issues[1].note.clone(), Some(FindingSite::Section(0)))]
    );
    assert!(checked.left.is_empty());
    // Asked together, then the one left out asked again alone.
    assert!(asks(&model).len() >= 2);
    Ok(())
}

#[tokio::test]
async fn a_decision_broken_again_leaves_its_finding_not_established() -> Result<()> {
    let model = answering(|shows| {
        if shows.proposal == "The kettle is described twice." {
            Some(decided(
                "actionable",
                "lesson_contract",
                &[],
                &[],
                &shows.part("Section 1"),
                "none",
            ))
        } else {
            grounded(shows)
        }
    });
    let issues = proposed(&["The kettle is described twice.", "The rope is never tied."]);
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &[]).await?;
    assert_eq!(
        placed(checked.established),
        [(issues[1].note.clone(), Some(FindingSite::Section(0)))]
    );
    assert_eq!(asks(&model).len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_decision_that_cannot_be_read_is_not_established() -> Result<()> {
    let model = answering(|shows| {
        if shows.proposal == "The kettle is described twice." {
            let mut unreadable = grounded(shows)?;
            unreadable["verdict"] = json!("maybe");
            Some(unreadable)
        } else {
            grounded(shows)
        }
    });
    let issues = proposed(&["The kettle is described twice.", "The rope is never tied."]);
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &[]).await?;
    assert_eq!(
        placed(checked.established),
        [(issues[1].note.clone(), Some(FindingSite::Section(0)))]
    );
    Ok(())
}

#[tokio::test]
async fn a_malformed_actionable_finding_is_asked_for_once_more() -> Result<()> {
    let model = answering_twice(
        |shows| {
            Some(decided(
                "actionable",
                "lesson_contract",
                &[],
                &[],
                &shows.part("Section 1"),
                "none",
            ))
        },
        grounded,
    );
    let issues = proposal("The exercise omitted its required deliverable.");
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &[]).await?;
    assert_eq!(
        placed(checked.established),
        [(issues[0].note.clone(), Some(FindingSite::Section(0)))]
    );
    // The refused decision is asked for once more, with its reason.
    let sent = asks(&model);
    assert_eq!(sent.len(), 2);
    assert!(prompt_of(&sent[1]).contains("cites at least one candidate part in candidateIds"));
    Ok(())
}

/// What the check establishes when the model answers one finding with `decide` every time it is asked, and how many asks that took.
async fn established_by(
    decide: impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static,
    issues: &[ReviewFinding],
    sources: &[LearningSourceDto],
) -> Result<(Vec<(String, Option<FindingSite>)>, usize)> {
    let model = answering(decide);
    let checked = run(&model, issues, Beside::NONE, &lesson(), sources).await?;
    Ok((placed(checked.established), asks(&model).len()))
}

#[tokio::test]
async fn flat_responses_still_require_the_right_evidence_for_internal_conflicts() -> Result<()> {
    let issues = proposal("The two statements in the candidate are incompatible.");
    let conflict = |cited: fn(&Shows) -> Vec<String>| {
        move |shows: &Shows| {
            Some(decided(
                "actionable",
                "internal_conflict",
                &cited(shows),
                &[],
                &shows.part("Question 1"),
                "none",
            ))
        }
    };
    let nothing: fn(&Shows) -> Vec<String> = |_| Vec::new();
    let (found, asks) = established_by(conflict(nothing), &issues, &[]).await?;
    assert!(found.is_empty());
    assert_eq!(asks, 2);
    let by_reference = |shows: &Shows| {
        Some(decided(
            "actionable",
            "internal_conflict",
            &[],
            &[shows.reference(0)],
            &shows.part("Question 1"),
            "none",
        ))
    };
    let (found, _) = established_by(by_reference, &issues, &kettle()).await?;
    assert!(found.is_empty());
    let cited: fn(&Shows) -> Vec<String> = |shows| vec![shows.part("Question 1")];
    let (found, asks) = established_by(conflict(cited), &issues, &[]).await?;
    assert_eq!(
        found,
        [(issues[0].note.clone(), Some(FindingSite::Question(0)))]
    );
    assert_eq!(asks, 1);
    // Free-text reasons never silently promote a negative model verdict.
    let (found, _) = established_by(
        |shows| {
            Some(decided(
                "not_established",
                "internal_conflict",
                &[],
                &[],
                &shows.part("Question 1"),
                "none",
            ))
        },
        &issues,
        &[],
    )
    .await?;
    assert!(found.is_empty());
    Ok(())
}

#[tokio::test]
async fn external_factual_corrections_require_reference_evidence() -> Result<()> {
    let issues = proposal("The kettle boils when the water reaches its limit.");
    let external = |cited: fn(&Shows) -> (Vec<String>, Vec<String>)| {
        move |shows: &Shows| {
            let (candidates, references) = cited(shows);
            Some(decided(
                "actionable",
                "external_fact",
                &candidates,
                &references,
                "none",
                "none",
            ))
        }
    };
    let candidate_only: fn(&Shows) -> (Vec<String>, Vec<String>) =
        |shows| (vec![shows.part("Section 1")], Vec::new());
    let (found, asks) = established_by(external(candidate_only), &issues, &kettle()).await?;
    assert!(found.is_empty());
    assert_eq!(asks, 2);
    let invented: fn(&Shows) -> (Vec<String>, Vec<String>) =
        |_| (Vec::new(), vec!["r99".to_owned()]);
    let (found, _) = established_by(external(invented), &issues, &kettle()).await?;
    assert!(found.is_empty());
    // Reference evidence names no part of the lesson: the whole lesson.
    let referenced: fn(&Shows) -> (Vec<String>, Vec<String>) =
        |shows| (Vec::new(), vec![shows.reference(0)]);
    let (found, _) = established_by(external(referenced), &issues, &kettle()).await?;
    assert_eq!(found, [(issues[0].note.clone(), Some(FindingSite::Lesson))]);
    // A finding the section review already placed keeps its section.
    let section = vec![ReviewFinding::reviewer(
        1,
        Some(FindingSite::Section(4)),
        "The kettle boils when the water reaches its limit, said Section 5.",
    )];
    let model = answering(external(referenced));
    let checked = run(&model, &section, Beside::NONE, &lesson(), &kettle()).await?;
    assert_eq!(checked.established, section);
    let (found, _) = established_by(
        |_| {
            Some(decided(
                "not_established",
                "external_fact",
                &[],
                &[],
                "none",
                "none",
            ))
        },
        &issues,
        &kettle(),
    )
    .await?;
    assert!(found.is_empty());
    // An answer with no entries decides nothing.
    let model = ScriptedModel::new().on_request::<ReviewEvidence>(|_, _| reply("{}".to_owned()));
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &kettle()).await?;
    assert!(checked.established.is_empty());
    Ok(())
}

#[tokio::test]
async fn instructional_corrections_need_a_candidate_part() -> Result<()> {
    let issues = proposal("The exercise omits a required deliverable.");
    let contract = |cited: fn(&Shows) -> (Vec<String>, Vec<String>)| {
        move |shows: &Shows| {
            let (candidates, references) = cited(shows);
            Some(decided(
                "actionable",
                "lesson_contract",
                &candidates,
                &references,
                &shows.part("Section 1"),
                "none",
            ))
        }
    };
    let reference_only: fn(&Shows) -> (Vec<String>, Vec<String>) =
        |shows| (Vec::new(), vec![shows.reference(0)]);
    let (found, asks) = established_by(contract(reference_only), &issues, &kettle()).await?;
    assert!(found.is_empty());
    assert_eq!(asks, 2);
    let candidate: fn(&Shows) -> (Vec<String>, Vec<String>) =
        |shows| (vec![shows.part("Section 1")], Vec::new());
    let (found, asks) = established_by(contract(candidate), &issues, &kettle()).await?;
    assert_eq!(
        found,
        [(issues[0].note.clone(), Some(FindingSite::Section(0)))]
    );
    assert_eq!(asks, 1);
    Ok(())
}

#[tokio::test]
async fn a_short_or_long_reason_is_read_not_refused() -> Result<()> {
    let issues = proposal("The two statements in the candidate are incompatible.");
    let long = "The candidate gives incompatible descriptions of the same scenario. ".repeat(20);
    assert!(long.trim().chars().count() > 700);
    for reason in ["Yes.".to_owned(), long] {
        let (found, asks) = established_by(
            move |shows| {
                let part = shows.part("Question 1");
                let mut decision = decided(
                    "actionable",
                    "internal_conflict",
                    &[part.clone()],
                    &[],
                    &part,
                    "none",
                );
                decision["reason"] = json!(reason);
                Some(decision)
            },
            &issues,
            &[],
        )
        .await?;
        assert_eq!(
            found,
            [(issues[0].note.clone(), Some(FindingSite::Question(0)))]
        );
        assert_eq!(asks, 1);
    }
    Ok(())
}

#[tokio::test]
async fn a_blank_reason_is_still_refused() -> Result<()> {
    let issues = proposal("The two statements in the candidate are incompatible.");
    let model = answering(|shows| {
        let part = shows.part("Question 1");
        let mut decision = decided(
            "actionable",
            "internal_conflict",
            &[part.clone()],
            &[],
            &part,
            "none",
        );
        decision["reason"] = json!("   ");
        Some(decision)
    });
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &[]).await?;
    assert!(checked.established.is_empty());
    let sent = asks(&model);
    assert_eq!(sent.len(), 2);
    assert!(prompt_of(&sent[1]).contains("Give the reason for this decision in reason."));
    Ok(())
}

/// A lesson of five sections and one question, so that parts have places to differ.
fn parts_lesson() -> Value {
    sections(5)
}

/// A decision on the one finding with the given verdict and basis, citing the parts labelled `cited` and the references numbered `referenced`, placed at the part labelled `part` (none: `none`).
fn part_decision(
    verdict: &'static str,
    basis: &'static str,
    cited: &'static [&'static str],
    referenced: &'static [usize],
    part: &'static str,
) -> impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static {
    move |shows| {
        let candidates: Vec<String> = cited.iter().map(|label| shows.part(label)).collect();
        let references: Vec<String> = referenced.iter().map(|n| shows.reference(*n)).collect();
        let placed = if part == "none" {
            "none".to_owned()
        } else {
            shows.part(part)
        };
        Some(decided(
            verdict,
            basis,
            &candidates,
            &references,
            &placed,
            "none",
        ))
    }
}

async fn placed_by(
    decide: impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static,
    issues: &[ReviewFinding],
) -> Result<Vec<ReviewFinding>> {
    let model = answering(decide);
    Ok(
        run(&model, issues, Beside::NONE, &parts_lesson(), &kettle())
            .await?
            .established,
    )
}

#[tokio::test]
async fn an_issue_is_placed_at_the_part_the_decision_names_not_the_lowest_it_cites() -> Result<()> {
    let issues = proposal("The worked example contradicts its explanation.");
    for (part, site) in [
        ("Section 5", FindingSite::Section(4)),
        ("Section 3", FindingSite::Section(2)),
        ("Question 1", FindingSite::Question(0)),
    ] {
        let found = placed_by(
            part_decision(
                "actionable",
                "internal_conflict",
                &["Section 3", "Section 5"],
                &[],
                part,
            ),
            &issues,
        )
        .await?;
        assert_eq!(
            placed(found),
            [(issues[0].note.clone(), Some(site))],
            "{part}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_issue_cites_a_second_part_beside_the_one_it_is_placed_at() -> Result<()> {
    let issues = proposal("The worked example contradicts its explanation.");
    let found = placed_by(
        part_decision(
            "actionable",
            "internal_conflict",
            &["Section 3", "Section 5"],
            &[],
            "Section 5",
        ),
        &issues,
    )
    .await?;
    assert_eq!(found[0].site, Some(FindingSite::Section(4)));
    assert_eq!(found[0].also, Some(FindingSite::Section(2)));
    Ok(())
}

#[tokio::test]
async fn an_external_fact_issue_is_placed_where_the_decision_says_not_at_the_references(
) -> Result<()> {
    let issues = proposal("The kettle boils when the water reaches its limit.");
    for (part, site) in [
        ("Section 2", FindingSite::Section(1)),
        ("none", FindingSite::Lesson),
    ] {
        let found = placed_by(
            part_decision("actionable", "external_fact", &[], &[0], part),
            &issues,
        )
        .await?;
        assert_eq!(
            placed(found),
            [(issues[0].note.clone(), Some(site))],
            "{part}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_external_fact_may_also_cite_candidate_parts() -> Result<()> {
    let issues = proposal("The kettle boils when the water reaches its limit.");
    let found = placed_by(
        part_decision(
            "actionable",
            "external_fact",
            &["Section 1", "Section 2"],
            &[0],
            "Section 2",
        ),
        &issues,
    )
    .await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].site, Some(FindingSite::Section(1)));
    assert_eq!(found[0].also, None);
    Ok(())
}

#[tokio::test]
async fn a_not_established_decision_needs_no_part_and_its_part_is_ignored() -> Result<()> {
    let issues = proposal("The kettle boils when the water reaches its limit.");
    for part in ["none", "Section 2"] {
        let found = placed_by(
            part_decision("not_established", "external_fact", &[], &[], part),
            &issues,
        )
        .await?;
        assert!(found.is_empty(), "{part}");
    }
    Ok(())
}

#[tokio::test]
async fn a_finding_that_already_has_a_site_keeps_it_whatever_the_decision_names() -> Result<()> {
    let section = vec![ReviewFinding::reviewer(
        1,
        Some(FindingSite::Section(4)),
        "The kettle boils when the water reaches its limit, said Section 5.",
    )];
    for part in ["Section 2", "Question 1", "none"] {
        let found = placed_by(
            part_decision("actionable", "external_fact", &["Section 1"], &[0], part),
            &section,
        )
        .await?;
        assert_eq!(found, section, "{part}");
    }
    Ok(())
}

#[tokio::test]
async fn a_cite_the_request_does_not_show_costs_only_that_cite() -> Result<()> {
    let issues = proposal("The kettle boils when the water reaches its limit.");
    let model = answering(|shows| {
        Some(decided(
            "actionable",
            "external_fact",
            &[],
            &[shows.reference(0), "r99".to_owned()],
            "none",
            "none",
        ))
    });
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &kettle()).await?;
    assert_eq!(
        placed(checked.established),
        [(issues[0].note.clone(), Some(FindingSite::Lesson))]
    );
    assert_eq!(asks(&model).len(), 1);
    let model = answering(|_| {
        Some(decided(
            "actionable",
            "external_fact",
            &[],
            &["r99".to_owned()],
            "none",
            "none",
        ))
    });
    let checked = run(&model, &issues, Beside::NONE, &lesson(), &kettle()).await?;
    assert!(checked.established.is_empty());
    let sent = asks(&model);
    assert_eq!(sent.len(), 2);
    assert!(prompt_of(&sent[1]).contains("cites at least one reference passage in referenceIds"));
    Ok(())
}

#[tokio::test]
async fn a_finding_asked_alone_is_shown_its_own_passages_first() -> Result<()> {
    let sources = vec![source("source-0", CRANE), source("source-1", KILN)];
    let issues = proposed(&[CRANE, KILN]);
    let model = answering_twice(
        |shows| (shows.proposal != KILN).then(|| withheld(shows)).flatten(),
        withheld,
    );
    run(&model, &issues, Beside::NONE, &lesson(), &sources).await?;
    let sent = asks(&model);
    assert_eq!(sent.len(), 2);
    assert_eq!(texts_of(&sent[1]), [KILN]);
    assert_eq!(
        passages_of(&sent[0]).first().map(String::as_str),
        Some(CRANE)
    );
    assert_eq!(
        passages_of(&sent[1]).first().map(String::as_str),
        Some(KILN)
    );
    Ok(())
}

#[tokio::test]
async fn passages_beyond_the_window_are_left_out_and_every_finding_keeps_its_best() -> Result<()> {
    let topics = [
        (
            "The harbour crane lifts crate {i} before the tide turns.",
            "The harbour crane lifts crate nine before the tide turns.",
        ),
        (
            "The kiln fires bowl {i} once the damper is shut.",
            "The kiln fires bowl nine once the damper is shut.",
        ),
        (
            "The quay rope holds boat {i} while the ferry loads.",
            "The quay rope holds boat nine while the ferry loads.",
        ),
    ];
    let sources: Vec<_> = (0..10)
        .flat_map(|i| {
            topics.iter().enumerate().map(move |(topic, (text, _))| {
                source(
                    &format!("source-{topic}-{i}"),
                    &text.replace("{i}", &i.to_string()),
                )
            })
        })
        .collect();
    let notes: Vec<&str> = topics.iter().map(|(_, note)| *note).collect();
    let issues = proposed(&notes);
    let references = ReferenceCollection::lexical(&sources)?;
    let mut best = Vec::new();
    for note in &notes {
        let found = references
            .retrieve_within(note, |passages| passages.len() <= 1)
            .await?;
        best.push(found[0].text.clone());
    }
    let skeleton = Input {
        issues: (0..issues.len())
            .map(|at| Issue {
                at,
                proposal: Told(notes[at].to_owned()),
            })
            .collect(),
        requirements: Told(requirements()),
        parts: parts_of(&lesson()),
        references: Packed::default(),
        rulers: Vec::new(),
    };
    let call = crate::model_call::answer_options();
    let passage_cost = {
        let probe = ScriptedModel::new();
        item_tokens(
            &probe,
            &json!({"id":"r0000","title":"Reference source-0-0","url":null,"text":best[0]}),
        )
    };
    let window = (6_000..60_000)
        .find(|window| {
            let model = ScriptedModel::new().window(*window);
            ask::room::<ReviewEvidence>(&model, &skeleton, &call)
                .is_ok_and(|room| room >= 4 * passage_cost)
        })
        .expect("a window with room for four passages");
    let model = answering(withheld).window(window);
    run(&model, &issues, Beside::NONE, &lesson(), &sources).await?;
    let shown = passages_of(&asks(&model)[0]);
    assert!(shown.len() < sources.len(), "{}", shown.len());
    for passage in &best {
        assert!(shown.contains(passage), "{passage}: {shown:?}");
    }
    Ok(())
}

#[test]
fn a_cut_to_the_room_keeps_the_opening_of_the_passages() {
    let passage = |at: usize| Reference {
        at,
        title: None,
        url: None,
        text: Told(format!("Passage {at} of the harbour almanac.")),
    };
    let input = Input {
        issues: Vec::new(),
        requirements: Told(requirements()),
        parts: Vec::new(),
        references: Packed::new((0..3).map(passage).collect(), |_| 10),
        rulers: Vec::new(),
    };
    let cut = ReviewEvidence::within(&input, 25).unwrap();
    assert_eq!(
        cut.references.iter().map(|r| r.at).collect::<Vec<_>>(),
        [0, 1]
    );
    let none = ReviewEvidence::within(&input, 0).unwrap();
    assert!(none.references.is_empty());
}

#[tokio::test]
async fn each_candidate_part_is_offered_with_the_label_the_user_sees() -> Result<()> {
    let model = answering(withheld);
    run(
        &model,
        &proposal("Section 2 of the lesson is wrong."),
        Beside::NONE,
        &json!({"blocks":[{"body":"one"},{"body":"two"}],"questions":[{"prompt":"ask"}]}),
        &[],
    )
    .await?;
    let sent = asks(&model);
    assert!(prompt_of(&sent[0]).contains("partId"));
    let parts: Vec<_> = task(&sent[0])["candidateParts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| {
            (
                part["id"].as_str().unwrap().to_owned(),
                part["label"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        parts,
        [
            ("p1".to_owned(), "Section 1".to_owned()),
            ("p2".to_owned(), "Section 2".to_owned()),
            ("p3".to_owned(), "Question 1".to_owned()),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn the_request_names_no_internal_id() -> Result<()> {
    let recorded = recorded_findings();
    let model = answering(withheld);
    run(
        &model,
        &proposed(&["The kettle is described twice.", "The rope is never tied."]),
        Beside {
            decides: &DECIDES,
            recorded: &recorded,
        },
        &lesson(),
        &kettle(),
    )
    .await?;
    let sent = asks(&model);
    let task = task(&sent[0]);
    let text = task.to_string();
    for internal in [
        "issue-",
        "blocks-",
        "questions-",
        "reference-",
        "recorded-",
        "check-",
        "sourceVersionId",
    ] {
        assert!(!text.contains(internal), "{internal}: {text}");
    }
    assert_eq!(task["requirements"], requirements());
    assert!(!passages_of(&sent[0]).is_empty());
    Ok(())
}

const DECIDES: [&str; 2] = [
    "whether the kettle is full; whether the lid is shut",
    "whether the rope is tied",
];

fn recorded_findings() -> Vec<ReviewFinding> {
    vec![
        ReviewFinding::rule(
            crate::review_findings::Rule::Schema,
            crate::review_findings::FindingKind::Structure,
            FindingSite::Section(0),
            "Repair note for the first recorded defect.",
        )
        .with_summary("The first recorded defect."),
        ReviewFinding::rule(
            crate::review_findings::Rule::Schema,
            crate::review_findings::FindingKind::Structure,
            FindingSite::Question(1),
            "The second recorded defect.",
        ),
    ]
}

/// The decision about the gate's one proposal with `verdict`, left to what `left` names.
fn gate(
    verdict: &'static str,
    left: impl Fn(&Shows) -> String + Send + Sync + 'static,
) -> impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static {
    move |shows| {
        let actionable = verdict == "actionable";
        let part = shows.part("Section 1");
        let cited = if actionable {
            vec![part.clone()]
        } else {
            Vec::new()
        };
        let placed = if actionable { part } else { "none".to_owned() };
        Some(decided(
            verdict,
            "internal_conflict",
            &cited,
            &[],
            &placed,
            &left(shows),
        ))
    }
}

fn nobody(_: &Shows) -> String {
    "none".to_owned()
}

fn recorded(at: usize) -> impl Fn(&Shows) -> String + Send + Sync + 'static {
    move |shows| shows.recorded.get(at).cloned().unwrap_or_default()
}

fn check_named(at: usize) -> impl Fn(&Shows) -> String + Send + Sync + 'static {
    move |shows| shows.checks.get(at).cloned().unwrap_or_default()
}

/// The gate asked about one proposal while `beside` stand, answering with `decide`: what it sent and what it made of the proposal.
async fn gate_ruled(
    decide: impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static,
    beside: Beside<'_>,
) -> Result<(Vec<Sent>, Checked)> {
    let model = answering(decide);
    let checked = run(
        &model,
        &proposal("The kettle is described twice."),
        beside,
        &json!({"blocks":[{"title":"Part"}],"questions":[]}),
        &kettle(),
    )
    .await?;
    Ok((asks(&model), checked))
}

/// The gate asked while `recorded` stand and no other check is listed.
async fn gate_with(
    decide: impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static,
    recorded: &[ReviewFinding],
) -> Result<(Vec<Sent>, Checked)> {
    gate_ruled(
        decide,
        Beside {
            decides: &[],
            recorded,
        },
    )
    .await
}

/// The gate asked while two other checks have read the candidate.
async fn gate_checks(
    decide: impl Fn(&Shows) -> Option<Value> + Send + Sync + 'static,
    recorded: &[ReviewFinding],
) -> Result<(Vec<Sent>, Checked)> {
    gate_ruled(
        decide,
        Beside {
            decides: &DECIDES,
            recorded,
        },
    )
    .await
}

#[tokio::test]
async fn prompt_snapshot_review_evidence() -> Result<()> {
    let sources = vec![source("source-0", KETTLE)];
    let references = ReferenceCollection::lexical(&sources)?;
    let inner = answering(|_| {
        Some(decided(
            "not_established",
            "internal_conflict",
            &[],
            &[],
            "none",
            "none",
        ))
    });
    let model = RequestLog::new(&inner);
    let issues = vec![
        ReviewFinding::reviewer(1, None, "The kettle is described twice."),
        ReviewFinding::reviewer(2, None, "The rope is never tied."),
    ];
    let recorded = recorded_findings();

    check(
        &model,
        &issues,
        Beside {
            decides: &DECIDES,
            recorded: &recorded,
        },
        &json!({"deliverable":"Record the resulting state."}),
        &json!({"blocks":[{"title":"Part","body":"The kettle boils."},{"title":"Next","body":"Tie the rope."}],"questions":[{"prompt":"Is the lid shut?"}]}),
        &references,
        None,
    )
    .await?;

    model.assert_snapshot::<ReviewEvidence>();
    Ok(())
}

#[test]
fn the_review_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Input {
        issues: vec![Issue {
            at: marks.reference(100_001_usize),
            proposal: marks.told("The harbour crane lifts no crate.".to_owned()),
        }],
        requirements: marks.told(json!({
            "deliverable": "Record the tide table.",
            "lessonShape": ["Keep one idea in each section."]
        })),
        parts: vec![Part {
            site: FindingSite::Section(1),
            label: marks.told("Section 2".to_owned()),
            content: marks
                .told(json!({"title":"Harbour tides","body":"The gate opens at first light."})),
        }],
        references: Packed::new(
            vec![Reference {
                at: marks.reference(100_002_usize),
                title: Some(marks.told("Harbour almanac".to_owned())),
                url: Some(marks.told("https://example.test/almanac".to_owned())),
                text: marks.told("Tide tables are revised each spring.".to_owned()),
            }],
            |_| 1,
        ),
        rulers: vec![
            Ruler::Check {
                at: marks.reference(100_003_usize),
                decides: marks.told("whether the kiln door is shut".to_owned()),
            },
            Ruler::Recorded {
                at: marks.reference(100_004_usize),
                part: marks.told("Section 2".to_owned()),
                defect: marks.told("The damper is never named.".to_owned()),
            },
        ],
    };
    holds::<ReviewEvidence>(&input, &marks);
}

#[test]
fn the_decision_schema_points_at_what_is_shown() {
    let input = Input {
        issues: vec![Issue {
            at: 0,
            proposal: Told("The kettle is described twice.".to_owned()),
        }],
        requirements: Told(requirements()),
        parts: parts_of(&lesson()),
        references: Packed::new(
            vec![Reference {
                at: 0,
                title: None,
                url: None,
                text: Told(KETTLE.to_owned()),
            }],
            |_| 1,
        ),
        rulers: rulers_of(Beside {
            decides: &DECIDES,
            recorded: &recorded_findings(),
        }),
    };
    let schema = schema_of::<ReviewEvidence>(&input, Pointers::Enum)
        .unwrap()
        .as_json()
        .to_string();
    for named in [
        "\"i1\"",
        "\"p1\"",
        "\"p2\"",
        "\"r1\"",
        "\"k1\"",
        "\"k2\"",
        "\"k3\"",
        "\"k4\"",
        "\"none\"",
        "\"candidateIds\"",
        "\"referenceIds\"",
        "\"partId\"",
        "\"leftTo\"",
    ] {
        assert!(schema.contains(named), "{named}: {schema}");
    }
    assert!(!schema.contains("\"p3\""));
}

#[test]
fn the_decision_schema_bounds_no_text_but_a_blank() {
    let input = Input {
        issues: vec![Issue {
            at: 0,
            proposal: Told("The kettle is described twice.".to_owned()),
        }],
        requirements: Told(requirements()),
        parts: parts_of(&lesson()),
        references: Packed::default(),
        rulers: Vec::new(),
    };
    let schema = schema_of::<ReviewEvidence>(&input, Pointers::Enum).unwrap();
    let bounds = length_bounds(schema.as_json());
    assert!(bounds.is_empty(), "{bounds:?}");
}

#[tokio::test]
async fn the_gate_is_shown_the_checks_and_the_recorded_findings_and_asked_where_each_is_left(
) -> Result<()> {
    let (sent, _) = gate_checks(gate("not_established", nobody), &recorded_findings()).await?;
    let shown = task(&sent[0]);
    assert_eq!(
        shown["otherChecks"],
        json!([
            {"id":"k1","decides":DECIDES[0]},
            {"id":"k2","decides":DECIDES[1]}
        ])
    );
    assert_eq!(
        shown["recordedFindings"],
        json!([
            {"id":"k3","part":"Section 1","defect":"The first recorded defect."},
            {"id":"k4","part":"Question 2","defect":"The second recorded defect."}
        ])
    );
    assert!(instructions(&sent[0]).contains(&format!("{SYSTEM}{LEFT_TO}")));
    Ok(())
}

#[tokio::test]
async fn the_gate_is_shown_the_recorded_findings_alone_as_before() -> Result<()> {
    let (sent, _) = gate_with(gate("not_established", nobody), &recorded_findings()).await?;
    let shown = task(&sent[0]);
    assert!(shown.get("otherChecks").is_none());
    assert_eq!(shown["recordedFindings"].as_array().map(Vec::len), Some(2));
    assert!(instructions(&sent[0]).contains(&format!("{SYSTEM}{LEFT_TO}")));
    Ok(())
}

#[tokio::test]
async fn with_checks_and_no_recorded_findings_only_the_checks_are_offered() -> Result<()> {
    let (sent, _) = gate_checks(gate("not_established", nobody), &[]).await?;
    let shown = task(&sent[0]);
    assert_eq!(shown["otherChecks"].as_array().map(Vec::len), Some(2));
    assert!(shown.get("recordedFindings").is_none());
    assert!(instructions(&sent[0]).contains(&format!("{SYSTEM}{LEFT_TO}")));
    Ok(())
}

#[tokio::test]
async fn beside_nothing_nothing_about_other_checks_is_asked() -> Result<()> {
    let (sent, _) = gate_ruled(gate("not_established", nobody), Beside::NONE).await?;
    let shown = task(&sent[0]);
    assert!(shown.get("recordedFindings").is_none());
    assert!(shown.get("otherChecks").is_none());
    assert!(instructions(&sent[0]).contains(&format!("{SYSTEM}{NO_OTHER_CHECK}")));
    assert!(!instructions(&sent[0]).contains("otherChecks"));
    Ok(())
}

#[tokio::test]
async fn a_proposal_naming_a_recorded_finding_is_not_established_and_not_asked_again() -> Result<()>
{
    let (sent, checked) = gate_with(gate("actionable", recorded(0)), &recorded_findings()).await?;
    assert!(checked.established.is_empty());
    assert_eq!(sent.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_proposal_naming_no_check_is_established_as_before() -> Result<()> {
    let (_, checked) = gate_checks(gate("actionable", nobody), &recorded_findings()).await?;
    assert_eq!(checked.established.len(), 1);
    assert_eq!(checked.established[0].site, Some(FindingSite::Section(0)));
    Ok(())
}

#[tokio::test]
async fn a_not_established_proposal_naming_a_recorded_finding_stays_so() -> Result<()> {
    let (_, checked) =
        gate_with(gate("not_established", recorded(1)), &recorded_findings()).await?;
    assert!(checked.established.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_actionable_proposal_left_to_a_check_is_not_established_and_not_asked_again(
) -> Result<()> {
    let (sent, checked) = gate_checks(gate("actionable", check_named(1)), &[]).await?;
    assert!(checked.established.is_empty());
    assert_eq!(sent.len(), 1);
    Ok(())
}

#[tokio::test]
async fn an_actionable_proposal_left_to_a_check_that_breaks_an_evidence_rule_is_asked_for_again(
) -> Result<()> {
    // Actionable with no evidence breaks the protocol, as for none.
    let model = answering_twice(
        |shows| {
            Some(decided(
                "actionable",
                "external_fact",
                &[],
                &[],
                "none",
                &shows.checks[0],
            ))
        },
        gate("actionable", check_named(0)),
    );
    let checked = run(
        &model,
        &proposal("The kettle is described twice."),
        Beside {
            decides: &DECIDES,
            recorded: &[],
        },
        &json!({"blocks":[{"title":"Part"}],"questions":[]}),
        &kettle(),
    )
    .await?;
    assert_eq!(asks(&model).len(), 2);
    assert_eq!(checked.left.len(), 1);
    assert!(checked.left[0].established.is_some());
    Ok(())
}

#[tokio::test]
async fn a_proposal_left_to_a_check_that_recorded_nothing_is_handed_back_with_the_checks_place(
) -> Result<()> {
    let issues = proposal("The kettle is described twice.");
    let (_, checked) = gate_checks(gate("actionable", check_named(1)), &[]).await?;
    assert!(checked.established.is_empty());
    let mut placed = issues[0].clone();
    placed.site = Some(FindingSite::Section(0));
    assert_eq!(
        checked.left,
        vec![Left {
            check: 1,
            finding: issues[0].clone(),
            established: Some(placed),
        }]
    );
    Ok(())
}

#[tokio::test]
async fn a_not_established_proposal_left_to_a_check_is_handed_back_with_nothing_established(
) -> Result<()> {
    let issues = proposal("The kettle is described twice.");
    let (_, checked) = gate_checks(gate("not_established", check_named(1)), &[]).await?;
    assert!(checked.established.is_empty());
    assert_eq!(
        checked.left,
        vec![Left {
            check: 1,
            finding: issues[0].clone(),
            established: None,
        }]
    );
    Ok(())
}

#[tokio::test]
async fn an_actionable_proposal_naming_a_recorded_finding_is_neither_established_nor_left(
) -> Result<()> {
    let (_, checked) = gate_checks(gate("actionable", recorded(0)), &recorded_findings()).await?;
    assert!(checked.established.is_empty());
    assert!(checked.left.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_system_text_sent_ends_with_the_sentence_that_leaves_leftto_to_say_who_rules(
) -> Result<()> {
    let (sent, _) = gate_checks(gate("not_established", nobody), &[]).await?;
    assert!(instructions(&sent[0]).contains(
        " Judge every proposal on its evidence whatever its leftTo: leftTo says only which check rules on it."
    ));
    Ok(())
}

#[tokio::test]
async fn a_proposal_left_to_a_recorded_finding_or_to_none_is_not_handed_back() -> Result<()> {
    let recorded_to_first = gate("not_established", recorded(0));
    let (_, checked) = gate_checks(recorded_to_first, &recorded_findings()).await?;
    assert!(checked.left.is_empty(), "recorded");
    let (_, checked) = gate_checks(gate("not_established", nobody), &recorded_findings()).await?;
    assert!(checked.left.is_empty(), "none");
    Ok(())
}

#[tokio::test]
async fn a_finding_may_cite_nine_evidence_ids() -> Result<()> {
    let issues = proposal("The nine sections contradict one another.");
    let model = answering(|shows| {
        let cited: Vec<String> = (1..=9)
            .map(|number| shows.part(&format!("Section {number}")))
            .collect();
        let first = cited[0].clone();
        Some(decided(
            "actionable",
            "internal_conflict",
            &cited,
            &[],
            &first,
            "none",
        ))
    });
    let checked = run(&model, &issues, Beside::NONE, &sections(9), &[]).await?;
    assert_eq!(checked.established.len(), 1);
    assert_eq!(checked.established[0].site, Some(FindingSite::Section(0)));
    assert_eq!(checked.established[0].also, Some(FindingSite::Section(1)));
    assert_eq!(asks(&model).len(), 1);
    Ok(())
}
