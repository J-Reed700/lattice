//! A lesson's review holds it to its course plan: what the plan gives it must
//! be taught, nothing is used before it is taught, and it neither contradicts
//! itself nor points at what the learner cannot see.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::{fixture_completion, CONTRACT_NOTHING};
use crate::lessons::content_verification::Protected;
use crate::lessons::course_generation_tests::{clean_review, verdict, ScriptedModel};
use crate::reference_collection::ReferenceCollection;
use crate::review_findings::{subject_of, Held, PlanCheck};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use std::sync::Mutex;

const EXCERPT: &str = "This saved reference describes the exact behavior of the example tool.";
const OPEN_TERMINAL: &str = "Opening the terminal application";

/// The scripted model, with the lesson's check against its plan answered
/// from `contract` and every question put to it kept.
struct Routed<'a> {
    inner: &'a ScriptedModel,
    contract: Mutex<std::collections::VecDeque<String>>,
    asked: Mutex<Vec<String>>,
    drafting: Mutex<Vec<String>>,
    confirming: Mutex<Vec<(String, String)>>,
    confirmed: Mutex<Vec<String>>,
    confirmed_schemas: Mutex<Vec<Value>>,
}

/// A confirming read nobody scripted: not taught, and an item belongs in
/// the lesson's middle section.
fn not_confirmed(user: &str) -> String {
    if user.contains(contract::confirm::PLANNED_TASK) {
        json!({"taught":false,"quote":"","belongs":"section-1"}).to_string()
    } else if user.contains(contract::confirm::CLASH_TASK) {
        json!({"case":"","contradict":false}).to_string()
    } else {
        json!({"asked":true,"taught":false,"where":"thisLesson","quote":""}).to_string()
    }
}

impl<'a> Routed<'a> {
    fn new(inner: &'a ScriptedModel, answers: Vec<String>) -> Self {
        Self {
            inner,
            contract: Mutex::new(answers.into()),
            asked: Mutex::new(Vec::new()),
            drafting: Mutex::new(Vec::new()),
            confirming: Mutex::new(Vec::new()),
            confirmed: Mutex::new(Vec::new()),
            confirmed_schemas: Mutex::new(Vec::new()),
        }
    }

    /// Answer every confirming read whose prompt holds `needle` with `reply`.
    fn confirming(self, needle: &str, reply: Value) -> Self {
        self.confirming
            .lock()
            .unwrap()
            .push((needle.to_owned(), reply.to_string()));
        self
    }

    fn confirmed(&self) -> Vec<String> {
        self.confirmed.lock().unwrap().clone()
    }

    fn confirmed_schemas(&self) -> Vec<Value> {
        self.confirmed_schemas.lock().unwrap().clone()
    }

    fn drafting(&self) -> Vec<String> {
        self.drafting.lock().unwrap().clone()
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LLMPort for Routed<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        if system.starts_with(contract::confirm::SYSTEM)
            && !crate::lessons::structured_call::is_draft_request(request)
        {
            self.confirmed.lock().unwrap().push(user.to_owned());
            if let Some(schema) = &request.json_schema {
                self.confirmed_schemas.lock().unwrap().push(schema.clone());
            }
            let reply = self
                .confirming
                .lock()
                .unwrap()
                .iter()
                .find(|(needle, _)| user.contains(needle.as_str()))
                .map_or_else(|| not_confirmed(user), |(_, reply)| reply.clone());
            return Ok(CompletionResponse::from_text(reply.as_str()));
        }
        let is_contract = system.starts_with(contract::SYSTEM);
        if is_contract && crate::lessons::structured_call::is_draft_request(request) {
            let said = request.input.iter().filter_map(|item| match item {
                CompletionInput::Message { role, content } if role == "system" => {
                    Some(content.as_str())
                }
                _ => None,
            });
            self.drafting
                .lock()
                .unwrap()
                .push(said.collect::<Vec<_>>().join(
                    "
",
                ));
        }
        if !is_contract || crate::lessons::structured_call::is_draft_request(request) {
            return self.inner.complete(request).await;
        }
        self.asked.lock().unwrap().push(user.to_owned());
        let reply = self
            .contract
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| CONTRACT_NOTHING.to_owned());
        Ok(fixture_completion(request, vec![reply]))
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn protected_sentence(unit: usize, quote: &str) -> Protected {
    serde_json::from_value(json!({"unit": unit, "field": "body", "quote": quote})).unwrap()
}

fn scripted(outputs: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(outputs.into()),
        prompts: Mutex::new(Vec::new()),
    }
}

/// The writer's prompt: the plan's lesson 2 is this one.
fn context(ideas: &[&str], actions: &[&str]) -> String {
    context_planning(ideas, &[], actions, "")
}

fn context_planning(ideas: &[&str], terms: &[&str], actions: &[&str], contrast: &str) -> String {
    json!({
        "sources":[{"sourceIndex":0,"excerpt":EXCERPT}],
        "priorKnowledge":"The learner has used a graphical file manager.",
        "currentModule":{"title":"Shell basics","lessonSequence":[{"title":"Looking around","current":false},{"title":"Working in the terminal","current":true},{"title":"Making files","current":false}]},
        "courseSequence":{"curriculum":[{"title":"Shell basics","outcomes":["Run commands"],"milestone":"A logbook of daily shell habits","lessons":[{"title":"Looking around"},{"title":"Working in the terminal"},{"title":"Making files"}]},{"title":"Secure remote access","outcomes":["Connect to another machine"],"milestone":"A tidy list of saved connections","lessons":[{"title":"Keys and logins"}]}],"coursePlan":[
            {"title":"Looking around","position":"earlier","ideas":["A folder holds files"],"terms":["folder"],"actions":["Listing the files in a folder"],"contrast":""},
            {"title":"Working in the terminal","position":"this","ideas":ideas,"terms":terms,"actions":actions,"contrast":contrast},
            {"title":"Making files","position":"later","ideas":[],"terms":[],"actions":["Creating a file"],"contrast":""}
        ]}
    })
    .to_string()
}

fn block(title: &str, body: &str) -> Value {
    json!({"kind":"explanation","title":title,"body":body,"rubric":[],"sourceIndex":0,"quote":EXCERPT})
}

fn block_of(kind: &str, title: &str, body: &str) -> Value {
    json!({"kind":kind,"title":title,"body":body,"rubric":[],"sourceIndex":0,"quote":EXCERPT})
}

fn lesson(bodies: [&str; 3]) -> Value {
    json!({"blocks":[
        block("Opening", bodies[0]),
        block("Listing", bodies[1]),
        block("Reading", bodies[2])
    ],"questions":[]})
}

fn reviewed(blocks: usize) -> String {
    reviewed_of(&(0..blocks).collect::<Vec<_>>(), &[])
}

/// A review that finds no defect in the sections it is asked about, and gives
/// `verdicts` on the findings it is asked about.
fn reviewed_of(sections: &[usize], verdicts: &[Value]) -> String {
    clean_review(sections, verdicts)
}

fn edits(edits: Value) -> String {
    json!({ "edits": edits }).to_string()
}

fn retitle() -> String {
    retitle_at(0)
}

/// A retitle of `section`, which a finding at that section may make.
fn retitle_at(section: usize) -> String {
    let before = ["Opening", "Listing", "Reading"][section];
    edits(json!([{
        "path": format!("/blocks/{section}/title"),
        "before": before,
        "after": format!("{before}, revised"),
    }]))
}

