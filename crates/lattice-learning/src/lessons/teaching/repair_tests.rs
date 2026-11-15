#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::super::repair_script::{
    finding_labels, first_label, key_item, quote_label, sentence_label, task_of,
};
use super::*;
use crate::lessons::text_edits::{OWN_FIGURES, OWN_SENTENCE};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::test_support::assert_round_trip;
use lattice_core::application::services::model_contract::test_support::{
    ScriptedModel, Sent as Asked,
};
use std::sync::atomic::{AtomicUsize, Ordering};

const STOVE: &str = "The kettle sits on the stove.";
const LID: &str = "The lid is open.";
const ROPE: &str = "The rope hangs by the door.";
const KNOT: &str = "The knot is tied.";
const KETTLE: &str = "The kettle boils when the water reaches its limit.";
const EXCERPT: &str = "The kettle boils when the water reaches its limit. The lid stays shut.";

#[test]
fn the_rewrite_contract_reads_what_it_renders() {
    assert_round_trip::<TeachingRewrite>(&json!({"blocks":[],"questions":[]}).to_string());
}

fn block(title: &str, body: &str) -> Value {
    json!({"kind":"explanation","title":title,"body":body,"rubric":[],"sourceIndex":null,"quote":""})
}

/// Two sections, each of two sentences.
fn material() -> Value {
    json!({
        "blocks": [block("First", &format!("{STOVE} {LID}")), block("Second", &format!("{ROPE} {KNOT}"))],
        "questions": []
    })
}

