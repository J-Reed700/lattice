#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::test_support::candidate;
use super::*;
use crate::lessons::course_generation_tests::RequestLog;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};

/// The task a review request carries.
pub(super) fn task_of(request: &CompletionRequest) -> Value {
    request
        .input()
        .iter()
        .find_map(|input| match input {
            CompletionInput::Message { content, .. } if content.trim_start().starts_with('{') => {
                serde_json::Deserializer::from_str(content)
                    .into_iter::<Value>()
                    .next()
                    .and_then(|task| task.ok())
            }
            _ => None,
        })
        .unwrap_or(Value::Null)
}

/// The handle the task shows for passage `number` of section `section`.
pub(super) fn passage(task: &Value, section: usize, number: usize) -> String {
    task["candidate"]["blocks"][section]["bodyPassages"][number]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The handle the task shows for its `at`th open finding.
pub(super) fn open(task: &Value, at: usize) -> String {
    task["openFindings"][at]["id"].as_str().unwrap().to_owned()
}

/// An answer holding `entries`, one to an item.
pub(super) fn items(entries: Vec<Value>) -> String {
    let items: Vec<Value> = entries
        .into_iter()
        .map(|entry| json!({ "entry": entry }))
        .collect();
    json!({ "items": items }).to_string()
}

pub(super) fn issue(note: &str) -> Value {
    json!({"kind":"issue","note":note,"continues":"none"})
}

pub(super) fn defect(at: &str, finding: &str) -> Value {
    json!({"kind":"sectionDefect","passageId":at,"finding":finding,"continues":"none"})
}

pub(super) fn resolved(finding: &str) -> Value {
    json!({"kind":"resolved","openFinding":finding})
}

/// A model that answers each review request of lens `L` with `answer` of its task.
pub(super) fn reviewer<L: Lens>(
    answer: impl Fn(&Value, usize) -> String + Send + Sync + 'static,
) -> ScriptedModel {
    ScriptedModel::new().on_request::<TeachingReview<L>>(move |sent: &Sent, nth| {
        let text = if sent.request.schema().is_none() {
            "The candidate reads as written.".to_owned()
        } else {
            answer(&task_of(&sent.request), nth)
        };
        let reply: BoxFuture<'static, Result<CompletionResponse>> =
            Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))));
        reply
    })
}

pub(super) async fn run(model: &ScriptedModel, scope: Scope<'_>, remit: Remit) -> Result<Judged> {
    review(
        model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        scope,
        remit,
        None,
    )
    .await
}

pub(super) fn asked_at(number: u32, site: FindingSite) -> Asked {
    let finding = ReviewFinding::reviewer(number, Some(site), format!("Alpha {number} fails."));
    Asked {
        id: finding.id(),
        site,
        passage_id: Some(format!("{}-passage-0", site.id_text())),
        note: finding.note.clone(),
    }
}

#[tokio::test]
async fn an_issue_and_a_section_defect_are_read_and_placed() {
    let model = reviewer::<Anything>(|task, _| {
        items(vec![
            issue("The lesson never states its aim."),
            defect(&passage(task, 2, 0), "The rest comes after rolling, not before."),
        ])
    });
    let judged = run(&model, Scope::default(), Remit::Open).await.unwrap();
    assert_eq!(judged.reported.len(), 2);
    assert_eq!(judged.reported[0].site, None);
    assert_eq!(judged.reported[0].summary, "The lesson never states its aim.");
    let placed = &judged.reported[1];
    assert_eq!(placed.site, Some(FindingSite::Section(2)));
    assert_eq!(placed.passage_id.as_deref(), Some("section-2-passage-0"));
    assert_eq!(
        placed.passage.as_deref(),
        Some(passages_of(&candidate())[2][0].1.as_str())
    );
    assert!(placed
        .note
        .starts_with(&format!("{}: ", FindingSite::Section(2).note_label())));
    assert!(judged.verdicts.is_empty());
}

#[tokio::test]
async fn a_defect_that_points_at_nothing_shown_is_left_out_and_the_others_stand() {
    let model = reviewer::<Anything>(|task, _| {
        items(vec![
            defect("p99", "A claim outside the lesson."),
            defect(&passage(task, 1, 0), "The membrane claim contradicts the example."),
        ])
    });
    let judged = run(&model, Scope::default(), Remit::Open).await.unwrap();
    assert_eq!(judged.reported.len(), 1);
    assert_eq!(judged.reported[0].site, Some(FindingSite::Section(1)));
}