/// A second retitle of `section`, after `retitle_at`'s.
fn retitle_again_at(section: usize) -> String {
    let before = ["Opening", "Listing", "Reading"][section];
    edits(json!([{
        "path": format!("/blocks/{section}/title"),
        "before": format!("{before}, revised"),
        "after": format!("{before}, revised again"),
    }]))
}

/// A third retitle of `section`, after `retitle_again_at`'s.
fn retitle_third_at(section: usize) -> String {
    let before = ["Opening", "Listing", "Reading"][section];
    edits(json!([{
        "path": format!("/blocks/{section}/title"),
        "before": format!("{before}, revised again"),
        "after": format!("{before}, revised a third time"),
    }]))
}

/// A retitle of sections 1 and 2 together, `before` and `after` being what
/// follows the section names in their titles.
fn retitle_both(before: &str, after: &str) -> String {
    edits(json!([
        {"path":"/blocks/1/title","before":format!("Listing{before}"),"after":format!("Listing{after}")},
        {"path":"/blocks/2/title","before":format!("Reading{before}"),"after":format!("Reading{after}")},
    ]))
}

fn answer(planned: Value, used: Value, contradictions: Value, unseen: Value) -> String {
    json!({"planned":planned,"usedBeforeTaught":used,"contradictions":contradictions,"unseen":unseen})
        .to_string()
}

fn used(quote: &str, what: &str) -> Value {
    json!({"what":what,"uses":[quote],"taughtLater":""})
}

fn not_taught(item: &str) -> Value {
    json!({"item":item,"taught":false,"quote":""})
}

fn taught(item: &str, quote: &str) -> Value {
    json!({"item":item,"taught":true,"quote":quote})
}

async fn run(
    model: &dyn LLMPort,
    candidate: &Value,
    context: &str,
    protected: &[Protected],
) -> Result<Reviewed> {
    run_under(
        model,
        candidate,
        context,
        protected,
        &json!({"type":"object"}),
    )
    .await
}

async fn run_under(
    model: &dyn LLMPort,
    candidate: &Value,
    context: &str,
    protected: &[Protected],
    schema: &Value,
) -> Result<Reviewed> {
    let sources = vec![crate::dto::LearningSourceDto {
        id: "saved-reference".into(),
        title: "Example reference".into(),
        url: None,
        acquired_at: 0,
        excerpt: EXCERPT.into(),
    }];
    let references = ReferenceCollection::lexical(&sources)?;
    review_material(
        model,
        LESSON_REPAIR_SYSTEM,
        context,
        ReviewSchema::Other(schema),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, protected, Held::default()),
    )
    .await
}

fn waiting(reviewed: &Reviewed) -> Vec<(String, Option<String>)> {
    let mut found: Vec<_> = reviewed
        .set_aside
        .iter()
        .map(|waiting| {
            (
                waiting.finding.place().id(),
                waiting.finding.passage.clone(),
            )
        })
        .collect();
    found.sort();
    found
}

fn body(reviewed: &Reviewed, section: usize) -> String {
    let lesson: Value = serde_json::from_str(&reviewed.candidate).unwrap();
    lesson["blocks"][section]["body"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn a_planned_action_the_lesson_does_not_teach_is_a_finding_at_its_section() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let missing = answer(
        json!([not_taught("action-0")]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![missing.clone(), missing.clone(), missing]).confirming(
        OPEN_TERMINAL,
        json!({"taught":false,"quote":"","belongs":"section-1"}),
    );
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[OPEN_TERMINAL]), &[]).await?;
    assert_eq!(waiting(&result), [("teaching:section-1".to_owned(), None)]);
    let note = &result.set_aside[0].finding.note;
    assert!(note.contains(OPEN_TERMINAL), "{note}");
    assert!(
        note.contains("Teach it in the lesson's own words"),
        "{note}"
    );
    assert!(!note.contains("course plan"), "{note}");
    let prompts = scripted.prompts.lock().unwrap();
    let repair = prompts
        .iter()
        .find(|prompt| prompt.contains("issuesToFix"))
        .unwrap();
    assert!(repair.contains("Teach it in the lesson's own words"));
    assert!(!repair.contains("Teach it here"));
    drop(prompts);
    let asked = model.asked();
    assert_eq!(asked.len(), 3);
    for expected in [
        "action-0",
        OPEN_TERMINAL,
        "Listing the files in a folder",
        "The learner has used a graphical file manager.",
        "section-2",
    ] {
        assert!(asked[0].contains(expected), "{expected}");
    }
    let prompt: Value = serde_json::from_str(&asked[0]).unwrap();
    assert!(!prompt["earlierLessons"]
        .to_string()
        .contains("Creating a file"));
    Ok(())
}

#[tokio::test]
async fn a_use_before_it_is_taught_is_a_finding_only_when_its_words_are_in_the_lesson() -> Result<()>
{
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let found = answer(
        json!([]),
        json!([
            used("Type ls -l to see sizes", "the -l option of ls"),
            used("Type mv notes.txt old.txt", "the mv command")
        ]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![found.clone(), found.clone(), found]);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "Type `ls -l`\nto see sizes in the list.",
        "Read the names.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert_eq!(
        waiting(&result),
        [(
            "teaching:section-1".to_owned(),
            Some("Type `ls -l`\nto see sizes".to_owned())
        )]
    );
    assert!(result.set_aside[0]
        .finding
        .note
        .contains("the -l option of ls"));
    let note = &result.set_aside[0].finding.note;
    assert!(note.contains("Teach it in this step"), "{note}");
    assert!(!note.contains("Teach it before this point"), "{note}");
    Ok(())
}

#[tokio::test]
async fn a_planned_item_reported_taught_with_words_not_in_the_lesson_is_not_taught() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let reported = answer(
        json!([
            taught("idea-0", "A terminal takes typed commands."),
            taught("action-0", "Press the terminal icon in the dock")
        ]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(
        &scripted,
        vec![reported.clone(), reported.clone(), reported],
    );
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    let ctx = context(&["Typing commands"], &[OPEN_TERMINAL]);
    let result = run(&model, &candidate, &ctx, &[]).await?;
    assert_eq!(waiting(&result), [("teaching:section-1".to_owned(), None)]);
    assert!(result.set_aside[0].finding.note.contains(OPEN_TERMINAL));
    Ok(())
}

#[tokio::test]
async fn a_planned_item_missing_from_the_answer_is_not_taught() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let model = Routed::new(&scripted, vec![]);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[OPEN_TERMINAL]), &[]).await?;
    assert_eq!(result.set_aside.len(), 1);
    assert!(result.set_aside[0].finding.note.contains(OPEN_TERMINAL));
    Ok(())
}

