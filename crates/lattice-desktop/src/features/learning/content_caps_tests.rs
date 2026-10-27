//! Content reaching a model is bounded by its window, never by a fixed count
//! or a character prefix.
use crate::features::learning::dto::{
    LearningOutcomeDefinitionDto, LearningSourceDto, LearningSourceVersionDto,
    LearningSourceVersionSummaryDto,
};
use crate::features::learning::lessons::course_generation_tests::{Model, ScriptedModel};
use crate::features::learning::reference_collection::{snapshot_passages, ReferenceCollection};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use serde_json::{json, Value};
use std::sync::Mutex;

fn source(id: &str, excerpt: String) -> LearningSourceDto {
    LearningSourceDto {
        id: id.into(),
        title: format!("Reference {id}"),
        url: None,
        excerpt,
        acquired_at: 1,
    }
}

fn version(id: &str, full_text: String) -> LearningSourceVersionDto {
    LearningSourceVersionDto {
        source_id: format!("source-{id}"),
        version: LearningSourceVersionSummaryDto {
            id: id.into(),
            version_number: 1,
            title: format!("Reference {id}"),
            publisher: None,
            resolved_url: None,
            excerpt: full_text.chars().take(2400).collect(),
            content_sha256: String::new(),
            word_count: 0,
            truncated: false,
            extraction_version: "fixture".into(),
            acquired_at: 1,
            snapshot_hash: None,
        },
        full_text,
        usage: vec![],
    }
}

fn scripted(outputs: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(outputs.into()),
        prompts: Mutex::new(Vec::new()),
    }
}

/// A long text whose only mention of `needle` sits far into it.
fn deep(needle: &str) -> String {
    format!(
        "{}{needle}",
        "Unrelated introductory material. ".repeat(900)
    )
}

#[tokio::test]
async fn retrieval_has_no_count_ceiling() -> Result<()> {
    let sources: Vec<_> = (0..60)
        .map(|i| {
            source(
                &format!("s{i}"),
                format!("Window functions compute totals in report {i}."),
            )
        })
        .collect();
    let collection = ReferenceCollection::lexical(&sources)?;
    assert_eq!(
        collection
            .retrieve("window functions totals", 60)
            .await?
            .len(),
        60
    );
    // Filled retrieval pages past its first page while everything fits, and
    // stops where the caller's window does.
    let all = collection
        .retrieve_within("window functions totals", |_| true)
        .await?;
    assert_eq!(all.len(), 60);
    let some = collection
        .retrieve_within("window functions totals", |passages| passages.len() <= 23)
        .await?;
    assert_eq!(some.len(), 23);
    Ok(())
}

#[test]
fn the_catalog_keeps_whole_headings_however_many() {
    let heading = format!("# {}", "A long descriptive heading ".repeat(8));
    let text = (0..60)
        .map(|i| format!("{heading}{i}\nBody {i}."))
        .collect::<Vec<_>>()
        .join("\n");
    let collection = ReferenceCollection::lexical(&[source("a", text)]).unwrap();
    let catalog = collection.catalog(&scripted(vec![]));
    let headings = catalog["sources"][0]["headings"].as_array().unwrap();
    assert_eq!(headings.len(), 60);
    assert!(headings.iter().all(|h| h.as_str().unwrap().len() > 200));
    assert_eq!(catalog["sources"][0]["additionalHeadingsOmitted"], false);
}

#[tokio::test]
async fn snapshot_passages_reach_text_past_the_former_prefixes() -> Result<()> {
    let needle = "Partition pruning skips partitions that cannot match the predicate.";
    let sources = vec![version("v1", deep(needle))];
    let passages = snapshot_passages(
        &scripted(vec![]),
        &sources,
        "partition pruning predicate",
        100_000,
    )
    .await?;
    assert!(passages[0]["passages"][0]
        .as_str()
        .unwrap()
        .contains(needle));
    // Nothing fits an empty room.
    let none = snapshot_passages(&scripted(vec![]), &sources, "partition pruning", 0).await?;
    assert_eq!(none, json!([]));
    Ok(())
}

