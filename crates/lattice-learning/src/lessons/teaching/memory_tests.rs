//! A teaching review is of the job's lesson, not of one exact draft: what it
//! judged stands while the inputs it judged are unchanged.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::loop_tests::{
    context, part_a_is_wrong, parts_lesson, references, retitled, ByRequest, EXCERPT, INPUT,
};
use super::*;
use crate::dto::LearningSourceDto;
use crate::lesson_drafts;
use crate::lessons::structured_call::{is_draft_request, task_messages};
use crate::reference_collection::ReferenceCollection;
use crate::review_findings::Held;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::shared::persistence::Database;
use std::sync::Mutex;

const FIRST_FACT: &str = "Part A states one plain fact about the tool.";
const SECOND_FACT: &str = "Part B states one plain fact about the tool.";

fn source(id: &str, excerpt: &str) -> LearningSourceDto {
    LearningSourceDto {
        id: id.into(),
        title: "Example reference".into(),
        url: None,
        acquired_at: 0,
        excerpt: excerpt.into(),
    }
}

/// `lesson` with one section's text changed, as a repair leaves it.
fn change_section(mut lesson: Value, index: usize) -> Value {
    let name = ["Part A", "Part B", "Part C"][index];
    lesson["blocks"][index]["body"] = json!(format!(
        "{name} states one plain fact about the tool. {name} then gives a better idea."
    ));
    lesson
}

async fn review(
    model: &dyn LLMPort,
    context: &str,
    candidate: &Value,
    sources: &[LearningSourceDto],
) -> Result<Reviewed> {
    lesson_drafts::save(&candidate.to_string()).await?;
    let references = ReferenceCollection::lexical(sources)?;
    review_material(
        model,
        LESSON_REPAIR_SYSTEM,
        context,
        ReviewSchema::Other(&json!({"type":"object"})),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(&references, sources, &[], Held::default()),
    )
    .await
}

/// Two reviews in one lesson job: the first of `first` under the references
/// `before`, then of what `then` makes of the first's result under `after`.
async fn two_reviews(
    model: &dyn LLMPort,
    context: &str,
    first: &Value,
    before: &[LearningSourceDto],
    then: &dyn Fn(Value) -> Value,
    after: &[LearningSourceDto],
) -> Result<(Reviewed, Reviewed)> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&pool).await?;
    lesson_drafts::run(
        &Database::from(pool.clone()),
        &repo,
        &job,
        &lesson_id,
        async {
            lesson_drafts::resume(INPUT.into()).await?;
            let one = review(model, context, first, before).await?;
            let next = then(serde_json::from_str::<Value>(&one.candidate)?);
            let two = review(model, context, &next, after).await?;
            Ok((one, two))
        },
    )
    .await
}

#[tokio::test]
async fn a_later_review_in_the_job_asks_only_about_the_section_that_changed() -> Result<()> {
    let model = ByRequest::new(|_| false, |_, _| Vec::new());
    let sources = references();
    let change = |lesson| change_section(lesson, 1);
    two_reviews(
        &model,
        &context(),
        &parts_lesson(3),
        &sources,
        &change,
        &sources,
    )
    .await?;
    let reviews = model.reviews();
    assert_eq!(reviews.len(), 2);
    assert_eq!(reviews[0]["sectionIndicesToReview"], json!([0, 1, 2]));
    assert_eq!(reviews[1]["sectionIndicesToReview"], json!([1]));
    Ok(())
}

/// The second reviewer request's sections, when the second review made one.
async fn asked_again(
    before: &[LearningSourceDto],
    after: &[LearningSourceDto],
) -> Result<Option<Value>> {
    let model = ByRequest::new(|_| false, |_, _| Vec::new());
    let same = |lesson: Value| lesson;
    two_reviews(&model, &context(), &parts_lesson(2), before, &same, after).await?;
    Ok(model
        .reviews()
        .get(1)
        .map(|request| request["sectionIndicesToReview"].clone()))
}

#[tokio::test]
async fn a_reference_added_leaves_verdicts_standing_and_one_changed_or_removed_clears_them(
) -> Result<()> {
    let kept = references();
    let mut added = kept.clone();
    added.push(source(
        "second-reference",
        "Another saved passage about a different tool.",
    ));
    let mut changed = kept.clone();
    changed[0].excerpt = "This saved reference now says something else entirely.".into();
    assert_eq!(asked_again(&kept, &added).await?, None);
    assert_eq!(asked_again(&kept, &changed).await?, Some(json!([0, 1])));
    assert_eq!(asked_again(&added, &kept).await?, Some(json!([0, 1])));
    Ok(())
}