#[tokio::test]
async fn a_contradiction_is_sited_at_the_statement_that_is_not_protected() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle(),
        retitle(),
        retitle(),
        retitle(),
    ]);
    let clash = answer(
        json!([]),
        json!([]),
        json!([{
            "quote":"Commands like ls take no folder name.",
            "otherQuote":"Type ls Documents to list that folder"
        }]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![clash]).confirming(
        contract::confirm::CLASH_TASK,
        json!({"case":"A command that takes no folder name cannot be given one.","contradict":true}),
    );
    let candidate = lesson([
        "Commands like ls take no folder name.",
        "The list shows names.",
        "Type ls Documents to list that folder.",
    ]);
    let protected = [protected_sentence(
        0,
        "Commands like ls take no folder name.",
    )];
    let result = run(&model, &candidate, &context(&[], &[]), &protected).await?;
    assert_eq!(
        waiting(&result),
        [(
            "teaching:section-2".to_owned(),
            Some("Type ls Documents to list that folder".to_owned())
        )]
    );
    assert_eq!(result.set_aside[0].finding.also, None);
    let note = &result.set_aside[0].finding.note;
    assert!(
        note.contains("Commands like ls take no folder name."),
        "{note}"
    );
    assert!(
        note.contains("Type ls Documents to list that folder"),
        "{note}"
    );
    Ok(())
}

#[tokio::test]
async fn a_contradiction_between_two_parts_that_can_change_is_about_both() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(2),
        reviewed_of(&[2], &[]),
        retitle_again_at(2),
        reviewed_of(&[2], &[]),
    ]);
    let clash = answer(
        json!([]),
        json!([]),
        json!([{
            "quote":"Commands like ls take no folder name.",
            "otherQuote":"Type ls Documents to list that folder"
        }]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![clash.clone(), clash.clone(), clash]).confirming(
        contract::confirm::CLASH_TASK,
        json!({"case":"A command that takes no folder name cannot be given one.","contradict":true}),
    );
    let candidate = lesson([
        "Commands like ls take no folder name.",
        "The list shows names.",
        "Type ls Documents to list that folder.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert_eq!(result.set_aside.len(), 1, "{:?}", waiting(&result));
    let found = &result.set_aside[0].finding;
    assert_eq!(found.site, Some(FindingSite::Section(0)));
    assert_eq!(found.also, Some(FindingSite::Section(2)));
    let after: Value = serde_json::from_str(&result.candidate)?;
    assert_eq!(after["blocks"][2]["title"], "Reading, revised again");
    Ok(())
}

#[tokio::test]
async fn a_finding_in_a_section_with_protected_sentences_is_repaired_when_it_touches_none(
) -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        edits(
            json!([{"path":"/blocks/1/body","before":"Type ls -l to see sizes.","after":"Type ls to list names."}]),
        ),
        reviewed_of(&[1], &[]),
    ]);
    let found = answer(
        json!([]),
        json!([used("Type ls -l to see sizes.", "the -l option of ls")]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![found]);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "Run pwd to print the folder you are in. Type ls -l to see sizes.",
        "Read the names.",
    ]);
    let protected = [protected_sentence(
        1,
        "Run pwd to print the folder you are in.",
    )];
    let result = run(&model, &candidate, &context(&[], &[]), &protected).await?;
    assert!(result.set_aside.is_empty(), "{:?}", waiting(&result));
    assert_eq!(
        body(&result, 1),
        "Run pwd to print the folder you are in. Type ls to list names."
    );
    Ok(())
}

#[tokio::test]
async fn teaching_can_be_added_beside_an_editable_sentence_in_a_section_with_protected_ones(
) -> Result<()> {
    let added = "To open the terminal, press the Terminal icon in the dock and a window with a prompt appears.";
    let scripted = scripted(vec![
        reviewed(3),
        edits(
            json!([{"path":"/blocks/1/body","before":"Then look at the list.","after":format!("Then look at the list. {added}")}]),
        ),
        reviewed_of(&[1], &[]),
    ]);
    let missing = answer(
        json!([not_taught("action-0")]),
        json!([]),
        json!([]),
        json!([]),
    );
    let now_taught = answer(
        json!([taught("action-0", "press the Terminal icon in the dock")]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![missing, now_taught]);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "Run pwd to print the folder you are in. Then look at the list.",
        "Read the names.",
    ]);
    let protected = [protected_sentence(
        1,
        "Run pwd to print the folder you are in.",
    )];
    let ctx = context(&[], &[OPEN_TERMINAL]);
    let result = run(&model, &candidate, &ctx, &protected).await?;
    assert!(result.set_aside.is_empty(), "{:?}", waiting(&result));
    let text = body(&result, 1);
    assert!(text.starts_with("Run pwd to print the folder you are in."));
    assert!(text.contains(added), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_recheck_after_a_repair_checks_the_whole_lesson_again() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        edits(json!([
            {"path":"/blocks/0/body","before":"Type ls -l to see sizes.","after":"Type ls to list names."},
            {"path":"/blocks/2/body","before":"Read the names.","after":"Then rename a file with mv old.txt new.txt."}
        ])),
        reviewed_of(&[0, 2], &[]),
        edits(
            json!([{"path":"/blocks/2/body","before":"Then rename a file with mv old.txt new.txt.","after":"Read the names."}]),
        ),
        reviewed_of(&[2], &[]),
    ]);
    let first = answer(
        json!([]),
        json!([]),
        json!([{
            "quote":"Type ls -l to see sizes.",
            "otherQuote":"Read the names."
        }]),
        json!([]),
    );
    let second = answer(
        json!([]),
        json!([used(
            "rename a file with mv old.txt new.txt",
            "the mv command"
        )]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![first, second]).confirming(
        contract::confirm::CLASH_TASK,
        json!({"case":"One lists sizes and the other only names.","contradict":true}),
    );
    let candidate = lesson([
        "Type ls -l to see sizes.",
        "The list shows names.",
        "Read the names.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert!(result.set_aside.is_empty(), "{:?}", waiting(&result));
    let asked = model.asked();
    assert_eq!(asked.len(), 3);
    assert!(asked[1].contains("mv old.txt new.txt"));
    assert_eq!(body(&result, 2), "Read the names.");
    Ok(())
}

fn repairs_asked(model: &ScriptedModel) -> usize {
    model
        .prompts
        .lock()
        .unwrap()
        .iter()
        .filter(|prompt| prompt.contains("issuesToFix"))
        .count()
}

const FIRST_USE: &str = "Use the first tool to list names.";
const SECOND_USE: &str = "Use the second tool to sort names.";
const THIRD_USE: &str = "Use the third tool to count names.";

fn uses_of(sentences: &[(&str, &str)]) -> String {
    let uses: Vec<Value> = sentences
        .iter()
        .map(|(sentence, what)| used(sentence, what))
        .collect();
    answer(json!([]), json!(uses), json!([]), json!([]))
}

fn replace_in_listing(before: &str, after: &str) -> String {
    edits(json!([{"path":"/blocks/1/body","before":before,"after":after}]))
}

#[tokio::test]
async fn a_third_defect_in_a_section_gets_its_own_repairs() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        replace_in_listing(FIRST_USE, "Name the first tool before listing names."),
        reviewed_of(&[1], &[]),
        replace_in_listing(SECOND_USE, "Name the second tool before sorting names."),
        reviewed_of(&[1], &[]),
        replace_in_listing(THIRD_USE, "Name the third tool before counting names."),
        reviewed_of(&[1], &[]),
    ]);
    let model = Routed::new(
        &scripted,
        vec![
            uses_of(&[(FIRST_USE, "the first tool")]),
            uses_of(&[(SECOND_USE, "the second tool")]),
            uses_of(&[(THIRD_USE, "the third tool")]),
        ],
    );
    let listing = format!("{FIRST_USE} {SECOND_USE} {THIRD_USE}");
    let candidate = lesson([
        "A terminal takes typed commands.",
        listing.as_str(),
        "Read the names.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert_eq!(repairs_asked(&scripted), 3);
    assert!(result.set_aside.is_empty(), "{:?}", waiting(&result));
    Ok(())
}

#[tokio::test]
async fn two_defects_in_one_section_are_two_entries_for_the_repair() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        replace_in_listing(
            &format!("{FIRST_USE} {SECOND_USE}"),
            "Name both tools before listing and sorting names.",
        ),
        reviewed_of(&[1], &[]),
    ]);
    let model = Routed::new(
        &scripted,
        vec![uses_of(&[
            (FIRST_USE, "the first tool"),
            (SECOND_USE, "the second tool"),
        ])],
    );
    let listing = format!("{FIRST_USE} {SECOND_USE}");
    let candidate = lesson([
        "A terminal takes typed commands.",
        listing.as_str(),
        "Read the names.",
    ]);
    run(&model, &candidate, &context(&[], &[]), &[]).await?;
    let prompts = scripted.prompts.lock().unwrap();
    let repair = prompts
        .iter()
        .find(|prompt| prompt.contains("issuesToFix"))
        .unwrap();
    let key = "\"issuesToFix\":";
    let start = repair.find(key).unwrap() + key.len();
    let issues: Value = serde_json::Deserializer::from_str(&repair[start..])
        .into_iter::<Value>()
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(issues.as_array().unwrap().len(), 2, "{issues}");
    Ok(())
}

