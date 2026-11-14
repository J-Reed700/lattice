//! Content reaching a model is bounded by its window, never by a fixed count
//! or a character prefix.
use crate::dto::{
    LearningOutcomeDefinitionDto, LearningProgramDto, LearningSourceDto, LearningSourceVersionDto,
    LearningSourceVersionSummaryDto,
};
use crate::lessons::course_generation_tests::{Model, ScriptedModel};
use crate::reference_collection::{snapshot_passages, ReferenceCollection};
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
async fn snapshot_passages_charge_one_separator_token_per_item() -> Result<()> {
    let texts = [
        "Borrowing gives access without moving ownership.",
        "Borrowing lets code read data it does not own.",
    ];
    let sources: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(at, text)| version(&format!("v{at}"), (*text).into()))
        .collect();
    let llm = scripted(vec![]);
    let cost = |value: Value| llm.count_tokens(&value.to_string()) + 1;
    let room: usize = sources
        .iter()
        .map(|source| {
            cost(json!(source.full_text))
                + cost(json!({"sourceId":source.source_id,"versionId":source.version.id,"title":source.version.title,"passages":[]}))
        })
        .sum();
    let found = snapshot_passages(&llm, &sources, "borrowing", room).await?;
    assert_eq!(found.as_array().map(Vec::len), Some(2), "{found}");
    Ok(())
}