#[tokio::test]
async fn web_search_pages_while_new_results_appear() -> Result<()> {
    use lattice_core::application::contracts::web::WebSearchResult;
    let web = crate::features::web::mocks::MockWebService::new();
    web.set_search_results(
        (0..30)
            .map(|i| WebSearchResult {
                title: format!("Result {i}"),
                url: format!("https://example.org/{i}"),
                snippet: String::new(),
                published_date: None,
                source: None,
            })
            .collect(),
    );
    let need =
        crate::features::learning::references::web_research::PlannedNeed::web("query".into());
    let results = crate::features::learning::references::web_research::search_need(
        &web,
        &need,
        crate::features::learning::references::web_research::Room::for_all(),
    )
    .await?;
    assert_eq!(results.len(), 30, "not the former 12");
    Ok(())
}

#[tokio::test]
async fn research_plans_as_many_searches_as_the_gaps_need() -> Result<()> {
    let queries: Vec<String> = (0..5).map(|i| format!("distinct search {i}")).collect();
    let needs: Vec<Value> = queries
        .iter()
        .map(|query| json!({"gap": 0, "query": query, "kind": "web"}))
        .collect();
    let llm = scripted(vec![json!({ "needs": needs }).to_string()]);
    let planned = crate::features::learning::references::web_research::plan_needs(
        &llm,
        "topic",
        &[json!({"claim":"c","missingEvidence":"m"})],
        json!({}),
        &[],
    )
    .await?;
    let planned: Vec<String> = planned
        .into_iter()
        .map(|served| served.need.query)
        .collect();
    assert_eq!(planned, queries);
    Ok(())
}

#[tokio::test]
async fn the_starting_point_check_covers_every_module() -> Result<()> {
    let mut program = crate::features::learning::tests::fixture();
    let template = program.modules[0].clone();
    for i in 0..6 {
        let mut module = template.clone();
        module.id = format!("extra-module-{i}");
        program.modules.push(module);
    }
    let outcomes: Vec<_> = program
        .modules
        .iter()
        .enumerate()
        .map(|(i, module)| LearningOutcomeDefinitionDto {
            id: format!("outcome-{i}"),
            module_id: Some(module.id.clone()),
            lesson_id: None,
            title: format!("Apply skill {i}"),
            description: String::new(),
            ordinal: i as i64,
            created_at: 0,
        })
        .collect();
    let tasks: Vec<_> = outcomes.iter().map(|o| json!({"outcomeId":o.id,"prompt":"Predict the result of this concrete case and explain why.","expectedAnswer":"The result follows from the stated rule applied to the case.","criteria":"Names the rule, applies it, and states the predicted result."})).collect();
    let llm = Model(json!({ "tasks": tasks }).to_string());
    let sources = vec![source("s", "Skill guidance for every module.".into())];
    let authored = crate::features::learning::diagnostic_generation::author(
        &llm, &program, &outcomes, &sources,
    )
    .await?;
    assert_eq!(authored.len(), 8, "not the former six");
    Ok(())
}

/// Answers each quiz authoring request with items for the one requirement
/// it names, quoting the first passage it carries.
struct PerRequirement {
    inner: Model,
    prompts: Mutex<Vec<Value>>,
}