#[tokio::test]
async fn one_fixed_defect_does_not_charge_the_other_in_its_section() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        replace_in_listing(FIRST_USE, "Name the first tool before listing names."),
        reviewed_of(&[1], &[]),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let model = Routed::new(
        &scripted,
        vec![
            uses_of(&[
                (FIRST_USE, "the first tool"),
                (SECOND_USE, "the second tool"),
            ]),
            uses_of(&[(SECOND_USE, "the second tool")]),
            uses_of(&[(SECOND_USE, "the second tool")]),
            uses_of(&[(SECOND_USE, "the second tool")]),
        ],
    );
    let listing = format!("{FIRST_USE} {SECOND_USE}");
    let candidate = lesson([
        "A terminal takes typed commands.",
        listing.as_str(),
        "Read the names.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert_eq!(repairs_asked(&scripted), 3);
    assert_eq!(result.set_aside.len(), 1, "{:?}", waiting(&result));
    let left = &result.set_aside[0];
    assert_eq!(left.repairs, 2);
    assert!(
        !left.finding.note.contains("first tool"),
        "{}",
        left.finding.note
    );
    Ok(())
}

/// A repair of the second section's body for `finding`.
fn replace_for(finding: &ReviewFinding, before: &str, after: &str) -> String {
    edits(json!([{
        "finding": finding.id().as_str(),
        "path": "/blocks/1/body",
        "before": before,
        "after": after
    }]))
}

#[tokio::test]
async fn a_defect_in_words_a_repair_wrote_continues_the_count_of_its_own_kind_of_defect(
) -> Result<()> {
    let (one, other) = (
        "Names never change when listed.",
        "Listing may change names.",
    );
    let first = "Sorting the list with the second tool leaves names unchanged.";
    let second = "Sorting the list with the third tool leaves names unchanged.";
    let third = "Sorting the list with the third tool always leaves names unchanged.";
    let mut digests = [subject_of(one), subject_of(other)];
    digests.sort();
    let clash = ReviewFinding::plan(
        PlanCheck::Contradiction,
        digests.join("-"),
        FindingSite::Section(0),
        "",
    );
    let use_of = |tool: &str| {
        ReviewFinding::plan(
            PlanCheck::Used,
            subject_of(tool),
            FindingSite::Section(1),
            "",
        )
    };
    let scripted = scripted(vec![
        reviewed(3),
        replace_for(&clash, other, first),
        reviewed_of(&[1], &[]),
        replace_for(&use_of("the second tool"), first, second),
        reviewed_of(&[1], &[]),
        replace_for(&use_of("the third tool"), second, third),
        reviewed_of(&[1], &[]),
    ]);
    let contradiction = answer(
        json!([]),
        json!([]),
        json!([{"quote": one, "otherQuote": other}]),
        json!([]),
    );
    let model = Routed::new(
        &scripted,
        vec![
            contradiction,
            uses_of(&[("the second tool", "the second tool")]),
            uses_of(&[("the third tool", "the third tool")]),
            uses_of(&[("the third tool", "the third tool")]),
        ],
    )
    .confirming(
        contract::confirm::CLASH_TASK,
        json!({"case":"Names cannot both never change and change when listed.","contradict":true}),
    );
    let candidate = lesson([
        "A terminal takes typed commands. Names never change when listed.",
        "Listing may change names. The list shows names in order.",
        "Read the names.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    // The use in the words the contradiction's repair wrote has its own
    // repairs; the use in the words that use's repair wrote continues its count.
    assert_eq!(repairs_asked(&scripted), 3);
    assert_eq!(result.set_aside.len(), 1, "{:?}", waiting(&result));
    assert_eq!(result.set_aside[0].repairs, 2);
    assert_eq!(
        result.set_aside[0].finding.passage.as_deref(),
        Some("the third tool")
    );
    assert_eq!(
        body(&result, 1),
        format!("{third} The list shows names in order.")
    );
    Ok(())
}

#[tokio::test]
async fn words_that_point_at_what_the_learner_never_sees_are_a_finding() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(2),
        reviewed_of(&[2], &[]),
        retitle_again_at(2),
        reviewed_of(&[2], &[]),
    ]);
    let pointing = answer(
        json!([]),
        json!([]),
        json!([]),
        json!([{"quote":"which the excerpt does not describe","what":"an excerpt the learner is never shown","neverShown":true}]),
    );
    let model = Routed::new(
        &scripted,
        vec![pointing.clone(), pointing.clone(), pointing],
    );
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "The sort order is one which the excerpt does not describe.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert_eq!(
        waiting(&result),
        [(
            "teaching:section-2".to_owned(),
            Some("which the excerpt does not describe".to_owned())
        )]
    );
    Ok(())
}

#[tokio::test]
async fn an_outline_review_never_checks_against_a_course_plan() -> Result<()> {
    let scripted = scripted(vec![json!({"issues":[]}).to_string()]);
    let model = Routed::new(&scripted, vec![]);
    let outline = json!({"modules":[{"title":"A module","lessons":[]}]});
    let schema = json!({"type":"object","properties":{"modules":{"type":"array"}}});
    let progress = crate::outline_progress::OutlineProgress::default();
    let sources = vec![crate::dto::LearningSourceDto {
        id: "saved-reference".into(),
        title: "Example reference".into(),
        url: None,
        acquired_at: 0,
        excerpt: EXCERPT.into(),
    }];
    review_and_repair_with_progress(
        &model,
        "Author a curriculum outline",
        &context(&[], &[OPEN_TERMINAL]),
        &schema,
        outline.to_string(),
        4000,
        OutlineReviewContext {
            progress: &progress,
            sources: &sources,
        },
    )
    .await?;
    assert!(model.asked().is_empty());
    Ok(())
}