fn at(site: FindingSite, number: u32) -> ReviewFinding {
    ReviewFinding::reviewer(number, Some(site), format!("Note {number}."))
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn prompt_of(sent: &Asked) -> String {
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

fn task(sent: &Asked) -> Value {
    task_of(&prompt_of(sent)).unwrap()
}

/// A model that answers each ask with the entries `answer(task, number of earlier asks)` gives.
fn scripted(answer: impl Fn(&Value, usize) -> Vec<Value> + Send + Sync + 'static) -> ScriptedModel {
    let asked = AtomicUsize::new(0);
    ScriptedModel::new().on_request::<TeachingRepair>(move |sent, _| {
        let earlier = asked.fetch_add(1, Ordering::SeqCst);
        let task = task_of(&prompt_of(sent)).unwrap_or(Value::Null);
        reply(json!({"entries": answer(&task, earlier)}).to_string())
    })
}

fn edits(replace: Vec<Value>) -> Value {
    json!({
        "sentences": {"replace": replace, "insert": [], "remove": [], "move": []},
        "cite": [], "recite": [], "uncite": [], "keys": []
    })
}

fn replacing(at: &str, text: &str) -> Value {
    json!({"at": at, "with": {"text": text, "from": []}})
}

fn entry(of: &str, plan: &str, edits: Value) -> Value {
    json!({"of": of, "plan": plan, "edits": edits})
}

fn label(task: &Value, pointer: &str, sentence: &str) -> String {
    sentence_label(task, pointer, sentence).unwrap()
}

async fn mended_with(
    model: &ScriptedModel,
    findings: &[ReviewFinding],
    keeps: Keeps<'_>,
    material: &Value,
    authoring: &Value,
    quote_checks: &[QuoteCheck],
) -> Result<Mended> {
    let schema = json!({"type": "object"});
    let wording = Wording::default();
    let request = Request {
        system: "",
        schema: &schema,
        authoring,
        candidate: material,
        quote_checks,
        issues: findings
            .iter()
            .map(|finding| Issue {
                finding,
                last_attempt: None,
            })
            .collect(),
        wording: &wording,
        keeps,
        output_tokens: 0,
    };
    mend(model, &request).await
}

async fn mended(
    model: &ScriptedModel,
    findings: &[ReviewFinding],
    keeps: Keeps<'_>,
    material: &Value,
) -> Result<Mended> {
    let authoring = json!({"deliverable": "Record the resulting state."});
    mended_with(model, findings, keeps, material, &authoring, &[]).await
}

fn body_of(mended: &Mended, block: usize) -> String {
    mended.candidate["blocks"][block]["body"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn an_applied_entry_stands_and_says_the_sentences_it_wrote() -> Result<()> {
    let model = scripted(|task, _| {
        let lid = label(task, "/blocks/0/body", LID);
        vec![entry(
            &finding_labels(task)[0],
            "Shut the lid.",
            edits(vec![replacing(&lid, "The lid is shut.")]),
        )]
    });
    let findings = [at(FindingSite::Section(0), 1)];
    let made = mended(&model, &findings, Keeps::Anything, &material()).await?;
    assert!(made.changed);
    assert_eq!(body_of(&made, 0), format!("{STOVE} The lid is shut."));
    assert_eq!(body_of(&made, 1), format!("{ROPE} {KNOT}"));
    assert_eq!(made.outcomes, [Outcome::Applied]);
    assert!(made.written[0]
        .iter()
        .any(|sentence| sentence.contains("The lid is shut.")));
    Ok(())
}

#[tokio::test]
async fn an_entry_with_no_edits_is_declined_with_its_plan() -> Result<()> {
    let model = scripted(|task, _| {
        vec![entry(
            &finding_labels(task)[0],
            "It is right as it stands.",
            edits(vec![]),
        )]
    });
    let findings = [at(FindingSite::Section(0), 1)];
    let made = mended(&model, &findings, Keeps::Anything, &material()).await?;
    assert!(!made.changed);
    assert_eq!(made.candidate, material());
    assert_eq!(
        made.outcomes,
        [Outcome::Declined("It is right as it stands.".into())]
    );
    assert_eq!(made.written, [Vec::<String>::new()]);
    Ok(())
}

#[tokio::test]
async fn a_finding_the_answer_never_names_is_unnamed_and_the_others_stand() -> Result<()> {
    let model = scripted(|task, earlier| {
        if earlier > 0 {
            return Vec::new();
        }
        let lid = label(task, "/blocks/0/body", LID);
        vec![entry(
            &finding_labels(task)[0],
            "Shut the lid.",
            edits(vec![replacing(&lid, "The lid is shut.")]),
        )]
    });
    let findings = [
        at(FindingSite::Section(0), 1),
        at(FindingSite::Section(1), 2),
    ];
    let made = mended(&model, &findings, Keeps::Anything, &material()).await?;
    assert_eq!(made.outcomes, [Outcome::Applied, Outcome::Unnamed]);
    assert_eq!(body_of(&made, 0), format!("{STOVE} The lid is shut."));
    assert_eq!(body_of(&made, 1), format!("{ROPE} {KNOT}"));
    Ok(())
}

#[tokio::test]
async fn an_entry_outside_its_findings_part_is_asked_for_again_alone_and_then_applied() -> Result<()>
{
    let model = scripted(|task, earlier| {
        let labels = finding_labels(task);
        if earlier == 0 {
            let lid = label(task, "/blocks/0/body", LID);
            let stove = label(task, "/blocks/0/body", STOVE);
            vec![
                entry(
                    &labels[0],
                    "Shut the lid.",
                    edits(vec![replacing(&lid, "The lid is shut.")]),
                ),
                entry(
                    &labels[1],
                    "Fix the rope.",
                    edits(vec![replacing(&stove, "The kettle sits on a shelf.")]),
                ),
            ]
        } else {
            let knot = label(task, "/blocks/1/body", KNOT);
            vec![entry(
                &labels[0],
                "Fix the rope.",
                edits(vec![replacing(&knot, "The knot is loose.")]),
            )]
        }
    });
    let findings = [
        at(FindingSite::Section(0), 1),
        at(FindingSite::Section(1), 2),
    ];
    let made = mended(&model, &findings, Keeps::Anything, &material()).await?;
    assert_eq!(made.outcomes, [Outcome::Applied, Outcome::Applied]);
    assert_eq!(body_of(&made, 0), format!("{STOVE} The lid is shut."));
    assert_eq!(body_of(&made, 1), format!("{ROPE} The knot is loose."));
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(finding_labels(&task(&sent[1])).len(), 1);
    let again = prompt_of(&sent[1]);
    assert!(
        again.contains("These edits change /blocks/0, which is outside this finding's part (/blocks/1). Change only sentences and citations of /blocks/1."),
        "{again}"
    );
    Ok(())
}

#[tokio::test]
async fn an_entry_that_meets_an_earlier_one_is_deferred_and_the_earlier_one_stands() -> Result<()> {
    let model = scripted(|task, _| {
        let labels = finding_labels(task);
        let lid = label(task, "/blocks/0/body", LID);
        vec![
            entry(
                &labels[0],
                "Shut it.",
                edits(vec![replacing(&lid, "The lid is shut.")]),
            ),
            entry(
                &labels[1],
                "Seal it.",
                edits(vec![replacing(&lid, "The lid is sealed.")]),
            ),
        ]
    });
    let findings = [
        at(FindingSite::Section(0), 1),
        at(FindingSite::Section(0), 2),
    ];
    let made = mended(&model, &findings, Keeps::Anything, &material()).await?;
    assert_eq!(made.outcomes, [Outcome::Applied, Outcome::Deferred]);
    assert_eq!(body_of(&made, 0), format!("{STOVE} The lid is shut."));
    assert!(made.written[1].is_empty());
    Ok(())
}

fn protect_the_lid() -> Protected {
    Protected {
        unit: 0,
        field: "body".into(),
        quote: LID.into(),
        released: false,
    }
}

fn lid_and_knot(task: &Value) -> Vec<Value> {
    let labels = finding_labels(task);
    let lid = label(task, "/blocks/0/body", LID);
    let knot = label(task, "/blocks/1/body", KNOT);
    vec![
        entry(
            &labels[0],
            "Shut the lid.",
            edits(vec![replacing(&lid, "The lid is shut.")]),
        ),
        entry(
            &labels[1],
            "Loosen it.",
            edits(vec![replacing(&knot, "The knot is loose.")]),
        ),
    ]
}

#[tokio::test]
async fn an_entry_that_changes_a_protected_sentence_is_refused_while_another_stands() -> Result<()>
{
    let model = scripted(|task, _| lid_and_knot(task));
    let findings = [
        at(FindingSite::Section(0), 1),
        at(FindingSite::Section(1), 2),
    ];
    let protected = [protect_the_lid()];
    let keeps = Keeps::Lesson {
        protected: &protected,
    };
    let made = mended(&model, &findings, keeps, &material()).await?;
    assert!(
        matches!(&made.outcomes[0], Outcome::Refused(reasons) if reasons.iter().all(|reason| reason.ends_with('.'))),
        "{:?}",
        made.outcomes
    );
    assert_eq!(made.outcomes[1], Outcome::Applied);
    assert_eq!(body_of(&made, 0), format!("{STOVE} {LID}"));
    assert_eq!(body_of(&made, 1), format!("{ROPE} The knot is loose."));
    Ok(())
}

#[tokio::test]
async fn the_same_edits_are_taken_when_a_repair_keeps_anything() -> Result<()> {
    let model = scripted(|task, _| lid_and_knot(task));
    let findings = [
        at(FindingSite::Section(0), 1),
        at(FindingSite::Section(1), 2),
    ];
    let made = mended(&model, &findings, Keeps::Anything, &material()).await?;
    assert_eq!(made.outcomes, [Outcome::Applied, Outcome::Applied]);
    assert_eq!(body_of(&made, 0), format!("{STOVE} The lid is shut."));
    Ok(())
}

#[tokio::test]
async fn an_entry_that_leaves_no_readable_lesson_is_refused() -> Result<()> {
    let model = scripted(|task, _| {
        let lid = label(task, "/blocks/0/body", LID);
        vec![entry(
            &finding_labels(task)[0],
            "Shut the lid.",
            edits(vec![replacing(&lid, "The lid is shut.")]),
        )]
    });
    let unreadable =
        json!({"blocks":[{"title":"First","body":format!("{STOVE} {LID}")}],"questions":[]});
    let findings = [at(FindingSite::Section(0), 1)];
    let keeps = Keeps::Lesson { protected: &[] };
    let made = mended(&model, &findings, keeps, &unreadable).await?;
    assert_eq!(
        made.outcomes,
        [Outcome::Refused(vec![
            "the repaired material no longer reads as a lesson.".into()
        ])]
    );
    assert!(!made.changed);
    assert_eq!(made.candidate, unreadable);
    Ok(())
}

#[tokio::test]
async fn a_citation_follows_its_sentence_when_the_sentence_is_replaced() -> Result<()> {
    let mut cited = material();
    cited["blocks"][0]["citations"] = json!([{"sentence": LID, "sourceIndex": 0, "quote": KETTLE}]);
    let model = scripted(|task, _| {
        let lid = label(task, "/blocks/0/body", LID);
        vec![entry(
            &finding_labels(task)[0],
            "Shut the lid.",
            edits(vec![replacing(&lid, "The lid is shut.")]),
        )]
    });
    let findings = [at(FindingSite::Section(0), 1)];
    let made = mended(&model, &findings, Keeps::Anything, &cited).await?;
    assert_eq!(
        made.candidate["blocks"][0]["citations"],
        json!([{"sentence": "The lid is shut.", "sourceIndex": 0, "quote": KETTLE}])
    );
    Ok(())
}

fn with_excerpt() -> Value {
    json!({"deliverable": "Record the resulting state.", "sources": [{"sourceIndex": 0, "excerpt": EXCERPT}]})
}

#[tokio::test]
async fn a_citation_asked_for_cites_the_quotable_sentence_it_names() -> Result<()> {
    let model = scripted(|task, _| {
        let lid = label(task, "/blocks/0/body", LID);
        let quote = quote_label(task, 0, KETTLE).unwrap();
        vec![entry(
            &finding_labels(task)[0],
            "Cite the lid.",
            json!({
                "sentences": {"replace": [], "insert": [], "remove": [], "move": []},
                "cite": [{"sentence": lid, "quote": quote}], "recite": [], "uncite": [], "keys": []
            }),
        )]
    });
    let findings = [at(FindingSite::Section(0), 1)];
    let made = mended_with(
        &model,
        &findings,
        Keeps::Anything,
        &material(),
        &with_excerpt(),
        &[],
    )
    .await?;
    assert_eq!(made.outcomes, [Outcome::Applied]);
    assert_eq!(
        made.candidate["blocks"][0]["citations"],
        json!([{"sentence": LID, "sourceIndex": 0, "quote": KETTLE}])
    );
    Ok(())
}

#[tokio::test]
async fn the_request_lists_one_row_for_each_finding_under_the_label_of_its_id() -> Result<()> {
    let named = at(FindingSite::Section(0), 1).with_passage("Passage one.");
    let again = at(FindingSite::Section(0), 1);
    let mut sentenced = at(FindingSite::Question(1), 4);
    sentenced.sentence = Some("Whole sentence.".to_owned());
    let both = at(FindingSite::Section(1), 3).with_also(FindingSite::Section(3));
    let whole = ReviewFinding::reviewer(0, None, "Note 0.");
    let model = scripted(|_, _| Vec::new());
    let findings = [named, again, sentenced, both, whole];
    mended(&model, &findings, Keeps::Anything, &material()).await?;
    let sent = model.sent();
    let task = task(&sent[0]);
    let rows = task["issuesToFix"].as_array().unwrap();
    let labels: Vec<&str> = rows
        .iter()
        .map(|row| row["finding"].as_str().unwrap())
        .collect();
    assert_eq!(labels[0], labels[1]);
    let distinct: BTreeSet<&str> = labels.iter().copied().collect();
    assert_eq!(distinct.len(), 4);
    assert_eq!(finding_labels(&task).len(), 4);
    let bare: Vec<Value> = rows
        .iter()
        .map(|row| {
            let mut row = row.clone();
            row.as_object_mut().unwrap().remove("finding");
            row
        })
        .collect();
    assert_eq!(
        bare,
        [
            json!({"part":"/blocks/0","also":null,"passage":"Passage one.","note":"Note 1.","lastAttempt":null}),
            json!({"part":"/blocks/0","also":null,"passage":null,"note":"Note 1.","lastAttempt":null}),
            json!({"part":"/questions/1","also":null,"passage":"Whole sentence.","note":"Note 4.","lastAttempt":null}),
            json!({"part":"/blocks/1","also":"/blocks/3","passage":null,"note":"Note 3.","lastAttempt":null}),
            json!({"part":null,"also":null,"passage":null,"note":"Note 0.","lastAttempt":null}),
        ]
    );
    let keys: Vec<&String> = rows[0].as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        ["finding", "part", "also", "passage", "note", "lastAttempt"]
    );
    Ok(())
}

#[tokio::test]
async fn the_task_marks_quotable_sentences_and_names_the_citation_a_quote_check_is_about(
) -> Result<()> {
    let mut cited = material();
    cited["blocks"][0]["citations"] = json!([{"sentence": LID, "sourceIndex": 0, "quote": KETTLE}]);
    let checks = [
        QuoteCheck {
            path: "/blocks/0".into(),
            source_index: Some(0),
            quote: KETTLE.into(),
            error: "It matches no saved reference.".into(),
        },
        QuoteCheck {
            path: "/blocks/1".into(),
            source_index: Some(1),
            quote: "No such words.".into(),
            error: "It matches no saved reference.".into(),
        },
    ];
    let model = scripted(|_, _| Vec::new());
    let findings = [at(FindingSite::Section(0), 1)];
    mended_with(
        &model,
        &findings,
        Keeps::Anything,
        &cited,
        &with_excerpt(),
        &checks,
    )
    .await?;
    let sent = model.sent();
    let task = task(&sent[0]);
    let named = &task["sourceQuoteChecks"][0];
    assert_eq!(named["error"], "It matches no saved reference.");
    assert!(
        named["citation"]
            .as_str()
            .is_some_and(|handle| handle.starts_with('c')),
        "{named}"
    );
    assert_eq!(
        task["sourceQuoteChecks"][1],
        json!({"path":"/blocks/1","sourceIndex":1,"quote":"No such words.","error":"It matches no saved reference."})
    );
    assert!(quote_label(&task, 0, KETTLE).is_some());
    assert_eq!(
        task["originalRequirements"]["deliverable"],
        "Record the resulting state."
    );
    Ok(())
}

#[test]
fn the_instructions_hold_the_four_writer_rules_only_when_the_material_takes_citations() {
    let with = system(true);
    let without = system(false);
    for text in [&with, &without] {
        assert!(text.contains(KEPT_TO_THE_CONTRACT));
        assert!(text.starts_with(OPENING));
    }
    for rule in [
        NEVER_SAY_WHERE,
        QUOTED_WORDING,
        UNQUOTED_REMARKS,
        STEPS_IN_ORDER,
    ] {
        assert!(with.contains(rule), "{rule}");
        assert!(!without.contains(rule), "{rule}");
    }
    assert!(with.contains(OWN_SENTENCE) && with.contains(OWN_FIGURES));
    assert!(!without.contains(OWN_SENTENCE));
    assert!(without.ends_with(NO_CITATIONS));
    assert!(!with.contains(NO_CITATIONS));
}

fn input_of(findings: &[ReviewFinding], excerpts: &[Value]) -> Input {
    let issues: Vec<Issue<'_>> = findings
        .iter()
        .map(|finding| Issue {
            finding,
            last_attempt: None,
        })
        .collect();
    let excerpts: Vec<Excerpt> = excerpts
        .iter()
        .enumerate()
        .map(|(at, entry)| Excerpt {
            at,
            entry: Told(entry.clone()),
        })
        .collect();
    Input {
        issues: sents_of(&issues),
        requirements: Told(json!({"deliverable": "Record the resulting state."})),
        excerpts: Packed::new(excerpts, |_| 10),
        candidate: Told(material()),
        quote_checks: Told(Vec::new()),
        cites: true,
        wording: Internal::of(Wording::default()),
        output_tokens: 100,
    }
}

#[test]
fn within_cuts_the_excerpts_and_nothing_else() {
    let findings = [at(FindingSite::Section(0), 1)];
    let excerpts: Vec<Value> = (0..3)
        .map(|index| json!({"sourceIndex": index, "excerpt": EXCERPT}))
        .collect();
    let input = input_of(&findings, &excerpts);
    let cut = TeachingRepair::within(&input, 25).unwrap();
    assert_eq!(cut.excerpts.len(), 2);
    let (mut whole, mut cut) = (
        serde_json::to_value(&input).unwrap(),
        serde_json::to_value(&cut).unwrap(),
    );
    whole.as_object_mut().unwrap().remove("excerpts");
    cut.as_object_mut().unwrap().remove("excerpts");
    assert_eq!(whole, cut);
}

#[test]
fn findings_sharing_an_id_are_one_sent_that_reaches_every_part_of_theirs() {
    let first = at(FindingSite::Section(0), 1);
    let second = at(FindingSite::Section(0), 1).with_also(FindingSite::Section(2));
    let other = at(FindingSite::Section(1), 2);
    let issues: Vec<Issue<'_>> = [&first, &second, &other]
        .into_iter()
        .map(|finding| Issue {
            finding,
            last_attempt: None,
        })
        .collect();
    let sents = sents_of(&issues);
    assert_eq!(sents.len(), 2);
    assert_eq!(sents[0].rows.len(), 2);
    assert_eq!(
        sents[0].reach.value(),
        &Reach::Parts(vec!["/blocks/0".into(), "/blocks/2".into()])
    );
    assert_eq!(
        sents[1].reach.value(),
        &Reach::Parts(vec!["/blocks/1".into()])
    );
}

#[test]
fn a_planned_item_and_a_finding_about_the_whole_material_reach_anywhere() {
    let planned = ReviewFinding::plan(
        crate::review_findings::PlanCheck::Planned,
        "action-1",
        FindingSite::Section(1),
        "Alpha holds the line.",
    );
    let whole = ReviewFinding::reviewer(0, None, "n");
    assert_eq!(Reach::of(&[&planned]), Reach::Anywhere);
    assert_eq!(Reach::of(&[&whole]), Reach::Anywhere);
    let own = at(FindingSite::Section(0), 1);
    assert_eq!(Reach::of(&[&own, &whole]), Reach::Anywhere);
}

fn parts(of: &[&str]) -> BTreeSet<String> {
    of.iter().map(|part| (*part).to_owned()).collect()
}

#[test]
fn a_reach_refuses_the_first_part_outside_it_in_the_words_a_correction_tells_the_model() {
    let reach = Reach::Parts(vec!["/blocks/0".into(), "/blocks/2".into()]);
    assert_eq!(reach.refuses(&parts(&["/blocks/0", "/blocks/2"])), None);
    assert_eq!(
        reach.refuses(&parts(&["/blocks/0", "/blocks/1"])).unwrap(),
        "These edits change /blocks/1, which is outside this finding's part (/blocks/0 and /blocks/2). Change only sentences and citations of /blocks/0 and /blocks/2."
    );
    assert_eq!(
        reach.refuses(&parts(&[""])).unwrap(),
        "These edits change a text outside every block and question, which is outside this finding's part (/blocks/0 and /blocks/2). Change only sentences and citations of /blocks/0 and /blocks/2."
    );
    assert_eq!(Reach::Anywhere.refuses(&parts(&["/blocks/9", ""])), None);
}

fn question(first: &str, second: &str) -> Value {
    json!({
        "kind": "multipleChoice",
        "prompt": "Which tide comes first?",
        "options": [first, second],
        "correctIndex": 0,
        "rationales": ["Neap is not it.", "Spring is it."],
        "explanation": "Springs run higher.",
        "sourceIndex": null,
        "quote": "",
        "citations": []
    })
}

fn quiz_material() -> Value {
    json!({
        "blocks": [block("First", &format!("{STOVE} {LID}"))],
        "questions": [question("Neap.", "Spring."), question("Slack.", "Ebb.")]
    })
}

fn moving_key(task: &Value, question: usize) -> Vec<Value> {
    let right = first_label(task, &format!("/questions/{question}/options/1")).unwrap();
    vec![entry(
        &finding_labels(task)[0],
        "Move the key.",
        json!({
            "sentences": {"replace": [], "insert": [], "remove": [], "move": []},
            "cite": [], "recite": [], "uncite": [],
            "keys": [{"right": right, "wasWrong": "Neap tides run lower."}]
        }),
    )]
}

#[tokio::test]
async fn a_key_is_moved_with_its_reasons_inside_the_findings_part() -> Result<()> {
    let model = scripted(|task, _| moving_key(task, 0));
    let findings = [at(FindingSite::Question(0), 1)];
    let made = mended(&model, &findings, Keeps::Anything, &quiz_material()).await?;
    assert_eq!(made.outcomes, [Outcome::Applied]);
    let question = &made.candidate["questions"][0];
    assert_eq!(question["correctIndex"], json!(1));
    assert_eq!(question["rationales"], json!(["Neap tides run lower.", ""]));
    assert_eq!(made.candidate["questions"][1]["correctIndex"], json!(0));
    assert_eq!(made.written[0], vec!["Neap tides run lower.".to_owned()]);
    Ok(())
}

#[tokio::test]
async fn a_key_of_a_question_outside_the_findings_part_is_refused_by_the_reach() -> Result<()> {
    let model = scripted(|task, _| moving_key(task, 1));
    let findings = [at(FindingSite::Question(0), 1)];
    let made = mended(&model, &findings, Keeps::Anything, &quiz_material()).await?;
    assert_ne!(made.outcomes, [Outcome::Applied]);
    assert_eq!(made.candidate["questions"][1]["correctIndex"], json!(0));
    let again = prompt_of(&model.sent()[1]);
    assert!(
        again.contains("outside this finding's part (/questions/0)"),
        "{again}"
    );
    Ok(())
}

#[tokio::test]
async fn a_key_of_a_protected_question_is_refused_by_the_protected_check() -> Result<()> {
    let model = scripted(|task, _| moving_key(task, 0));
    let findings = [at(FindingSite::Question(0), 1)];
    let protected = [Protected {
        unit: 1,
        field: "correctIndex".into(),
        quote: String::new(),
        released: false,
    }];
    let keeps = Keeps::Lesson {
        protected: &protected,
    };
    let made = mended(&model, &findings, keeps, &quiz_material()).await?;
    assert!(
        matches!(&made.outcomes[0], Outcome::Refused(_)),
        "{:?}",
        made.outcomes
    );
    assert_eq!(made.candidate["questions"][0]["correctIndex"], json!(0));
    Ok(())
}

#[test]
fn the_instructions_hold_the_key_rule_once_right_after_the_sentence_rule() {
    for text in [system(true), system(false)] {
        assert_eq!(text.matches(KEYS).count(), 1);
        let sentences = text.find(SENTENCES).unwrap();
        assert_eq!(text.find(KEYS).unwrap(), sentences + SENTENCES.len() + 1);
    }
}

#[test]
fn a_flat_key_change_becomes_a_keys_item_for_the_first_sentence_of_the_right_option() {
    let task =
        json!({"candidate": {"questions": [{"options": ["[u1] Three", "[u2] Four. [u3] Yes."]}]}});
    let flat = json!({"path": "/questions/0/correctIndex", "before": 0, "after": 1});
    assert_eq!(
        key_item(&task, &flat),
        Some(json!({"right": "u2", "wasWrong": "It is not the right answer."}))
    );
    let said = json!({"path": "/questions/0/correctIndex", "before": 1, "after": 0, "wasWrong": "Four is too many."});
    assert_eq!(
        key_item(&task, &said),
        Some(json!({"right": "u1", "wasWrong": "Four is too many."}))
    );
    assert_eq!(
        key_item(&task, &json!({"path": "/questions/0/prompt"})),
        None
    );
}