#[tokio::test]
async fn a_blank_issue_and_a_blank_defect_are_dropped_and_equal_notes_count_once() {
    let model = reviewer::<Anything>(|task, _| {
        let at = passage(task, 0, 0);
        items(vec![
            issue("   "),
            defect(&at, ""),
            issue("The workload is unrealistic."),
            issue("The workload is unrealistic."),
        ])
    });
    let judged = run(&model, Scope::default(), Remit::Open).await.unwrap();
    assert_eq!(judged.reported.len(), 1);
}

#[tokio::test]
async fn only_passages_of_sections_in_review_carry_a_name() {
    let model = reviewer::<Anything>(|_, _| items(vec![]));
    let scope = Scope {
        sections: Some(&[0, 2]),
        asked: None,
    };
    run(&model, scope, Remit::Open).await.unwrap();
    let task = model
        .sent()
        .iter()
        .filter(|sent| sent.request.schema().is_some())
        .map(|sent| task_of(&sent.request))
        .next()
        .unwrap();
    let blocks = task["candidate"]["blocks"].as_array().unwrap();
    let marks: Vec<Value> = blocks.iter().map(|block| block["inReview"].clone()).collect();
    assert_eq!(marks, [json!(true), json!(false), json!(true)]);
    assert!(blocks[0]["bodyPassages"][0].get("id").is_some());
    assert!(blocks[1]["bodyPassages"][0].get("id").is_none());
    assert!(blocks[1]["bodyPassages"][0].get("text").is_some());
    assert!(blocks.iter().all(|block| block.get("label").is_some()));
}

#[tokio::test]
async fn an_open_finding_reported_resolved_is_gone_and_one_left_unmarked_is_present() {
    let asked = [
        asked_at(1, FindingSite::Section(0)),
        asked_at(2, FindingSite::Section(1)),
    ];
    let model = reviewer::<Anything>(|task, _| items(vec![resolved(&open(task, 1))]));
    let scope = Scope {
        sections: Some(&[0]),
        asked: Some(&asked),
    };
    let judged = run(&model, scope, Remit::Open).await.unwrap();
    let verdict = |id: &FindingId| judged.verdicts.iter().find(|v| &v.id == id).map(|v| v.present);
    assert_eq!(verdict(&asked[0].id), Some(true));
    assert_eq!(verdict(&asked[1].id), Some(false));
}

#[tokio::test]
async fn the_open_findings_are_shown_by_handle_and_the_hash_id_never_is() {
    let asked = [asked_at(1, FindingSite::Section(0))];
    let model = reviewer::<Anything>(|_, _| items(vec![]));
    let scope = Scope {
        sections: Some(&[0]),
        asked: Some(&asked),
    };
    run(&model, scope, Remit::Open).await.unwrap();
    let sent = model.sent();
    let request = sent.iter().find(|s| s.request.schema().is_some()).unwrap();
    let task = task_of(&request.request);
    let entry = &task["openFindings"][0];
    assert_eq!(entry["id"], "f1");
    assert_eq!(entry["part"], "Section 1");
    assert_eq!(entry["passageId"], passage(&task, 0, 0));
    assert!(!task.to_string().contains(asked[0].id.as_str()));
}

#[tokio::test]
async fn a_report_carries_the_open_finding_it_continues() {
    let asked = [asked_at(1, FindingSite::Section(0))];
    let model = reviewer::<Anything>(|task, _| {
        let mut continuing = issue("The same gap, worded differently.");
        continuing["continues"] = json!(open(task, 0));
        items(vec![continuing, issue("A different gap.")])
    });
    let scope = Scope {
        sections: Some(&[0]),
        asked: Some(&asked),
    };
    let judged = run(&model, scope, Remit::Open).await.unwrap();
    assert_eq!(judged.reported[0].continues.as_ref(), Some(&asked[0].id));
    assert_eq!(judged.reported[1].continues, None);
}