fn defect_in_first_section() -> String {
    let checks = (0..3)
        .map(|index| json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":"The first section promises that the tutor certifies the learner's work as expert reviewed.","hasDefect":index == 0,"concerns":if index == 0 {vec!["promise"]} else {vec![]}}))
        .collect();
    json!({"issues":[],"blockChecks":crate::lessons::teaching_review::fixture_checks(checks)})
        .to_string()
}

fn actionable() -> String {
    json!({"decisions":[{"issueId":"issue-0","verdict":"actionable","basis":"internal_conflict","evidenceIds":["blocks-0"],"partId":"blocks-0","requirementId":null,"reason":"The section promises expert certification."}]}).to_string()
}

const LEARNER_SEES_ONLY_THE_LESSON: &str = "never mentions the course plan";

#[tokio::test]
async fn the_span_edit_repair_is_told_the_learner_sees_only_the_lesson() -> Result<()> {
    let scripted = scripted(vec![
        defect_in_first_section(),
        actionable(),
        retitle(),
        reviewed_of(&[0], &[verdict("reviewer/1", "resolved")]),
    ]);
    let model = Routed::new(&scripted, vec![]);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    run(&model, &candidate, &context(&[], &[]), &[]).await?;
    let prompts = scripted.prompts.lock().unwrap();
    let repair = prompts
        .iter()
        .find(|prompt| prompt.contains("issuesToFix"))
        .map(String::as_str);
    assert!(repair.is_some(), "no repair request was sent");
    assert!(repair.unwrap().contains(LEARNER_SEES_ONLY_THE_LESSON));
    Ok(())
}

#[tokio::test]
async fn the_whole_lesson_repair_is_told_not_to_mention_the_plan_or_the_excerpts() -> Result<()> {
    let mut broken = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    broken.as_object_mut().unwrap().remove("questions");
    let fixed = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    let scripted = scripted(vec![fixed.to_string(), reviewed(3)]);
    let model = Routed::new(&scripted, vec![]);
    let schema = json!({"type":"object","required":["blocks","questions"]});
    run_under(&model, &broken, &context(&[], &[]), &[], &schema).await?;
    let prompts = scripted.prompts.lock().unwrap();
    let repair = prompts
        .iter()
        .find(|prompt| prompt.contains("Repair the teaching material"))
        .map(String::as_str);
    assert!(repair.is_some(), "no whole-lesson repair request was sent");
    assert!(repair
        .unwrap()
        .contains("the course plan or the sources' excerpts"));
    Ok(())
}

#[tokio::test]
async fn the_check_covers_the_steps_a_deliverable_needs_including_saving_the_work() -> Result<()> {
    let scripted = scripted(vec![reviewed(3)]);
    let model = Routed::new(&scripted, vec![]);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    run(&model, &candidate, &context(&[], &[]), &[]).await?;
    let drafting = model.drafting();
    assert_eq!(drafting.len(), 1);
    assert!(
        drafting[0].contains("how the learner records or saves the work"),
        "{}",
        drafting[0]
    );
    Ok(())
}

#[tokio::test]
async fn the_request_carries_the_modules_title_and_milestone() -> Result<()> {
    let scripted = scripted(vec![reviewed(3)]);
    let model = Routed::new(&scripted, vec![]);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    run(&model, &candidate, &context(&[], &[]), &[]).await?;
    let asked = model.asked();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains("Shell basics"), "{}", asked[0]);
    assert!(
        asked[0].contains("A logbook of daily shell habits"),
        "{}",
        asked[0]
    );
    assert!(asked[0].contains("Making files"), "{}", asked[0]);
    assert!(asked[0].contains("Secure remote access"), "{}", asked[0]);
    assert!(
        asked[0].contains("A tidy list of saved connections"),
        "{}",
        asked[0]
    );
    assert!(
        asked[0].contains("Connect to another machine"),
        "{}",
        asked[0]
    );
    assert!(asked[0].contains("Keys and logins"), "{}", asked[0]);
    Ok(())
}

#[tokio::test]
async fn the_check_is_told_what_later_lessons_teach() -> Result<()> {
    let scripted = scripted(vec![reviewed(3)]);
    let model = Routed::new(&scripted, vec![]);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    run(&model, &candidate, &context(&[], &[]), &[]).await?;
    let asked = model.asked();
    assert_eq!(asked.len(), 1);
    let prompt: Value = serde_json::from_str(&asked[0]).unwrap();
    let later = prompt.get("laterLessons").and_then(Value::as_array);
    assert_eq!(later.map(Vec::len), Some(1), "{}", asked[0]);
    let entry = later.and_then(|later| later.first()).unwrap();
    assert_eq!(entry["title"], "Making files");
    assert_eq!(entry["actions"], json!(["Creating a file"]));
    assert!(
        !prompt["earlierLessons"]
            .to_string()
            .contains("Making files"),
        "{}",
        asked[0]
    );
    Ok(())
}

#[test]
fn the_check_s_rule_lets_a_lesson_say_what_a_later_one_teaches() {
    assert!(contract::SYSTEM.contains("neither is saying what a later lesson will teach"));
}

#[test]
fn pointing_at_the_outline_or_an_earlier_lesson_is_not_unseen() {
    assert!(contract::SYSTEM
        .contains("Pointing at the course outline or at an earlier lesson is not unseen"));
}

#[test]
fn the_contract_check_looks_for_unseen_words_in_questions_too() {
    assert!(contract::SYSTEM.contains(
        "words in any part, a question's choices, rationales and explanation included, that point the learner at something they never see"
    ));
}

#[test]
fn the_unseen_check_names_the_whole_outline_and_scopes_its_qualifier() {
    assert!(contract::SYSTEM.contains("courseOutline"));
    assert!(contract::SYSTEM
        .contains("text above or below or a step number that this lesson does not contain"));
}

const SOFTWARE_WORDS: [&str; 7] = [
    "command",
    "option",
    "key combination",
    "file operation",
    "folder",
    "a file",
    "path",
];

fn assert_no_software_words(what: &str, text: &str) {
    for word in SOFTWARE_WORDS {
        assert!(!text.contains(word), "{what} mentions \"{word}\": {text}");
    }
}

async fn finding_note(planned: Value, used: Value, marker: &str) -> Result<String> {
    finding_note_in(context(&[], &[OPEN_TERMINAL]), planned, used, marker).await
}

async fn finding_note_in(
    context_json: String,
    planned: Value,
    used: Value,
    marker: &str,
) -> Result<String> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
        retitle_third_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let found = answer(planned, used, json!([]), json!([]));
    let model = Routed::new(
        &scripted,
        vec![found.clone(), found.clone(), found.clone(), found],
    );
    let candidate = lesson([
        "The first step is shown.",
        "Apply the second method\nto the first item.",
        "Read the result.",
    ]);
    let result = run(&model, &candidate, &context_json, &[]).await?;
    let note = result
        .set_aside
        .iter()
        .map(|waiting| waiting.finding.note.clone())
        .find(|note| note.contains(marker))
        .unwrap_or_default();
    Ok(note
        .split_once(marker)
        .map_or(note.clone(), |(_, rest)| format!("{marker}{rest}")))
}