/// A model that answers the check against the plan by calling every item it
/// is asked about taught, and keeps which items it was asked about.
struct Planning<'a> {
    inner: &'a ByRequest,
    planned: Mutex<Vec<Vec<String>>>,
}

#[async_trait::async_trait]
impl LLMPort for Planning<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = task_messages(request);
        if !system.starts_with(contract::SYSTEM) || is_draft_request(request) {
            return self.inner.complete(request).await;
        }
        let sent: Value = user
            .rsplit("\n\n")
            .next()
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or(Value::Null);
        let ids: Vec<String> = sent
            .pointer("/thisLesson/planned")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item["id"].as_str().map(str::to_owned))
            .collect();
        let taught: Vec<Value> = ids
            .iter()
            .map(|id| {
                let quote = if id == "idea-0" {
                    FIRST_FACT
                } else {
                    SECOND_FACT
                };
                json!({"item": id, "taught": true, "quote": quote})
            })
            .collect();
        self.planned.lock().unwrap().push(ids);
        let reply =
            json!({"planned": taught, "usedBeforeTaught": [], "contradictions": [], "unseen": []})
                .to_string();
        Ok(crate::content_verification::tests::fixture_completion(
            request,
            vec![reply],
        ))
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

#[tokio::test]
async fn a_later_review_does_not_ask_the_plan_check_about_an_item_it_found_taught() -> Result<()> {
    let by_request = ByRequest::new(|_| false, |_, _| Vec::new());
    let model = Planning {
        inner: &by_request,
        planned: Mutex::new(Vec::new()),
    };
    let plan = json!({
        "sources":[{"sourceIndex":0,"excerpt":EXCERPT}],
        "courseSequence":{"coursePlan":[{"title":"This lesson","position":"this","ideas":["The first plain fact","The second plain fact"],"terms":[],"actions":[],"contrast":""}]}
    })
    .to_string();
    let sources = references();
    let change = |lesson| change_section(lesson, 2);
    two_reviews(&model, &plan, &parts_lesson(3), &sources, &change, &sources).await?;
    let asked = model.planned.lock().unwrap().clone();
    assert_eq!(
        asked,
        vec![
            vec!["idea-0".to_owned(), "idea-1".to_owned()],
            Vec::<String>::new()
        ]
    );
    Ok(())
}

#[tokio::test]
async fn teaching_state_follows_the_job_input_not_the_exact_draft() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&pool).await?;
    let (one, two, three) = (r#"{"draft":1}"#, r#"{"draft":2}"#, r#"{"draft":3}"#);
    lesson_drafts::run(
        &Database::from(pool.clone()),
        &repo,
        &job,
        &lesson_id,
        async {
            lesson_drafts::resume(INPUT.into()).await?;
            lesson_drafts::save(one).await?;
            lesson_drafts::save_with_passes(one, "kept state".into()).await?;
            lesson_drafts::record_pending_repair(one, "a repair".into()).await?;
            lesson_drafts::save(two).await?;
            assert_eq!(
                lesson_drafts::teaching_passes().await?.as_deref(),
                Some("kept state")
            );
            assert_eq!(lesson_drafts::pending_repair(two).await?, None);
            lesson_drafts::resume("another-input".into()).await?;
            lesson_drafts::save(three).await?;
            assert_eq!(lesson_drafts::teaching_passes().await?, None);
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn a_finding_that_used_its_repairs_gets_no_more_in_a_later_review_of_the_job() -> Result<()> {
    let model = ByRequest::new(part_a_is_wrong, |sent, _| vec![retitled(sent, 0)]);
    let sources = references();
    let change = |lesson| change_section(lesson, 1);
    let (first, second) = two_reviews(
        &model,
        &context(),
        &parts_lesson(2),
        &sources,
        &change,
        &sources,
    )
    .await?;
    assert_eq!(first.set_aside.len(), 1);
    assert_eq!(first.set_aside[0].repairs, 2);
    assert_eq!(
        model.repairs().len(),
        2,
        "the second review repaired it again"
    );
    assert_eq!(second.set_aside.len(), 1);
    assert_eq!(second.set_aside[0].repairs, 2);
    Ok(())
}
