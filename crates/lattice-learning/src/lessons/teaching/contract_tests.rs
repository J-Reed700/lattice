//! A lesson's review holds it to its course plan: what the plan gives it must
//! be taught, nothing is used before it is taught, and it neither contradicts
//! itself nor points at what the learner cannot see.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::fixture_completion;
use crate::lessons::content_verification::Protected;
use crate::lessons::course_generation_tests::{ScriptedModel, CONTRACT_NOTHING};
use crate::reference_collection::ReferenceCollection;
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
}

impl<'a> Routed<'a> {
    fn new(inner: &'a ScriptedModel, answers: Vec<String>) -> Self {
        Self {
            inner,
            contract: Mutex::new(answers.into()),
            asked: Mutex::new(Vec::new()),
            drafting: Mutex::new(Vec::new()),
        }
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

fn scripted(outputs: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(outputs.into()),
        prompts: Mutex::new(Vec::new()),
    }
}

/// The writer's prompt: the plan's lesson 2 is this one.
fn context(ideas: &[&str], actions: &[&str]) -> String {
    json!({
        "sources":[{"sourceIndex":0,"excerpt":EXCERPT}],
        "priorKnowledge":"The learner has used a graphical file manager.",
        "currentModule":{"title":"Shell basics","lessonSequence":[{"title":"Looking around","current":false},{"title":"Working in the terminal","current":true},{"title":"Making files","current":false}]},
        "courseSequence":{"curriculum":[{"title":"Shell basics","outcomes":["Run commands"],"milestone":"A logbook of daily shell habits","lessons":[{"title":"Looking around"},{"title":"Working in the terminal"},{"title":"Making files"}]},{"title":"Secure remote access","outcomes":["Connect to another machine"],"milestone":"A tidy list of saved connections","lessons":[{"title":"Keys and logins"}]}],"coursePlan":[
            {"title":"Looking around","position":"earlier","ideas":["A folder holds files"],"terms":["folder"],"actions":["Listing the files in a folder"],"contrast":""},
            {"title":"Working in the terminal","position":"this","ideas":ideas,"terms":[],"actions":actions,"contrast":""},
            {"title":"Making files","position":"later","ideas":[],"terms":[],"actions":["Creating a file"],"contrast":""}
        ]}
    })
    .to_string()
}

fn block(title: &str, body: &str) -> Value {
    json!({"kind":"explanation","title":title,"body":body,"rubric":[],"sourceIndex":0,"quote":EXCERPT})
}

fn lesson(bodies: [&str; 3]) -> Value {
    json!({"blocks":[
        block("Opening", bodies[0]),
        block("Listing", bodies[1]),
        block("Reading", bodies[2])
    ],"questions":[]})
}

fn reviewed(blocks: usize) -> String {
    let checks = (0..blocks)
        .map(|index| json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":"No defect was found in this section.","hasDefect":false}))
        .collect();
    json!({"issues":[],"blockChecks":crate::lessons::teaching_review::fixture_checks(checks)})
        .to_string()
}

fn edits(edits: Value) -> String {
    json!({ "edits": edits }).to_string()
}

fn retitle() -> String {
    edits(json!([{"path":"/blocks/0/title","before":"Opening","after":"Opening, revised"}]))
}

fn answer(planned: Value, used: Value, contradictions: Value, unseen: Value) -> String {
    json!({"planned":planned,"usedBeforeTaught":used,"contradictions":contradictions,"unseen":unseen})
        .to_string()
}

fn used(section: usize, quote: &str, what: &str) -> Value {
    json!({"sectionId":format!("section-{section}"),"quote":quote,"what":what})
}

fn not_taught(item: &str, section: usize) -> Value {
    json!({"item":item,"taught":false,"sectionId":format!("section-{section}"),"quote":""})
}

fn taught(item: &str, section: usize, quote: &str) -> Value {
    json!({"item":item,"taught":true,"sectionId":format!("section-{section}"),"quote":quote})
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
        schema,
        candidate.to_string(),
        4000,
        ReviewContext {
            outline: None,
            protected,
            lesson_references: Some(&references),
            held: BTreeSet::new(),
        },
    )
    .await
}

fn waiting(reviewed: &Reviewed) -> Vec<(String, Option<String>)> {
    let mut found: Vec<_> = reviewed
        .set_aside
        .iter()
        .map(|waiting| (waiting.finding.key().id(), waiting.finding.passage.clone()))
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
    let scripted = scripted(vec![reviewed(3), retitle(), reviewed(3)]);
    let missing = answer(
        json!([not_taught("action-0", 1)]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![missing.clone(), missing]);
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
    assert_eq!(asked.len(), 2);
    for expected in [
        "action-0",
        OPEN_TERMINAL,
        "Listing the files in a folder",
        "The learner has used a graphical file manager.",
        "section-2",
    ] {
        assert!(asked[0].contains(expected), "{expected}");
    }
    assert!(!asked[0].contains("Creating a file"));
    Ok(())
}

#[tokio::test]
async fn a_use_before_it_is_taught_is_a_finding_only_when_its_words_are_in_the_section(
) -> Result<()> {
    let scripted = scripted(vec![reviewed(3), retitle(), reviewed(3)]);
    let found = answer(
        json!([]),
        json!([
            used(1, "Type ls -l to see sizes", "the -l option of ls"),
            used(2, "Type mv notes.txt old.txt", "the mv command")
        ]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![found.clone(), found]);
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
    Ok(())
}

#[tokio::test]
async fn a_planned_item_reported_taught_with_words_not_in_its_section_is_not_taught() -> Result<()>
{
    let scripted = scripted(vec![reviewed(3), retitle(), reviewed(3)]);
    let reported = answer(
        json!([
            taught("idea-0", 0, "A terminal takes typed commands."),
            taught("action-0", 1, "Press the terminal icon in the dock")
        ]),
        json!([]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![reported.clone(), reported]);
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
    let scripted = scripted(vec![reviewed(3), retitle(), reviewed(3)]);
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
    let scripted = scripted(vec![reviewed(3), retitle()]);
    let clash = answer(
        json!([]),
        json!([]),
        json!([{
            "sectionId":"section-0","quote":"Commands like ls take no folder name.",
            "otherSectionId":"section-2","otherQuote":"Type ls Documents to list that folder"
        }]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![clash]);
    let candidate = lesson([
        "Commands like ls take no folder name.",
        "The list shows names.",
        "Type ls Documents to list that folder.",
    ]);
    let protected = [Protected {
        unit: 0,
        field: "body".into(),
        quote: "Commands like ls take no folder name.".into(),
    }];
    let result = run(&model, &candidate, &context(&[], &[]), &protected).await?;
    assert_eq!(
        waiting(&result),
        [(
            "teaching:section-2".to_owned(),
            Some("Type ls Documents to list that folder".to_owned())
        )]
    );
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
async fn a_finding_in_a_section_with_protected_sentences_is_repaired_when_it_touches_none(
) -> Result<()> {
    let scripted = scripted(vec![
        reviewed(3),
        edits(
            json!([{"path":"/blocks/1/body","before":"Type ls -l to see sizes.","after":"Type ls to list names."}]),
        ),
        reviewed(3),
    ]);
    let found = answer(
        json!([]),
        json!([used(1, "Type ls -l to see sizes.", "the -l option of ls")]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![found]);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "Run pwd to print the folder you are in. Type ls -l to see sizes.",
        "Read the names.",
    ]);
    let protected = [Protected {
        unit: 1,
        field: "body".into(),
        quote: "Run pwd to print the folder you are in.".into(),
    }];
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
        reviewed(3),
    ]);
    let missing = answer(
        json!([not_taught("action-0", 1)]),
        json!([]),
        json!([]),
        json!([]),
    );
    let now_taught = answer(
        json!([taught("action-0", 1, "press the Terminal icon in the dock")]),
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
    let protected = [Protected {
        unit: 1,
        field: "body".into(),
        quote: "Run pwd to print the folder you are in.".into(),
    }];
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
        reviewed(3),
        edits(
            json!([{"path":"/blocks/2/body","before":"Then rename a file with mv old.txt new.txt.","after":"Read the names."}]),
        ),
        reviewed(3),
    ]);
    let first = answer(
        json!([]),
        json!([used(0, "Type ls -l to see sizes.", "the -l option of ls")]),
        json!([]),
        json!([]),
    );
    let second = answer(
        json!([]),
        json!([used(
            2,
            "rename a file with mv old.txt new.txt",
            "the mv command"
        )]),
        json!([]),
        json!([]),
    );
    let model = Routed::new(&scripted, vec![first, second]);
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

#[tokio::test]
async fn words_that_point_at_what_the_learner_never_sees_are_a_finding() -> Result<()> {
    let scripted = scripted(vec![reviewed(3), retitle(), reviewed(3)]);
    let pointing = answer(
        json!([]),
        json!([]),
        json!([]),
        json!([{"sectionId":"section-2","quote":"which the excerpt does not describe","what":"an excerpt the learner is never shown"}]),
    );
    let model = Routed::new(&scripted, vec![pointing.clone(), pointing]);
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
    review_and_repair_with_progress(
        &model,
        "Author a curriculum outline",
        &context(&[], &[OPEN_TERMINAL]),
        &schema,
        outline.to_string(),
        4000,
        None,
    )
    .await?;
    assert!(model.asked().is_empty());
    Ok(())
}

fn defect_in_first_section() -> String {
    let checks = (0..3)
        .map(|index| json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":"The first section states a wrong result.","hasDefect":index == 0}))
        .collect();
    json!({"issues":[],"blockChecks":crate::lessons::teaching_review::fixture_checks(checks)})
        .to_string()
}

fn actionable() -> String {
    json!({"decisions":[{"issueId":"issue-0","verdict":"actionable","basis":"internal_conflict","evidenceIds":["blocks-0"],"partId":"blocks-0","requirementId":null,"reason":"The section states a wrong result."}]}).to_string()
}

const LEARNER_SEES_ONLY_THE_LESSON: &str = "never mentions the course plan";

#[tokio::test]
async fn the_span_edit_repair_is_told_the_learner_sees_only_the_lesson() -> Result<()> {
    let scripted = scripted(vec![
        defect_in_first_section(),
        actionable(),
        retitle(),
        reviewed(3),
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

#[test]
fn pointing_at_the_outline_or_an_earlier_lesson_is_not_unseen() {
    assert!(contract::SYSTEM
        .contains("Pointing at the course outline or at an earlier lesson is not unseen"));
}

#[test]
fn the_unseen_check_names_the_whole_outline_and_scopes_its_qualifier() {
    assert!(contract::SYSTEM.contains("courseOutline"));
    assert!(contract::SYSTEM
        .contains("text above or below or a step number that this lesson does not contain"));
}
