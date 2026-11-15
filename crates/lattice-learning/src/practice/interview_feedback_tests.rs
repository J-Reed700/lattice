#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use super::*;
use crate::lessons::course_generation_tests::RequestLog;
use crate::practical_dto::*;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};

const LEARNER: &str = "I preserve the intended effect. I would drop the acknowledgement and retry, then check that only one effect was committed.";
const CHECK: &str =
    "I would drop the acknowledgement and retry, then check that only one effect was committed.";
const QUOTE: &str = "An idempotent operation has the same intended effect when repeated.";
const FIRST: &str = "00000000-0000-4000-8000-000000000301";

fn fixture(max_points: &[u32]) -> (ClosingEvidence, LearningSimulationSessionDto) {
    let question = LearningInterviewQuestionDto {
        id: FIRST.into(),
        round: LearningInterviewRoundKind::TechnicalDeepDive,
        question: "Assume an upload response can be lost. How would you make retrying safe?".into(),
        company_rationale: "PRIVATE_PRELIMINARY_HINT".into(),
        follow_up_probes: vec!["PRIVATE_PRELIMINARY_PROBE".into()],
        evaluation_signals: vec!["PRIVATE_PRELIMINARY_SIGNAL".into()],
    };
    let mut second = question.clone();
    second.id = "00000000-0000-4000-8000-000000000302".into();
    second.question = "How would you verify the lost-response case?".into();
    let plan = LearningInterviewPlanDto {
        company_name: "Acme".into(),
        company_url: None,
        target_role: "Engineer".into(),
        round_kind: LearningInterviewRoundKind::TechnicalDeepDive,
        company_summary: "Hypothetical constraints.".into(),
        learner_role: "Candidate".into(),
        counterpart_role: "Interviewer".into(),
        researched_source_version_ids: vec!["PRIVATE_SOURCE_VERSION".into()],
        questions: vec![question, second],
    };
    let questions = plan.questions.iter().map(|question| json!({
        "questionId":question.id,"question":question.question,
        "standard":{
            "referenceAnswer":"Preserve intended effects; verify the lost-response case.",
            "keyPoints":[{"point":"Repeated execution preserves intended effects.","evidence":"sourced","citations":[{
                "sourceId":"00000000-0000-4000-8000-000000000201",
                "versionId":"00000000-0000-4000-8000-000000000202","quote":QUOTE
            }]}],
            "acceptableAlternatives":["Any valid approach preserving effects."],
            "commonMistakes":["Requiring identical response bytes."],
            "scoringCriteria":[],"evidenceGaps":["No company hiring practices are established."]
        }
    })).collect::<Vec<_>>();
    let guides: InterviewAnswerGuides = serde_json::from_value(json!({
        "schemaVersion":1,"authorModel":"test","reviewerModel":"test","questions":questions
    }))
    .expect("guide fixture");
    let rubric = ["Evidence and reasoning", "Collaboration"]
        .into_iter()
        .zip(max_points)
        .enumerate()
        .map(|(index, (title, max_points))| LearningRubricCriterion {
            id: format!("00000000-0000-4000-8000-{:012}", index + 401),
            title: title.into(),
            description: title.into(),
            max_points: *max_points,
        })
        .collect::<Vec<_>>();
    let evidence = ClosingEvidence::new(&plan, &guides, &rubric).expect("boundary fixture");
    let session = LearningSimulationSessionDto {
        id: "session".into(),
        program_id: "program".into(),
        activity_id: "activity".into(),
        activity_revision: 0,
        practice_session_id: None,
        learner_role: "Candidate".into(),
        counterpart_role: "Interviewer".into(),
        status: LearningSimulationStatus::Active,
        revision: 1,
        created_at: 0,
        updated_at: 0,
        submitted_at: None,
        turns: vec![
            LearningSimulationTurnDto {
                id: "interviewer".into(),
                ordinal: 0,
                speaker: LearningSimulationSpeaker::Counterpart,
                content: "I coordinated the team.".into(),
                citations: vec![],
                model_name: None,
                created_at: 0,
            },
            LearningSimulationTurnDto {
                id: "learner".into(),
                ordinal: 1,
                speaker: LearningSimulationSpeaker::Learner,
                content: LEARNER.into(),
                citations: vec![],
                model_name: None,
                created_at: 0,
            },
        ],
    };
    (evidence, session)
}