#[tokio::test]
async fn a_need_search_pages_while_new_results_appear_and_search_need_reads_one_page() -> Result<()>
{
    use lattice_core::application::contracts::web::WebSearchResult;
    let web = lattice_core::application::ports::MockWebService::new();
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
    let need = crate::references::web_research::PlannedNeed::web("query".into());
    use crate::references::web_research::{search_need, NeedSearch, Room};
    let first = search_need(&web, &need, Room::for_all()).await?;
    assert_eq!(first.len(), 12, "one page");
    let mut search = NeedSearch::new(&need);
    let mut results = Vec::new();
    while !search.is_done() {
        results.extend(search.next_page(&web, Room::for_all()).await?);
    }
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
    let planned = crate::references::web_research::plan_needs(
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
    let mut program = crate::tests::fixture();
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
    let authored =
        crate::diagnostic_generation::author(&llm, &program, &outcomes, &sources).await?;
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
        let (system, user) = crate::lessons::structured_call::task_messages(request);
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
    use crate::assessment_engine::{LearningBlueprintRequirement, LearningItemFormat};
    use crate::assessment_generation::{author_items, AuthoringNeed, BankPass, MaterialPassage};
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
        pass: BankPass::Retakes,
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
    use crate::dto::{LearningPracticeTutorTurnDto, LearningTutorRequestKind};
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
    let shown = crate::practice_generation::with_recent_turns(&llm, prompt.clone(), &few);
    assert_eq!(shown["recentTurns"].as_array().unwrap().len(), 6);
    assert_eq!(shown["earlierTurns"], "");
    // Four hundred do not: the newest whole turns, then a count of the rest.
    let shown = crate::practice_generation::with_recent_turns(&llm, prompt, &many);
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

/// Every text length a schema bounds beyond a non-blank string: a `maxLength`
/// anywhere, a `minLength` above 1.
pub(crate) fn text_caps(schema: &Value) -> Vec<String> {
    fn walk(value: &Value, at: &str, found: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                for (key, inner) in fields {
                    let here = format!("{at}/{key}");
                    let capped = match key.as_str() {
                        "maxLength" => true,
                        "minLength" => inner.as_u64().is_some_and(|bound| bound > 1),
                        _ => false,
                    };
                    if capped {
                        found.push(format!("{here} = {inner}"));
                    }
                    walk(inner, &here, found);
                }
            }
            Value::Array(items) => {
                for (index, inner) in items.iter().enumerate() {
                    walk(inner, &format!("{at}/{index}"), found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(schema, "", &mut found);
    found
}

/// Answers as the model it wraps does, keeping every schema a request carried.
pub(crate) struct Recording<'a> {
    inner: &'a dyn LLMPort,
    schemas: Mutex<Vec<Value>>,
}

impl<'a> Recording<'a> {
    pub(crate) fn new(inner: &'a dyn LLMPort) -> Self {
        Self {
            inner,
            schemas: Mutex::new(Vec::new()),
        }
    }

    /// The schemas sent so far that mention `field`.
    pub(crate) fn schemas_naming(&self, field: &str) -> Vec<Value> {
        self.schemas
            .lock()
            .unwrap()
            .iter()
            .filter(|schema| schema.to_string().contains(field))
            .cloned()
            .collect()
    }
}

#[async_trait::async_trait]
impl LLMPort for Recording<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if let Some(schema) = &request.json_schema {
            self.schemas.lock().unwrap().push(schema.clone());
        }
        self.inner.complete(request).await
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
        self.inner.is_ready().await
    }
}

#[test]
fn a_schema_is_searched_for_every_text_length_it_bounds() {
    let schema = json!({"type":"object","properties":{
        "blank":{"type":"string","minLength":1},
        "nested":{"type":"array","items":{"type":["string","null"],"maxLength":700}},
        "floor":{"anyOf":[{"type":"string","minLength":30}]},
        "count":{"type":"array","minItems":2,"maxItems":6}
    }});
    let mut found = text_caps(&schema);
    found.sort();
    assert_eq!(
        found,
        [
            "/properties/floor/anyOf/0/minLength = 30",
            "/properties/nested/items/maxLength = 700"
        ]
    );
}

fn module_outcomes(program: &LearningProgramDto) -> Vec<LearningOutcomeDefinitionDto> {
    program
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
        .collect()
}

/// A model's answer: the same task for every outcome.
fn starting_point_tasks(
    outcomes: &[LearningOutcomeDefinitionDto],
    prompt: &str,
    expected_answer: &str,
    criteria: &str,
) -> String {
    let tasks: Vec<_> = outcomes
        .iter()
        .map(|outcome| {
            json!({"outcomeId":outcome.id,"prompt":prompt,"expectedAnswer":expected_answer,"criteria":criteria})
        })
        .collect();
    json!({ "tasks": tasks }).to_string()
}

#[tokio::test]
async fn starting_point_tasks_are_read_at_any_length() -> Result<()> {
    let program = crate::tests::fixture();
    let outcomes = module_outcomes(&program);
    let sources = vec![source("s", "Skill guidance for every module.".into())];
    for (what, prompt, expected, criteria) in [
        (
            "tasks past their old ceilings",
            "Predict the result of this concrete case and explain why. ".repeat(40),
            "The result follows from the stated rule applied to the case. ".repeat(40),
            "Names the rule, applies it, and states the predicted result. ".repeat(25),
        ),
        (
            "tasks under their old floors",
            "Name the rule.".to_owned(),
            "Rule one.".to_owned(),
            "Rule.".to_owned(),
        ),
    ] {
        let llm = Model(starting_point_tasks(
            &outcomes, &prompt, &expected, &criteria,
        ));
        let authored =
            crate::diagnostic_generation::author(&llm, &program, &outcomes, &sources).await;
        assert!(authored.is_ok(), "{what}: {authored:?}");
        let authored = authored?;
        assert_eq!(authored.len(), outcomes.len(), "{what}");
        assert!(
            authored.iter().all(|task| task.prompt == prompt
                && task.expected_answer == expected
                && task.criteria == criteria),
            "{what}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_starting_point_tasks_schema_bounds_no_text_but_a_blank_one() -> Result<()> {
    let program = crate::tests::fixture();
    let outcomes = module_outcomes(&program);
    let sources = vec![source("s", "Skill guidance for every module.".into())];
    let model = Model(starting_point_tasks(
        &outcomes,
        "Predict the result of this concrete case and explain why.",
        "The result follows from the stated rule applied to the case.",
        "Names the rule, applies it, and states the predicted result.",
    ));
    let recording = Recording::new(&model);
    crate::diagnostic_generation::author(&recording, &program, &outcomes, &sources).await?;
    let schemas = recording.schemas_naming("expectedAnswer");
    assert!(!schemas.is_empty(), "the tasks schema was sent");
    for schema in &schemas {
        let caps = text_caps(schema);
        assert!(caps.is_empty(), "{caps:?}");
    }
    Ok(())
}