#[tokio::test]
async fn a_cut_off_answer_keeps_what_closed_leaves_open_findings_as_they_stood_and_asks_for_the_rest()
{
    let asked = [asked_at(1, FindingSite::Section(0))];
    let model = ScriptedModel::new().on_request::<TeachingReview<Anything>>(|sent, _| {
        let reply: BoxFuture<'static, Result<CompletionResponse>> = if sent
            .request
            .schema()
            .is_none()
        {
            Box::pin(std::future::ready(Ok(CompletionResponse::from_text(
                "The candidate reads as written.",
            ))))
        } else if task_of(&sent.request).get("alreadyReported").is_none() {
            let cut = format!(
                "{{\"items\":[{{\"entry\":{}}},{{\"entry\":{{\"kind\":\"iss",
                issue("The first gap stands.")
            );
            Box::pin(std::future::ready(Ok(CompletionResponse {
                finish_reason: "length".into(),
                ..CompletionResponse::from_text(cut)
            })))
        } else {
            Box::pin(std::future::ready(Ok(CompletionResponse::from_text(
                items(vec![issue("The second gap.")]),
            ))))
        };
        reply
    });
    let scope = Scope {
        sections: Some(&[0]),
        asked: Some(&asked),
    };
    let judged = run(&model, scope, Remit::Open).await.unwrap();
    let notes: Vec<&str> = judged.reported.iter().map(|r| r.note.as_str()).collect();
    assert_eq!(notes, ["The first gap stands.", "The second gap."]);
    let later = model
        .sent()
        .iter()
        .map(|sent| task_of(&sent.request))
        .find(|task| task.get("alreadyReported").is_some())
        .unwrap();
    assert_eq!(later["alreadyReported"], json!(["The first gap stands."]));
    assert_eq!(judged.verdicts.len(), 1);
    assert!(judged.verdicts[0].present);
}

#[tokio::test]
async fn a_cut_off_answer_with_no_closed_item_does_not_judge_an_open_finding_present() {
    let asked = [asked_at(1, FindingSite::Section(0))];
    let model = ScriptedModel::new().on_request::<TeachingReview<Anything>>(|sent, _| {
        let text = if sent.request.schema().is_none() {
            "The candidate reads as written.".to_owned()
        } else {
            "{\"items\":[{\"entry\":{\"kind\":\"res".to_owned()
        };
        let reply: BoxFuture<'static, Result<CompletionResponse>> =
            Box::pin(std::future::ready(Ok(CompletionResponse {
                finish_reason: "length".into(),
                ..CompletionResponse::from_text(text)
            })));
        reply
    });
    let scope = Scope {
        sections: Some(&[0]),
        asked: Some(&asked),
    };
    assert!(run(&model, scope, Remit::Open).await.is_err());
}

#[tokio::test]
async fn a_review_that_stays_unreadable_fails() {
    let model = reviewer::<Anything>(|_, _| "no review here".to_owned());
    assert!(run(&model, Scope::default(), Remit::Open).await.is_err());
}

#[tokio::test]
async fn an_empty_complete_review_finds_nothing() {
    let model = reviewer::<Anything>(|_, _| items(vec![]));
    let judged = run(&model, Scope::default(), Remit::Open).await.unwrap();
    assert!(judged.reported.is_empty() && judged.verdicts.is_empty());
}

#[tokio::test]
async fn a_long_finding_is_read_not_refused() {
    let long = "The step is wrong. ".repeat(400);
    let said = long.clone();
    let model = reviewer::<Anything>(move |task, _| {
        items(vec![defect(&passage(task, 0, 0), &said)])
    });
    let judged = run(&model, Scope::default(), Remit::Open).await.unwrap();
    assert_eq!(judged.reported[0].summary, long.trim());
}

#[tokio::test]
async fn the_writers_excerpts_are_shown_the_cited_first() {
    let prompt = json!({"authoringContext":{"sources":[
        {"sourceIndex":0,"text":"Tide tables are revised each spring."},
        {"sourceIndex":1,"text":"The harbour gate opens at first light."}
    ]}});
    let cited = json!({"blocks":[{"body":"The gate opens early.","sourceIndex":1}]});
    let model = reviewer::<Anything>(|_, _| items(vec![]));
    review(
        &model,
        "Review instructional quality.",
        prompt,
        &cited,
        Scope::default(),
        Remit::Open,
        None,
    )
    .await
    .unwrap();
    let task = model
        .sent()
        .iter()
        .filter(|sent| sent.request.schema().is_some())
        .map(|sent| task_of(&sent.request))
        .next()
        .unwrap();
    let order: Vec<u64> = task["authoringContext"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["sourceIndex"].as_u64().unwrap())
        .collect();
    assert_eq!(order, [1, 0]);
}