#[tokio::test]
async fn the_contract_check_describes_learner_actions_in_no_subjects_terms() -> Result<()> {
    let system = contract::SYSTEM;
    let start = system.find("1. planned:").unwrap();
    let end = system.find("3. contradictions:").unwrap();
    let checks = &system[start..end];
    assert_no_software_words("checks 1 and 2", checks);
    for expected in [
        "the exact steps, and the result the learner should get",
        "anything a part asks the learner to do or use that they need to be shown how to do (an action, a technique, a tool, a setting, or a variant of something taught)",
        "A variant is a new thing: teaching how to do something does not teach doing it another way or with something added.",
    ] {
        assert!(checks.contains(expected), "missing {expected:?} in {checks}");
    }

    let scripted = scripted(vec![reviewed(3)]);
    let model = Routed::new(&scripted, vec![]);
    let candidate = lesson(["One.", "Two.", "Three."]);
    run(&model, &candidate, &context(&[], &[]), &[]).await?;
    let drafting = model.drafting();
    let start = drafting[0].find("Go through the lesson part by part in order.");
    let end = drafting[0].find("say where the lesson, the earlier lessons");
    assert!(start.is_some() && end.is_some(), "{}", drafting[0]);
    let brief = &drafting[0][start.unwrap()..end.unwrap()];
    assert_no_software_words("the brief", brief);
    assert!(
        brief.contains("each variant of it"),
        "the brief never asks about variants: {brief}"
    );

    let action = finding_note(
        json!([not_taught("action-0")]),
        json!([]),
        "Teach it in the lesson's own words: ",
    )
    .await?;
    assert!(
        action.contains("the exact steps, and the result the learner should get"),
        "{action}"
    );
    assert_no_software_words("the action note", &action);

    let used_note = finding_note(
        json!([]),
        json!([used("Apply the second method", "the second method")]),
        "Teach it in this step",
    )
    .await?;
    assert!(used_note.contains("exactly how to do it"), "{used_note}");
    assert_no_software_words("the used-before-taught note", &used_note);
    Ok(())
}

#[tokio::test]
async fn an_untaught_idea_or_contrast_is_told_its_name_is_the_plans_label_and_a_term_is_not(
) -> Result<()> {
    let label = "is a short label for it";
    let idea = finding_note_in(
        context_planning(&["A first idea"], &[], &[], ""),
        json!([not_taught("idea-0")]),
        json!([]),
        "The lesson does not yet teach the idea",
    )
    .await?;
    let contrast = finding_note_in(
        context_planning(&[], &[], &[], "A planned confusion"),
        json!([not_taught("contrast")]),
        json!([]),
        "The lesson does not yet teach the contrast",
    )
    .await?;
    for note in [&idea, &contrast] {
        assert!(!note.is_empty());
        assert!(!note.contains("course plan"), "{note}");
        assert!(note.contains(label), "{note}");
        assert!(note.contains("without copying the label"), "{note}");
    }
    let term = finding_note_in(
        context_planning(&[], &["a planned term"], &[], ""),
        json!([not_taught("term-0")]),
        json!([]),
        "The lesson does not yet teach the term",
    )
    .await?;
    assert!(!term.is_empty());
    assert!(term.contains("say what it means"), "{term}");
    assert!(!term.contains(label), "{term}");
    assert!(!term.contains("copying the label"), "{term}");
    Ok(())
}

const SHOWN: &str = "what it is for, exactly how to do it, and the result the learner should get";

#[tokio::test]
async fn check_two_counts_teaching_where_a_step_asks_for_it() -> Result<()> {
    let system = contract::SYSTEM;
    let start = system.find("2. usedBeforeTaught").unwrap();
    let end = system.find("3. contradictions:").unwrap();
    let check = &system[start..end];
    let criterion = format!(
        "or that it treats as known, which the lesson has not taught by the words that ask for it and which priorKnowledge and the actions and terms of earlierLessons do not cover. The lesson has taught something the learner does or uses when an earlier part, or this part before or in those words, says {SHOWN}: words that ask for it and say all of this teach it where it is used."
    );
    assert!(check.contains(&criterion), "{check}");
    assert!(
        !system.contains("which no earlier part of this lesson teaches"),
        "{check}"
    );

    let scripted = scripted(vec![reviewed(3)]);
    let model = Routed::new(&scripted, vec![]);
    let candidate = lesson(["One.", "Two.", "Three."]);
    run(&model, &candidate, &context(&[], &[]), &[]).await?;
    let drafting = model.drafting();
    let brief = format!(
        "an earlier part, or the same part up to and including the words that ask for it, and whether that place says {SHOWN}"
    );
    assert!(drafting[0].contains(&brief), "{}", drafting[0]);
    Ok(())
}

#[test]
fn check_two_asks_for_a_noun_phrase_never_a_sentence() {
    let system = contract::SYSTEM;
    assert!(
        system.contains(
            "Give one entry for each such thing, however many parts use it: in what, its name, a noun phrase, never a sentence; in uses, the words copied exactly of every place that asks the learner to use it before the lesson has taught it; in taughtLater, the words copied exactly with which the lesson first teaches it after those places, empty when it never does."
        ),
        "{system}"
    );
    assert!(!system.contains("and what it is."), "{system}");
}

#[tokio::test]
async fn the_check_and_the_note_define_shown_how_in_the_same_words() -> Result<()> {
    let note = finding_note(
        json!([]),
        json!([used("Apply the second method", "the second method")]),
        "Teach it in this step",
    )
    .await?;
    let (_, rest) = note.split_once("where it is first used (").unwrap();
    let (phrase, _) = rest.split_once(')').unwrap();
    assert_eq!(phrase, EACH_SHOWN, "{note}");
    assert!(contract::SYSTEM.contains(SHOWN), "{}", contract::SYSTEM);
    Ok(())
}

const EACH_SHOWN: &str = "for something the learner does or uses, what it is for, exactly how to do it, and the result the learner should get; for a term, what it means";

#[test]
fn the_check_says_when_a_term_has_been_taught() {
    let system = contract::SYSTEM;
    assert!(
        system.contains("It has taught a term when an earlier part, or this part before or in those words, says what it means."),
        "{system}"
    );
    assert!(
        system.contains(&format!(
            "says {SHOWN}: words that ask for it and say all of this teach it where it is used."
        )),
        "{system}"
    );
}

#[test]
fn the_confirming_read_says_what_a_part_only_mentions_is_not_used() {
    let system = contract::confirm::SYSTEM;
    assert!(
        system.contains("A part uses something when its words ask the learner to do or use it, when its practice deliverable needs it, or when it treats it as known. What a part only mentions, and does not ask of the learner, it does not use; then asked is false and quote is empty."),
        "{system}"
    );
    assert!(system.contains("It has taught a term when"), "{system}");
}

#[tokio::test]
async fn the_used_note_names_what_teaches_an_action_and_what_teaches_a_term() -> Result<()> {
    let note = finding_note(
        json!([]),
        json!([used("Apply the second method", "the second method")]),
        "Teach it in this step",
    )
    .await?;
    assert!(note.contains(EACH_SHOWN), "{note}");
    assert_no_software_words("the used-before-taught note", &note);
    Ok(())
}

const TYPED: &str = "A terminal takes typed commands.";