#[async_trait::async_trait]
impl LLMPort for PerRequirement {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) =
            crate::features::learning::lessons::structured_call::task_messages(request);
        if !system.starts_with("Write quiz questions") {
            return self.inner.complete(request).await;
        }
        let context: Value = serde_json::from_str(user)?;
        self.prompts.lock().unwrap().push(context.clone());
        let quote: String = context["passages"][0]["text"]
            .as_str()
            .unwrap()
            .chars()
            .take(60)
            .collect();
        let items: Vec<_> = (0..context["returnCount"].as_u64().unwrap()).map(|i| json!({
            "prompt":format!("Explain how partition pruning changes this query plan for {}, case {i}.", context["outcome"].as_str().unwrap()),
            "rubric":[
                {"title":"Mechanism","description":"Explains which partitions the predicate excludes and why.","dimension":"explanation"},
                {"title":"Evidence","description":"Connects the explanation to the stated query plan.","dimension":"application"}
            ],
            "explanation":"A strong answer ties pruning to the predicate.",
            "blockIndex":0,"quote":quote,"difficulty":3
        })).collect();
        Ok(CompletionResponse::from_text(
            json!({ "items": items }).to_string(),
        ))
    }
    fn model_name(&self) -> &str {
        "per-requirement"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn quiz_items_are_authored_per_requirement_from_retrieved_passages() -> Result<()> {
    use crate::features::learning::assessment_engine::{
        LearningBlueprintRequirement, LearningItemFormat,
    };
    use crate::features::learning::assessment_generation::{
        author_items, AuthoringNeed, MaterialPassage,
    };
    let needle = "Partition pruning skips partitions that cannot match the predicate.";
    let need = |title: &str| AuthoringNeed {
        requirement: LearningBlueprintRequirement {
            outcome_id: uuid::Uuid::new_v4().to_string(),
            format: LearningItemFormat::Explanation,
            count: 1,
            difficulty_min: 2,
            difficulty_max: 3,
        },
        outcome_title: title.into(),
        outcome_description: "Explain partition pruning against a predicate.".into(),
        lesson_id: None,
        count: 1,
        material: vec![MaterialPassage {
            lesson_title: "Query plans".into(),
            title: "Pruning".into(),
            text: deep(needle),
            source_version_ids: vec![],
        }],
        existing_prompts: vec![],
    };
    let model = PerRequirement {
        inner: Model(json!({"issues":[]}).to_string()),
        prompts: Mutex::new(Vec::new()),
    };
    let first = author_items(&model, &need("Partition pruning")).await?;
    let second = author_items(&model, &need("Predicate pushdown and pruning")).await?;
    assert_eq!(first.items.len() + second.items.len(), 2);
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2, "one request per requirement");
    // The passage past the former 8,000-character prefix reached the model.
    assert!(prompts[0]["passages"][0]["text"]
        .as_str()
        .unwrap()
        .contains(needle));
    Ok(())
}

#[test]
fn the_tutor_sees_the_newest_whole_turns_that_fit_and_counts_the_rest() {
    use crate::features::learning::dto::{LearningPracticeTutorTurnDto, LearningTutorRequestKind};
    let turn = |index: usize| LearningPracticeTutorTurnDto {
        id: format!("turn-{index}"),
        prompt: format!(
            "Question {index}: {}",
            "why does this step hold? ".repeat(40)
        ),
        response: format!(
            "Answer {index}: {}",
            "consider the earlier result. ".repeat(40)
        ),
        request_kind: LearningTutorRequestKind::Question,
        hint_level: None,
        citations: vec![],
        proposal_ids: vec![],
        model_name: "fixture".into(),
        created_at: index as i64,
    };
    let few: Vec<_> = (0..6).map(turn).collect();
    let many: Vec<_> = (0..400).map(turn).collect();
    let llm = scripted(vec![]);
    let prompt =
        json!({"taskPrompt":"Solve it.","recentTurns":[],"earlierTurns":"","sourceSnapshots":[]});
    // Six turns all fit: not the former four.
    let shown = crate::features::learning::practice_generation::with_recent_turns(
        &llm,
        prompt.clone(),
        &few,
    );
    assert_eq!(shown["recentTurns"].as_array().unwrap().len(), 6);
    assert_eq!(shown["earlierTurns"], "");
    // Four hundred do not: the newest whole turns, then a count of the rest.
    let shown =
        crate::features::learning::practice_generation::with_recent_turns(&llm, prompt, &many);
    let turns = shown["recentTurns"].as_array().unwrap();
    assert!(turns.len() > 4 && turns.len() < 400);
    assert!(turns[0]["request"]
        .as_str()
        .unwrap()
        .starts_with("Question 399:"));
    assert_eq!(
        shown["earlierTurns"],
        format!("{} earlier turns not shown", 400 - turns.len())
    );
}