#[test]
fn the_review_request_shows_what_it_must_and_nothing_internal() {
    let mut marks = Marks::new();
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "x");
    let input = Input {
        system: marks.told("Review the harbour notes with care.".to_owned()),
        prompt: marks.told(json!({"task":"Review instructional quality at the quay","authoringContext":{"goal":"Learn the harbour tides"}})),
        candidate: marks.told(json!({"blocks":[{"label":"Section 1","inReview":true,"bodyPassages":[{"text":"The harbour gate opens at first light."}]}]})),
        passages: vec![Passage {
            id: marks.reference("section-0-passage-0".to_owned()),
            section: 0,
            number: 0,
        }],
        sources: Packed::new(
            vec![marks.told(json!({"text":"Tide tables are revised each spring."}))],
            |_| 10,
        ),
        open: vec![OpenFinding {
            id: marks.reference(finding.id()),
            part: marks.told("Section 1".to_owned()),
            passage: Some(marks.reference("section-0-passage-0".to_owned())),
            note: marks.told("The kiln rota contradicts itself.".to_owned()),
        }],
        reported: marks.told(vec!["An earlier note about tide tables.".to_owned()]),
    };
    holds::<TeachingReview<Anything>>(&input, &marks);
    holds::<TeachingReview<Concerns>>(&input, &marks);
}

#[tokio::test]
async fn prompt_snapshot_teaching_review() {
    let asked = [
        asked_at(1, FindingSite::Section(0)),
        asked_at(2, FindingSite::Section(0)),
    ];
    let scripted = reviewer::<Anything>(|task, _| items(vec![resolved(&open(task, 1))]));
    let log = RequestLog::new(&scripted);
    super::review(
        &log,
        "Review instructional quality.",
        json!({"priorKnowledge":"The learner has seen a map of the town."}),
        &two_parts(),
        Scope {
            sections: Some(&[0]),
            asked: Some(&asked),
        },
        Remit::Open,
        None,
    )
    .await
    .unwrap();
    log.assert_snapshot::<TeachingReview<Anything>>();
}

fn two_parts() -> Value {
    json!({"blocks":[{"body":"Alpha holds the line."},{"body":"Beta holds another."}]})
}

fn shown_prompt() -> String {
    let task = json!({
        "candidate":{"blocks":[
            {"bodyPassages":[{"id":"p1","text":"Alpha holds the line."}]},
            {"bodyPassages":[{"id":"p2","text":"Beta holds another."}]}
        ]},
        "openFindings":[
            {"id":"f1","part":"Section 1","note":"First."},
            {"id":"f2","part":"Section 2","note":"Second."}
        ]
    });
    format!("Review instructional quality.\n{ANSWER_FORM}\n\n{task}")
}

fn translated(script: Value) -> Vec<Value> {
    let reply = scripted::reply(&shown_prompt(), &script.to_string()).unwrap();
    serde_json::from_str::<Value>(&reply).unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["entry"].clone())
        .collect()
}

#[test]
fn a_scripted_issue_becomes_an_issue_entry_that_continues_the_open_finding() {
    let entries = translated(json!({"issues":["A gap.",{"note":"Again.","continues":"reviewer/2"}]}));
    assert_eq!(entries[0], json!({"kind":"issue","note":"A gap.","continues":"none"}));
    assert_eq!(entries[1]["continues"], "f1");
}

#[test]
fn a_scripted_section_defect_lands_on_the_handle_of_its_passage() {
    let entries = translated(json!({"issues":[],"sectionDefects":[
        {"passageId":"section-1-passage-0","finding":"Beta fails."}
    ]}));
    assert_eq!(
        entries,
        [json!({"kind":"sectionDefect","passageId":"p2","finding":"Beta fails.","continues":"none"})]
    );
}

#[test]
fn a_scripted_resolved_verdict_is_an_entry_and_a_present_one_is_none() {
    let entries = translated(json!({"issues":[],"openFindings":[
        {"id":"reviewer/1","quote":"","verdict":"present"},
        {"id":"reviewer/2","quote":"","verdict":"resolved"}
    ]}));
    assert_eq!(entries, [json!({"kind":"resolved","openFinding":"f2"})]);
}

#[test]
fn a_reply_already_in_the_layers_form_passes_through() {
    let new = items(vec![issue("A gap.")]);
    assert_eq!(scripted::reply(&shown_prompt(), &new), None);
    assert_eq!(scripted::reply("Another request.", "{\"issues\":[]}"), None);
}