#[tokio::test]
async fn an_item_is_taught_wherever_in_the_lesson_its_words_are() -> Result<()> {
    let scripted = scripted(vec![reviewed(3), retitle(), reviewed(3)]);
    let reported = answer(
        json!([taught("idea-0", TYPED)]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![reported.clone(), reported]);
    let candidate = lesson([TYPED, "The list shows names.", "Read the names."]);
    let ctx = context(&["Typing commands"], &[]);
    let result = run(&model, &candidate, &ctx, &[]).await?;
    assert!(result.set_aside.is_empty(), "{:?}", waiting(&result));
    assert!(model.confirmed().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_use_is_a_finding_at_the_part_that_holds_its_words() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let found = answer(
        json!([]),
        json!([used("Type ls -l to see sizes", "the -l option of ls")]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![found.clone(), found.clone(), found]);
    let candidate = lesson([
        TYPED,
        "Type ls -l to see sizes in the list.",
        "Read the names.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert_eq!(
        waiting(&result),
        [(
            "teaching:section-1".to_owned(),
            Some("Type ls -l to see sizes".to_owned())
        )]
    );
    Ok(())
}

#[tokio::test]
async fn a_use_the_confirming_read_finds_taught_raises_nothing_and_asks_for_no_repair() -> Result<()>
{
    let scripted = scripted(vec![reviewed(3), retitle(), reviewed(3)]);
    let found = answer(
        json!([]),
        json!([used(
            "open the terminal application",
            "the terminal application"
        )]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![found.clone(), found]).confirming(
        "the terminal application",
        json!({"asked":true,"taught":true,"where":"thisLesson","quote":"Click the terminal icon in the dock"}),
    );
    let candidate = lesson([
        TYPED,
        "Click the terminal icon in the dock, then open the terminal application.",
        "Read the names.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert!(result.set_aside.is_empty(), "{:?}", waiting(&result));
    assert_eq!(model.confirmed().len(), 1);
    assert!(!scripted
        .prompts
        .lock()
        .unwrap()
        .iter()
        .any(|prompt| prompt.contains("issuesToFix")));
    Ok(())
}

/// A second sentence of the opening section, where the second check finds a use.
const LISTED: &str = "Look at the list of names.";

#[tokio::test]
async fn an_item_found_taught_is_not_asked_again_until_its_sentence_changes() -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        edits(
            json!([{"path":"/blocks/1/body","before":"Type ls -l to see sizes.","after":"Type ls to list names."}]),
        ),
        reviewed_of(&[1], &[]),
        edits(
            json!([{"path":"/blocks/0/body","before":TYPED,"after":"A terminal runs typed commands."}]),
        ),
        reviewed_of(&[0], &[]),
    ]);
    let first = answer(
        json!([taught("idea-0", TYPED)]),
        json!([used("Type ls -l to see sizes.", "the -l option of ls")]),
        json!([]),
        json!([]),
    );
    let second = answer(
        json!([not_taught("idea-0")]),
        json!([used(LISTED, "reading names")]),
        json!([]),
        json!([]),
    );
    let third = answer(
        json!([taught("idea-0", "A terminal runs typed commands.")]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![first, second, third]).confirming(
        "Typing commands",
        json!({"taught":false,"quote":"","belongs":"section-0"}),
    );
    let opening = format!("{TYPED} {LISTED}");
    let candidate = lesson([
        opening.as_str(),
        "Type ls -l to see sizes.",
        "Read the names.",
    ]);
    let ctx = context(&["Typing commands"], &[]);
    let result = run(&model, &candidate, &ctx, &[]).await?;
    let asked = model.asked();
    assert_eq!(asked.len(), 3);
    assert!(asked[0].contains("Typing commands"), "{}", asked[0]);
    assert!(!asked[1].contains("Typing commands"), "{}", asked[1]);
    assert!(asked[2].contains("Typing commands"), "{}", asked[2]);
    assert!(result.set_aside.is_empty(), "{:?}", waiting(&result));
    Ok(())
}

#[tokio::test]
async fn a_confirming_read_that_finds_a_planned_items_words_settles_it() -> Result<()> {
    let scripted = scripted(vec![reviewed(3), retitle(), reviewed(3)]);
    let missing = answer(
        json!([not_taught("action-0")]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![missing.clone(), missing]).confirming(
        OPEN_TERMINAL,
        json!({"taught":true,"quote":"Press the terminal icon in the dock","belongs":"section-1"}),
    );
    let candidate = lesson([
        TYPED,
        "Press the terminal icon in the dock to open it.",
        "Read the names.",
    ]);
    let ctx = context(&[], &[OPEN_TERMINAL]);
    let result = run(&model, &candidate, &ctx, &[]).await?;
    assert!(result.set_aside.is_empty(), "{:?}", waiting(&result));
    Ok(())
}

#[tokio::test]
async fn a_planned_item_the_confirming_read_does_not_find_is_a_finding_where_it_belongs(
) -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(2),
        reviewed_of(&[2], &[]),
        retitle_again_at(2),
        reviewed_of(&[2], &[]),
    ]);
    let missing = answer(
        json!([not_taught("idea-0")]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![missing.clone(), missing.clone(), missing]).confirming(
        "Typing commands",
        json!({"taught":false,"quote":"","belongs":"section-2"}),
    );
    let candidate = lesson([TYPED, "The list shows names.", "Read the names."]);
    let ctx = context(&["Typing commands"], &[]);
    let result = run(&model, &candidate, &ctx, &[]).await?;
    assert_eq!(waiting(&result), [("teaching:section-2".to_owned(), None)]);
    Ok(())
}

async fn waiting_after_planned_contrast(
    reply: Value,
) -> Result<(Vec<(String, Option<String>)>, Vec<Value>)> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let missing = answer(
        json!([not_taught("contrast")]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![missing.clone(), missing.clone(), missing])
        .confirming("A planned confusion", reply);
    let candidate = json!({"blocks":[
        block_of("explanation", "Opening", TYPED),
        block_of("contrast", "Contrast", "The list shows names."),
        block_of("explanation", "Reading", "Read the names.")
    ],"questions":[]});
    let ctx = context_planning(&[], &[], &[], "A planned confusion");
    let result = run(&model, &candidate, &ctx, &[]).await?;
    Ok((waiting(&result), model.confirmed_schemas()))
}

#[tokio::test]
async fn a_planned_contrast_is_placed_only_in_a_contrast_section() -> Result<()> {
    let (found, schemas) =
        waiting_after_planned_contrast(json!({"taught":false,"quote":"","belongs":"section-1"}))
            .await?;
    assert_eq!(found, [("teaching:section-1".to_owned(), None)]);
    assert!(!schemas.is_empty());
    for schema in &schemas {
        assert_eq!(
            schema["properties"]["belongs"]["enum"],
            json!(["section-1"])
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_planned_contrast_the_confirming_read_puts_in_an_explanation_is_not_placed_there(
) -> Result<()> {
    let (found, _) =
        waiting_after_planned_contrast(json!({"taught":false,"quote":"","belongs":"section-0"}))
            .await?;
    assert_eq!(found, [("teaching:lesson".to_owned(), None)]);
    Ok(())
}

/// The section a use is confirmed in: words before the use, the use, words after.
const USED_IN: &str = "Use the list command. Type ls -l to see sizes. Then read the list.";

/// What waits after the check finds a use and `reply` answers the
/// confirming read of it.
async fn waiting_after_confirming(reply: Value) -> Result<Vec<(String, Option<String>)>> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let found = answer(
        json!([]),
        json!([used("Type ls -l to see sizes", "the -l option of ls")]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![found.clone(), found.clone(), found])
        .confirming("the -l option of ls", reply);
    let candidate = lesson([TYPED, USED_IN, "Read the names."]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    Ok(waiting(&result))
}

fn confirmed_by(place: &str, quote: &str) -> Value {
    json!({"asked":true,"taught":true,"where":place,"quote":quote})
}

#[tokio::test]
async fn a_confirming_read_settles_a_use_when_its_words_check_out() -> Result<()> {
    for (place, quote) in [
        ("thisLesson", TYPED),
        ("thisLesson", "Use the list command."),
        ("thisLesson", "Type ls -l to see sizes"),
        ("earlierLessons", "Listing the files in a folder"),
        ("priorKnowledge", "used a graphical file manager"),
    ] {
        let left = waiting_after_confirming(confirmed_by(place, quote)).await?;
        assert!(left.is_empty(), "{place} {quote}: {left:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_confirming_read_that_does_not_check_out_leaves_the_finding_standing() -> Result<()> {
    let standing = vec![(
        "teaching:section-1".to_owned(),
        Some("Type ls -l to see sizes".to_owned()),
    )];
    for (place, quote) in [
        ("thisLesson", "Then read the list."),
        ("thisLesson", "Read the names."),
        ("thisLesson", "Words that are nowhere in the lesson"),
        ("earlierLessons", "Creating a file"),
        ("priorKnowledge", "used a graphical file editor"),
    ] {
        let left = waiting_after_confirming(confirmed_by(place, quote)).await?;
        assert_eq!(left, standing, "{place} {quote}");
    }
    Ok(())
}

#[tokio::test]
async fn a_use_the_part_never_asked_for_raises_no_finding() -> Result<()> {
    let left = waiting_after_confirming(
        json!({"asked":false,"taught":false,"where":"thisLesson","quote":""}),
    )
    .await?;
    assert!(left.is_empty(), "{left:?}");
    Ok(())
}

#[tokio::test]
async fn a_use_the_part_asked_for_and_nothing_taught_stands() -> Result<()> {
    let left = waiting_after_confirming(
        json!({"asked":true,"taught":false,"where":"thisLesson","quote":""}),
    )
    .await?;
    assert_eq!(
        left,
        [(
            "teaching:section-1".to_owned(),
            Some("Type ls -l to see sizes".to_owned())
        )]
    );
    Ok(())
}

#[tokio::test]
async fn the_question_put_for_a_use_asks_whether_the_part_asks_it_and_names_both_standards(
) -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let found = answer(
        json!([]),
        json!([used("Type ls -l to see sizes", "the -l option of ls")]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![found.clone(), found.clone(), found]);
    let candidate = lesson([TYPED, USED_IN, "Read the names."]);
    run(&model, &candidate, &context(&[], &[]), &[]).await?;
    let confirmed = model.confirmed();
    let prompt = confirmed.first().unwrap();
    assert!(
        prompt.contains("ask the learner to do or use the -l option of ls, need it for its practice deliverable, or treat it as known? If it does, has the lesson"),
        "{prompt}"
    );
    assert!(
        prompt.contains(&format!("taught it: {EACH_SHOWN}?")),
        "{prompt}"
    );
    Ok(())
}

#[tokio::test]
async fn every_negative_is_read_again_once_and_a_use_is_shown_only_the_parts_up_to_it() -> Result<()>
{
    let scripted = scripted(vec![
        reviewed(3),
        retitle_at(1),
        reviewed_of(&[1], &[]),
        retitle_again_at(1),
        reviewed_of(&[1], &[]),
        retitle_third_at(1),
        reviewed_of(&[1], &[]),
    ]);
    let found = answer(
        json!([not_taught("idea-0"), not_taught("idea-1")]),
        json!([used("Type ls -l to see sizes", "the -l option of ls")]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(
        &scripted,
        vec![found.clone(), found.clone(), found.clone(), found],
    );
    let candidate = lesson([TYPED, "Type ls -l to see sizes.", "Read the names."]);
    let ctx = context(&["A first idea", "A second idea"], &[]);
    run(&model, &candidate, &ctx, &[]).await?;
    let confirmed = model.confirmed();
    assert!(confirmed.len() >= 3, "{confirmed:?}");
    let first_pass = &confirmed[..3];
    for needle in ["A first idea", "A second idea", "the -l option of ls"] {
        let asking = first_pass
            .iter()
            .filter(|prompt| prompt.contains(needle))
            .count();
        assert_eq!(asking, 1, "{needle}: {first_pass:?}");
    }
    let about_the_use = first_pass
        .iter()
        .find(|prompt| prompt.contains("the -l option of ls"))
        .unwrap();
    assert!(about_the_use.contains("section-1"), "{about_the_use}");
    assert!(!about_the_use.contains("section-2"), "{about_the_use}");
    assert_eq!(confirmed.len(), 3 * model.asked().len());
    Ok(())
}

#[tokio::test]
async fn words_pointing_at_what_is_never_seen_are_a_finding_in_every_part_that_holds_them(
) -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        retitle_both("", ", revised"),
        reviewed_of(&[1, 2], &[]),
        retitle_both(", revised", ", revised again"),
        reviewed_of(&[1, 2], &[]),
    ]);
    let pointing = answer(
        json!([]),
        json!([]),
        json!([]),
        json!([{"quote":"see the excerpt above","what":"an excerpt the learner is never shown","neverShown":true}]),
    );
    let model = Routed::new(
        &scripted,
        vec![pointing.clone(), pointing.clone(), pointing],
    );
    let candidate = lesson([
        TYPED,
        "For sizes, see the excerpt above.",
        "For names, see the excerpt above.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert_eq!(
        waiting(&result),
        [
            (
                "teaching:section-1".to_owned(),
                Some("see the excerpt above".to_owned())
            ),
            (
                "teaching:section-2".to_owned(),
                Some("see the excerpt above".to_owned())
            )
        ]
    );
    Ok(())
}

#[tokio::test]
async fn an_unseen_entry_the_check_says_the_learner_sees_is_no_finding() -> Result<()> {
    let scripted = scripted(vec![reviewed(3), reviewed(3), reviewed(3)]);
    let dismissed = answer(
        json!([]),
        json!([]),
        json!([]),
        json!([{"quote":"see the excerpt above","what":"the steps below, which the lesson shows","neverShown":false}]),
    );
    let model = Routed::new(
        &scripted,
        vec![dismissed.clone(), dismissed.clone(), dismissed],
    );
    let candidate = lesson([
        TYPED,
        "For sizes, see the excerpt above.",
        "For names, see the excerpt above.",
    ]);
    let result = run(&model, &candidate, &context(&[], &[]), &[]).await?;
    assert_eq!(waiting(&result), []);
    Ok(())
}

#[test]
fn the_check_and_the_confirming_read_define_taught_in_the_same_words() {
    let definition = "An action is taught when the lesson shows the learner how to perform it: the exact steps, and the result the learner should get.";
    for system in [contract::SYSTEM, contract::confirm::SYSTEM] {
        assert!(system.contains(definition), "{system}");
        assert!(system.contains(SHOWN), "{system}");
        assert!(system.contains("A variant is a new thing"), "{system}");
    }
}

#[cfg(test)]
#[path = "contract_used_once_tests.rs"]
mod used_once;