fn base() -> Value {
    json!({
        "scenario":{"title":"Retry interview","brief":"Test safe retry reasoning."},
        "roles":{"learner":"Candidate","counterpart":"Interviewer"},
    })
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

/// The task a request carries: the one message that is the debrief's JSON.
fn task_of(sent: &Sent) -> Value {
    sent.request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => serde_json::from_str::<Value>(content).ok(),
            _ => None,
        })
        .find(|task| task.get("rubric").is_some())
        .expect("a request carries the task")
}

fn messages_of(sent: &Sent) -> String {
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

/// The label a learner sentence containing `needle` is shown under.
fn label_of(task: &Value, needle: &str) -> String {
    task["transcript"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|turn| turn["speaker"] == "learner")
        .flat_map(|turn| turn["content"].as_str().unwrap().lines())
        .find(|row| row.contains(needle))
        .and_then(|row| row.strip_prefix('['))
        .and_then(|row| row.split_once(']'))
        .map(|(label, _)| label.to_owned())
        .expect("a labelled sentence")
}

/// The id of the level of `points` listed under a criterion of the rubric.
fn level_of(criterion: &Value, points: u64) -> String {
    criterion["levels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|level| level["points"] == points)
        .and_then(|level| level["id"].as_str())
        .expect("a level")
        .to_owned()
}

/// An answer to the scorecard request with an entry for each criterion `decide` gives fields for.
fn entries(task: &Value, decide: impl Fn(&Value) -> Option<Value>) -> String {
    let entries: Vec<Value> = task["rubric"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|criterion| {
            let mut entry = serde_json::Map::new();
            entry.insert("of".into(), criterion["id"].clone());
            entry.extend(decide(criterion)?.as_object().cloned().unwrap());
            Some(Value::Object(entry))
        })
        .collect();
    json!({ "entries": entries }).to_string()
}

fn observed(task: &Value, criterion: &Value, points: u64, needle: &str) -> Value {
    json!({
        "level": level_of(criterion, points),
        "learnerQuotes": [label_of(task, needle)],
        "feedback": "You described concrete verification after a lost response.",
    })
}

fn unobserved(feedback: &str) -> Value {
    json!({"level":"none","learnerQuotes":[],"feedback":feedback})
}

fn scorecard(task: &Value) -> String {
    entries(task, |criterion| {
        Some(if criterion["title"] == "Evidence and reasoning" {
            observed(task, criterion, 4, "drop the acknowledgement")
        } else {
            unobserved("No collaboration example was provided.")
        })
    })
}

fn review(approach: &str, answered: &[&str], sources: &[&str]) -> String {
    json!({
        "referenceApproach":approach,
        "nextPracticeStep":"Repeat the verification with concurrent duplicates.",
        "answered":answered,"sources":sources,
    })
    .to_string()
}

fn model(
    scorecards: impl Fn(&Sent, usize) -> String + Send + Sync + 'static,
    reviews: impl Fn(&Sent, usize) -> String + Send + Sync + 'static,
) -> ScriptedModel {
    ScriptedModel::new()
        .on_request::<ClosingScorecard>(move |sent, nth| reply(scorecards(sent, nth)))
        .on_request::<ClosingReview>(move |sent, nth| reply(reviews(sent, nth)))
}

fn plain(approach: &'static str) -> ScriptedModel {
    model(
        |sent, _| scorecard(&task_of(sent)),
        move |_, _| review(approach, &["q1"], &["e1"]),
    )
}

async fn close(
    model: &dyn LLMPort,
    evidence: &ClosingEvidence,
    session: &LearningSimulationSessionDto,
) -> Result<SimulationJson> {
    evidence
        .close(model, "Close the interview.", &base(), session)
        .await
}

#[tokio::test]
async fn a_closing_debrief_names_sentences_levels_and_choices_and_the_app_renders_it() -> Result<()>
{
    let (evidence, session) = fixture(&[4, 4]);
    let model = plain("Preserve intended effects across retries; verify a lost response.");
    let result = close(&model, &evidence, &session).await?;
    assert_eq!(result.citations.len(), 1);
    assert_eq!(
        result.citations[0].source_id,
        "00000000-0000-4000-8000-000000000201"
    );
    assert_eq!(
        result.citations[0].version_id,
        "00000000-0000-4000-8000-000000000202"
    );
    assert_eq!(result.citations[0].quote, QUOTE);
    assert_eq!(result.evaluated_question_ids, Some(vec![FIRST.into()]));
    assert!(result
        .content
        .contains("Evidence and reasoning: 4/4 (provisional)"));
    assert!(result
        .content
        .contains("Collaboration: insufficient evidence; no points assigned"));
    assert!(result
        .content
        .contains(&format!("Learner evidence: “{CHECK}”")));
    assert!(!result.content.contains("Collaboration: 0/4"));
    assert!(result
        .content
        .contains("No company hiring practices are established."));
    Ok(())
}

#[tokio::test]
async fn the_requests_show_labels_levels_and_sentences_and_nothing_internal() -> Result<()> {
    let (evidence, session) = fixture(&[4, 2]);
    let model = plain("Preserve intended effects.");
    close(&model, &evidence, &session).await?;
    let sent = model.sent();
    let scoring = sent
        .iter()
        .find(|sent| sent.call.id == ClosingScorecard::ID)
        .unwrap();
    let task = task_of(scoring);
    let text = messages_of(scoring);
    assert!(!text.contains("00000000-0000-4000-8000-"));
    assert!(!text.contains("PRIVATE_"));
    assert!(text.contains("referenceAnswer"));
    assert!(text.contains(QUOTE));
    assert_eq!(task["rubric"][0]["id"], "c1");
    assert_eq!(task["rubric"][0]["levels"].as_array().unwrap().len(), 5);
    assert_eq!(task["rubric"][1]["levels"].as_array().unwrap().len(), 3);
    assert_eq!(task["rubric"][1]["levels"][0]["id"], "l1");
    assert_eq!(task["interviewPlan"]["questions"][0]["id"], "q1");
    assert_eq!(task["answerGuideEvidence"][0]["id"], "e1");
    assert_eq!(task["answerGuideEvidence"][0]["questionId"], "q1");
    assert_eq!(task["transcript"][0]["content"], "I coordinated the team.");
    assert!(task["transcript"][1]["content"]
        .as_str()
        .unwrap()
        .starts_with("[u1] I preserve the intended effect."));
    Ok(())
}

#[tokio::test]
async fn a_criterion_the_answer_leaves_out_is_asked_for_alone_and_the_others_stand() -> Result<()> {
    let (evidence, session) = fixture(&[4, 4]);
    let model = model(
        |sent, nth| {
            let task = task_of(sent);
            if nth == 0 {
                entries(&task, |criterion| {
                    (criterion["title"] == "Evidence and reasoning")
                        .then(|| observed(&task, criterion, 3, "drop the acknowledgement"))
                })
            } else {
                scorecard(&task)
            }
        },
        |_, _| review("Preserve intended effects.", &["q1"], &["e1"]),
    );
    let result = close(&model, &evidence, &session).await?;
    let sent = model.sent();
    let scoring: Vec<_> = sent
        .iter()
        .filter(|sent| sent.call.id == ClosingScorecard::ID)
        .collect();
    assert_eq!(scoring.len(), 2);
    let again = task_of(scoring[1]);
    assert_eq!(again["rubric"].as_array().unwrap().len(), 1);
    assert_eq!(again["rubric"][0]["id"], "c2");
    assert!(result
        .content
        .contains("Evidence and reasoning: 3/4 (provisional)"));
    assert!(result
        .content
        .contains("Collaboration: insufficient evidence"));
    Ok(())
}

#[tokio::test]
async fn a_level_of_another_criterion_is_no_level_here() -> Result<()> {
    let (evidence, session) = fixture(&[4, 2]);
    let model = model(
        |sent, nth| {
            let task = task_of(sent);
            entries(&task, |criterion| {
                Some(if criterion["title"] == "Evidence and reasoning" {
                    observed(&task, criterion, 4, "drop the acknowledgement")
                } else if nth == 0 {
                    json!({"level":"l5","learnerQuotes":[label_of(&task, "preserve")],"feedback":"Collaboration was shown."})
                } else {
                    unobserved("No collaboration example was provided.")
                })
            })
        },
        |_, _| review("Preserve intended effects.", &["q1"], &["e1"]),
    );
    let result = close(&model, &evidence, &session).await?;
    assert!(result
        .content
        .contains("Collaboration: insufficient evidence"));
    assert_eq!(model.count::<ClosingScorecard>(), 2);
    Ok(())
}

#[tokio::test]
async fn an_observed_criterion_without_sentences_is_corrected_alone() -> Result<()> {
    let (evidence, session) = fixture(&[4, 4]);
    let model = model(
        |sent, nth| {
            let task = task_of(sent);
            entries(&task, |criterion| {
                Some(if criterion["title"] != "Evidence and reasoning" {
                    unobserved("No collaboration example was provided.")
                } else if nth == 0 {
                    json!({"level":level_of(criterion, 4),"learnerQuotes":[],"feedback":"Strong verification."})
                } else {
                    observed(&task, criterion, 4, "drop the acknowledgement")
                })
            })
        },
        |_, _| review("Preserve intended effects.", &["q1"], &["e1"]),
    );
    let result = close(&model, &evidence, &session).await?;
    let sent = model.sent();
    let scoring: Vec<_> = sent
        .iter()
        .filter(|sent| sent.call.id == ClosingScorecard::ID)
        .collect();
    assert_eq!(scoring.len(), 2);
    assert!(messages_of(scoring[1]).contains("needs learnerQuotes"));
    assert_eq!(task_of(scoring[1])["rubric"].as_array().unwrap().len(), 1);
    assert!(result.content.contains(CHECK));
    Ok(())
}

#[tokio::test]
async fn unobserved_feedback_uses_an_app_owned_explanation() -> Result<()> {
    let (evidence, session) = fixture(&[4, 4]);
    let model = model(
        |sent, _| {
            let task = task_of(sent);
            entries(&task, |criterion| {
                Some(if criterion["title"] == "Evidence and reasoning" {
                    observed(&task, criterion, 4, "drop the acknowledgement")
                } else {
                    json!({"level":"none","learnerQuotes":[label_of(&task, "preserve")],"feedback":"0 points: you lack the ability to collaborate."})
                })
            })
        },
        |_, _| review("Preserve intended effects.", &["q1"], &["e1"]),
    );
    let result = close(&model, &evidence, &session).await?;
    assert!(!result.content.contains("0 points"));
    assert!(!result.content.contains("lack the ability"));
    assert!(!result.content.contains("preserve the intended effect."));
    assert!(result
        .content
        .contains("does not contain enough learner evidence"));
    Ok(())
}

#[tokio::test]
async fn a_scorecard_nothing_of_which_can_be_read_fails_the_debrief() {
    let (evidence, session) = fixture(&[4, 4]);
    let model = model(|_, _| "no scorecard".into(), |_, _| String::new());
    assert!(close(&model, &evidence, &session).await.is_err());
    assert_eq!(model.count::<ClosingReview>(), 0);
}

#[tokio::test]
async fn the_review_is_told_the_scores_and_sees_no_levels() -> Result<()> {
    let (evidence, session) = fixture(&[4, 4]);
    let model = plain("Preserve intended effects.");
    close(&model, &evidence, &session).await?;
    let sent = model.sent();
    let reviewing = sent
        .iter()
        .find(|sent| sent.call.id == ClosingReview::ID)
        .unwrap();
    let task = task_of(reviewing);
    assert_eq!(task["scorecard"][0]["score"], "4/4");
    assert_eq!(
        task["scorecard"][1]["score"],
        "insufficient evidence; no points assigned"
    );
    assert!(task["rubric"][0].get("levels").is_none());
    assert_eq!(
        task["transcript"][1]["content"], LEARNER,
        "the review reads the learner's turn as written"
    );
    Ok(())
}

#[tokio::test]
async fn an_answered_sourced_question_needs_a_source_and_is_corrected() -> Result<()> {
    let (evidence, session) = fixture(&[4, 4]);
    let model = model(
        |sent, _| scorecard(&task_of(sent)),
        |_, nth| {
            if nth == 0 {
                review("Preserve intended effects.", &["q1"], &[])
            } else {
                review("Preserve intended effects.", &["q1"], &["e1"])
            }
        },
    );
    let result = close(&model, &evidence, &session).await?;
    assert_eq!(result.citations.len(), 1);
    assert_eq!(model.count::<ClosingReview>(), 2);
    let sent = model.sent();
    let corrected = sent
        .iter()
        .filter(|sent| sent.call.id == ClosingReview::ID)
        .nth(1)
        .unwrap();
    assert!(messages_of(corrected).contains("evidence label of question q1"));
    Ok(())
}

#[tokio::test]
async fn a_credited_score_needs_an_answered_question() -> Result<()> {
    let (evidence, session) = fixture(&[4, 4]);
    let model = model(
        |sent, _| scorecard(&task_of(sent)),
        |_, nth| {
            if nth == 0 {
                review("Preserve intended effects.", &[], &[])
            } else {
                review("Preserve intended effects.", &["q1"], &["e1"])
            }
        },
    );
    let result = close(&model, &evidence, &session).await?;
    assert_eq!(model.count::<ClosingReview>(), 2);
    assert_eq!(result.evaluated_question_ids, Some(vec![FIRST.into()]));
    Ok(())
}

#[tokio::test]
async fn a_selected_source_names_its_question_and_equal_quotes_count_once() -> Result<()> {
    let (evidence, session) = fixture(&[4, 4]);
    let model = model(
        |sent, _| scorecard(&task_of(sent)),
        |_, _| review("Preserve intended effects.", &["q1"], &["e1", "e2"]),
    );
    let result = close(&model, &evidence, &session).await?;
    assert_eq!(
        result.evaluated_question_ids,
        Some(vec![
            FIRST.into(),
            "00000000-0000-4000-8000-000000000302".into()
        ])
    );
    assert_eq!(
        result.citations.len(),
        1,
        "Identical canonical quotes are deduplicated"
    );
    Ok(())
}

#[tokio::test]
async fn with_no_answered_question_and_no_credit_the_debrief_stands_unsourced() -> Result<()> {
    let (evidence, session) = fixture(&[4]);
    let model = model(
        |sent, _| {
            let task = task_of(sent);
            entries(&task, |_| Some(unobserved("Nothing was answered.")))
        },
        |_, _| review("Insufficient evidence.", &[], &[]),
    );
    let result = close(&model, &evidence, &session).await?;
    assert!(result.citations.is_empty());
    assert_eq!(result.evaluated_question_ids, Some(vec![]));
    assert!(!result.content.contains("Source limits"));
    Ok(())
}

#[tokio::test]
async fn a_long_closing_debrief_is_read_not_refused() -> Result<()> {
    let (evidence, mut session) = fixture(&[4, 4]);
    let learner = "I would retry once the acknowledgement was dropped. ".repeat(20);
    session.turns[1].content = format!("{LEARNER} {}", learner.trim());
    let essay =
        "Preserve intended effects across retries and verify the lost response. ".repeat(70);
    let wordy = "You described a concrete check after the lost response. ".repeat(30);
    let model = model(
        |sent, _| {
            let task = task_of(sent);
            entries(&task, |criterion| {
                Some(if criterion["title"] == "Evidence and reasoning" {
                    json!({
                        "level": level_of(criterion, 4),
                        "learnerQuotes": [label_of(&task, "drop the acknowledgement")],
                        "feedback": "You described a concrete check after the lost response. ".repeat(30).trim(),
                    })
                } else {
                    unobserved("No collaboration example was provided.")
                })
            })
        },
        {
            let essay = essay.clone();
            move |_, _| review(essay.trim(), &["q1"], &["e1"])
        },
    );
    let result = close(&model, &evidence, &session).await?;
    assert!(result.content.contains(essay.trim()));
    assert!(result.content.contains(wordy.trim()));
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_closing_scorecard() -> Result<()> {
    let (evidence, session) = fixture(&[4, 2]);
    let inner = plain("Preserve intended effects.");
    let model = RequestLog::new(&inner);
    close(&model, &evidence, &session).await?;
    model.assert_snapshot::<ClosingScorecard>();
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_closing_review() -> Result<()> {
    let (evidence, session) = fixture(&[4, 2]);
    let inner = plain("Preserve intended effects.");
    let model = RequestLog::new(&inner);
    close(&model, &evidence, &session).await?;
    model.assert_snapshot::<ClosingReview>();
    Ok(())
}

fn marked(marks: &mut Marks) -> Material {
    let mut company = Map::new();
    company.insert("companyName".into(), json!("Harbour Works Limited"));
    company.insert("companySummary".into(), json!("A tide table office."));
    Material {
        system: marks.told("Close the harbour interview carefully.".to_owned()),
        scenario: marks.told(json!({"title":"Harbour interview","brief":"Settle a tide table dispute."})),
        roles: marks.told(json!({"learner":"Candidate harbourmaster","counterpart":"Interviewer pilot"})),
        company: marks.told(company),
        questions: vec![Question {
            id: marks.reference("question-100001".to_owned()),
            round: marks.told(json!("technical deep dive")),
            text: marks.told("How would you read a tide table?".to_owned()),
            guide_question: marks.told("How is a tide table read?".to_owned()),
            reference_answer: marks.told("Read the height against the hour.".to_owned()),
            key_points: vec![KeyPoint {
                point: marks.told("Heights are listed by hour.".to_owned()),
                evidence: marks.told(json!("sourced evidence")),
                choices: vec![100_001],
            }],
            alternatives: marks.told(vec!["Use the printed chart.".to_owned()]),
            mistakes: marks.told(vec!["Ignoring the datum.".to_owned()]),
            criteria: marks.told(json!([{"criterion":"Reading heights","fullCredit":"Reads the height against the hour.","partialCredit":"Reads the hour only."}])),
            gaps: marks.told(vec!["The datum is not stated.".to_owned()]),
        }],
        choices: vec![Choice {
            at: marks.reference(100_001_usize),
            question: marks.internal("question-100001".to_owned()),
            quote: marks.told("Heights are listed for each hour of the day.".to_owned()),
            source: marks.internal("source-100001".to_owned()),
            version: marks.internal("version-100001".to_owned()),
        }],
        criteria: vec![Criterion {
            id: marks.reference("criterion-100001".to_owned()),
            title: marks.told("Reading the table".to_owned()),
            description: marks.told("Reads heights against hours.".to_owned()),
            max_points: Told(3),
        }],
        turns: Packed::new(
            vec![
                Turn {
                    learner: true,
                    speaker: Told("learner".to_owned()),
                    content: marks.told("I read the height beside the hour.".to_owned()),
                },
                Turn {
                    learner: false,
                    speaker: marks.told("counterpart".to_owned()),
                    content: marks.told("Which hour matters most?".to_owned()),
                },
            ],
            |_| 10,
        ),
        earlier: Told(0),
    }
}

#[test]
fn the_scorecard_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Scored {
        material: marked(&mut marks),
    };
    holds::<ClosingScorecard>(&input, &marks);
}

#[test]
fn the_review_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Reviewed {
        material: marked(&mut marks),
        standing: vec![Standing {
            criterion: marks.told("Reading the table".to_owned()),
            score: marks.told("2/3 (as given)".to_owned()),
            feedback: marks.told("You read the height correctly.".to_owned()),
            observed: true,
        }],
    };
    holds::<ClosingReview>(&input, &marks);
}
