use super::*;
use crate::content_verification::research::{self, tests as research_tests};
use crate::lessons::course_generation_tests::ScriptedModel;
use crate::lessons::structured_call::is_draft_request;
use crate::repository::LearningRepository;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{
    ScriptedModel as Scripted, Sent,
};
use lattice_core::shared::persistence::Database;
use std::sync::Mutex;

fn scripted(outputs: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(outputs.into()),
        prompts: Mutex::new(Vec::new()),
    }
}

fn source(id: &str, excerpt: &str) -> LearningSourceDto {
    LearningSourceDto {
        id: id.into(),
        title: format!("Source {id}"),
        url: None,
        excerpt: excerpt.into(),
        acquired_at: 1,
    }
}

fn passage(source_id: &str, start: usize, text: &str) -> ReferencePassage {
    ReferencePassage {
        source_id: source_id.into(),
        text: text.into(),
        start_byte: start,
        end_byte: start + text.len(),
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
    }
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

/// The task of a coverage request: its topics and passages.
fn task_of(sent: &Sent) -> Value {
    let prompt = sent
        .request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    prompt
        .split("\n\n")
        .find_map(|chunk| {
            serde_json::from_str::<Value>(chunk)
                .ok()
                .filter(|task| task.get("topics").is_some())
        })
        .unwrap_or(Value::Null)
}

/// The names of the topics a coverage request lists.
fn topics_of(task: &Value) -> Vec<String> {
    task["topics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|topic| topic["id"].as_str().map(str::to_owned))
        .collect()
}

/// A model that answers each coverage ask with `answer(task, number of earlier asks)`, and a draft step with prose.
fn judging(answer: impl Fn(&Value, usize) -> String + Send + Sync + 'static) -> Scripted {
    let asked = std::sync::atomic::AtomicUsize::new(0);
    Scripted::new().on_request::<EvidenceCoverage>(move |sent, _| {
        if is_draft_request(&sent.request) {
            return reply("The topics are weighed.".to_owned());
        }
        let earlier = asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        reply(answer(&task_of(sent), earlier))
    })
}

fn topic(name: &str) -> Topic {
    Topic {
        topic: name.into(),
        queries: vec![name.into()],
    }
}

/// Every reply cut off at the output limit.
struct CutOff;

#[async_trait::async_trait]
impl LLMPort for CutOff {
    async fn complete(
        &self,
        _: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        Ok(
            lattice_core::application::ports::llm_port::CompletionResponse {
                finish_reason: "length".into(),
                ..lattice_core::application::ports::llm_port::CompletionResponse::from_text(
                    "{\"topics\":[{\"topic\":",
                )
            },
        )
    }
    fn model_name(&self) -> &str {
        "cut-off"
    }
    fn max_context_tokens(&self) -> usize {
        32_768
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_plan_cut_off_falls_back_to_one_topic_and_coverage_to_none() -> Result<()> {
    let program = crate::tests::fixture();
    let lesson = &program.modules[0].lessons[0];
    let references = ReferenceCollection::lexical(&program.sources)?;
    let topics = plan(&CutOff, &program, lesson, &references).await?;
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0].topic, lesson.title);
    let store = vec![passage("s1", 0, "Aggregate roots enforce invariants.")];
    let covering = judge(&CutOff, &topics, &store, &[vec![0]], &references).await?;
    assert_eq!(covering, vec![None]);
    Ok(())
}

#[tokio::test]
async fn an_unusable_plan_falls_back_to_one_topic_from_the_lesson() -> Result<()> {
    let program = crate::tests::fixture();
    let lesson = &program.modules[0].lessons[0];
    let references = ReferenceCollection::lexical(&program.sources)?;
    let llm = scripted(vec!["Here are some topics: aggregates, CQRS.".into()]);

    let topics = plan(&llm, &program, lesson, &references).await?;

    assert_eq!(
        topics,
        vec![Topic {
            topic: lesson.title.clone(),
            queries: vec![format!("{} {}", lesson.title, lesson.objective)],
        }]
    );
    // A partly usable answer keeps only its usable topics, each search once.
    let partial = json!({"topics":[
        {"topic":"Aggregates","queries":[]},
        {"topic":"Aggregate roots","queries":["aggregate root invariants","aggregate root invariants"]},
        {"name":"stray"},
        {"topic":"aggregate roots","queries":["duplicate topic"]}
    ]});
    assert_eq!(
        parse_plan(&partial.to_string()),
        Some(vec![Topic {
            topic: "Aggregate roots".into(),
            queries: vec!["aggregate root invariants".into()],
        }])
    );
    assert_eq!(parse_plan(&json!({"topics":[]}).to_string()), None);
    Ok(())
}

fn two_sources() -> Vec<LearningSourceDto> {
    vec![
        source(
            "a",
            "Aggregates protect invariants inside one consistency boundary.",
        ),
        source(
            "b",
            "Command handlers change state; query handlers only read it.",
        ),
    ]
}

#[tokio::test]
async fn coverage_ignores_unknown_handles_and_a_missing_topic_is_asked_again_alone() -> Result<()> {
    let sources = two_sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let store = vec![
        passage("a", 0, &sources[0].excerpt),
        passage("b", 0, &sources[1].excerpt),
    ];
    let topics = vec![topic("Aggregates"), topic("CQRS")];
    let lists = vec![vec![0], vec![1]];
    // t2 is never answered; an entry that names nothing shown is no entry, an
    // unknown passage is only a lost cite, and a passage listed for the other
    // topic still counts.
    let llm = judging(|task, _| {
        let mut entries = vec![json!({"of":"t7","covering":["p2"]})];
        if topics_of(task).contains(&"t1".to_owned()) {
            entries.push(json!({"of":"t1","covering":["p9","p2","bogus","p1"]}));
        }
        json!({ "entries": entries }).to_string()
    });

    let covering = judge(&llm, &topics, &store, &lists, &references).await?;

    assert_eq!(covering, vec![Some(vec![1, 0]), None]);
    let asked: Vec<Vec<String>> = llm
        .sent()
        .iter()
        .filter(|sent| !is_draft_request(&sent.request))
        .map(|sent| topics_of(&task_of(sent)))
        .collect();
    assert_eq!(asked[0], ["t1", "t2"]);
    assert!(asked.len() > 1 && asked[1..].iter().all(|topics| topics == &["t2"]));
    // An unreadable answer leaves every topic unjudged and never fails.
    let llm = judging(|_, _| "not a judgement".to_owned());
    let covering = judge(&llm, &topics, &store, &lists, &references).await?;
    assert_eq!(covering, vec![None, None]);
    Ok(())
}

#[tokio::test]
async fn a_topic_decided_twice_differently_is_asked_again_alone_and_the_others_stand() -> Result<()>
{
    let sources = two_sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let store = vec![
        passage("a", 0, &sources[0].excerpt),
        passage("b", 0, &sources[1].excerpt),
    ];
    let topics = vec![topic("Aggregates"), topic("CQRS")];
    let lists = vec![vec![0], vec![1]];
    let llm = judging(|task, _| {
        if topics_of(task) == ["t2"] {
            return json!({"entries":[{"of":"t2","covering":["p2"]}]}).to_string();
        }
        json!({"entries":[
            {"of":"t1","covering":["p1"]},
            {"of":"t2","covering":["p2"]},
            {"of":"t2","covering":[]}
        ]})
        .to_string()
    });

    let covering = judge(&llm, &topics, &store, &lists, &references).await?;

    assert_eq!(covering, vec![Some(vec![0]), Some(vec![1])]);
    Ok(())
}

#[tokio::test]
async fn a_topic_with_no_candidate_is_uncovered_without_being_asked() -> Result<()> {
    let sources = two_sources();
    let references = ReferenceCollection::lexical(&sources)?;
    let store = vec![passage("a", 0, &sources[0].excerpt)];
    let topics = vec![topic("Aggregates"), topic("CQRS")];
    let llm = judging(|_, _| json!({"entries":[{"of":"t1","covering":["p1"]}]}).to_string());

    let covering = judge(&llm, &topics, &store, &[vec![0], vec![]], &references).await?;

    assert_eq!(covering, vec![Some(vec![0]), Some(vec![])]);
    assert_eq!(llm.sent().len(), 1);
    Ok(())
}

#[tokio::test]
async fn an_uncovered_topic_triggers_one_research_round_before_writing() -> Result<()> {
    let db = Database::new(crate::tests::pool().await?);
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    program.sources[0].excerpt = "Random numbers decide the lottery draws in this course.".into();
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let topics = vec![Topic {
        topic: "Random assignment".into(),
        queries: vec!["random assignment confounding".into()],
    }];
    let llm = scripted(vec![
        // The saved lottery passage does not cover the topic.
        json!({"entries":[{"of":"t1","covering":[]}]}).to_string(),
        crate::references::web_research::needs_answer(json!([{"gap":0,"query":"random assignment confounding official guidance","kind":"web"}])).to_string(),
        research_tests::selection(&[0]),
        // Only the newly captured passage is judged again, and covers it.
        json!({"entries":[{"of":"t1","covering":["p1"]}]}).to_string(),
    ]);

    let gathered = research::run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(research_tests::web(false)),
        gather(&llm, &references, &topics, 50_000),
    )
    .await?;

    let expanded = gathered
        .expanded
        .as_ref()
        .expect("research saved a reference");
    assert_eq!(expanded.sources.len(), sources.len() + 1);
    let covered: Vec<_> = gathered.covering[0]
        .iter()
        .map(|index| gathered.passages[*index].text.as_str())
        .collect();
    assert_eq!(covered, vec![research_tests::CAPTURE]);
    assert_eq!(gathered.candidates[0].len(), 2);
    assert!(llm.outputs.lock().unwrap().is_empty());
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 4);
    assert!(prompts[3].contains(research_tests::CAPTURE));
    assert!(!prompts[3].contains("lottery"));
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_evidence_plan() -> Result<()> {
    let mut program = crate::tests::fixture();
    crate::lessons::course_generation_tests::fixed_ids(&mut program);
    let lesson = &program.modules[0].lessons[0];
    let references = ReferenceCollection::lexical(&program.sources)?;
    let inner = scripted(vec![json!({"topics":[
        {"topic":"Aggregates","queries":["aggregate root invariants"]},
        {"topic":"Command handlers","queries":["command handler state change"]}
    ]})
    .to_string()]);
    let llm = crate::lessons::course_generation_tests::RequestLog::new(&inner);

    plan(&llm, &program, lesson, &references).await?;

    llm.assert_snapshot::<EvidencePlan>();
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_evidence_coverage() -> Result<()> {
    let sources = vec![
        source(
            "a",
            "Aggregates protect invariants inside one consistency boundary.",
        ),
        source(
            "b",
            "Command handlers change state; query handlers only read it.",
        ),
    ];
    let references = ReferenceCollection::lexical(&sources)?;
    let store = vec![
        passage("a", 0, &sources[0].excerpt),
        passage("b", 0, &sources[1].excerpt),
    ];
    let topics = vec![topic("Aggregates"), topic("CQRS")];
    let lists = vec![vec![0, 1], vec![1]];
    let answer = json!({"entries":[
        {"of":"t1","covering":["p1"]},
        {"of":"t2","covering":["p2"]}
    ]});
    let inner = scripted(vec![answer.to_string()]);
    let llm = crate::lessons::course_generation_tests::RequestLog::new(&inner);

    judge(&llm, &topics, &store, &lists, &references).await?;

    llm.assert_snapshot::<EvidenceCoverage>();
    Ok(())
}

#[test]
fn the_pack_goes_round_robin_across_topics_within_the_room_without_cutting_passages() {
    let long = |tag: &str| format!("{tag} {}", "evidence ".repeat(375));
    let texts: Vec<String> = ["a0", "a1", "a2", "b0", "c0"]
        .iter()
        .map(|tag| long(tag))
        .collect();
    assert!(texts.iter().all(|text| text.len() > 3000));
    let sources = vec![
        source("a", &texts[..3].join("\n")),
        source("b", &texts[3]),
        source("c", &texts[4]),
    ];
    let references = ReferenceCollection::lexical(&sources).unwrap();
    let store = vec![
        passage("a", 0, &texts[0]),
        passage("a", 4000, &texts[1]),
        passage("a", 8000, &texts[2]),
        passage("b", 0, &texts[3]),
        passage("c", 0, &texts[4]),
    ];
    let gathered = Gathered {
        expanded: None,
        passages: store,
        // Topic 0 is covered by its third passage; topic 1 shares a passage
        // with topic 0; topic 2 is uncovered.
        candidates: vec![vec![0, 1, 2], vec![0, 3], vec![4]],
        covering: vec![vec![2], vec![3], vec![]],
    };
    let topics = vec![topic("First"), topic("Second"), topic("Third")];
    let llm = scripted(Vec::new());
    let passage_tokens = llm.count_tokens(&json!(texts[0]).to_string()) + 1;

    // Room for four passages and their sources, not five.
    let added = pack(&llm, &gathered, &references, 4 * passage_tokens + 100);

    assert_eq!(added, vec![2, 3, 4, 0]);
    let (packed, map) = assemble(&gathered, &topics, &references, &added);
    assert_eq!(
        packed.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    // Whole passages, in document order, with no excerpt cap.
    assert_eq!(packed[0].excerpt, format!("{}\n\n{}", texts[0], texts[2]));
    assert_eq!(
        map,
        vec![
            json!({"topic":"First","coveringSourceIndexes":[0]}),
            json!({"topic":"Second","coveringSourceIndexes":[1]}),
            json!({"topic":"Third","coveringSourceIndexes":[]}),
        ]
    );
    // A larger room packs every passage once.
    assert_eq!(
        pack(&llm, &gathered, &references, 10 * passage_tokens).len(),
        5
    );
}

#[test]
fn the_lesson_pack_charges_one_separator_token_per_item() {
    let sources = vec![
        source("a", "Aggregate roots enforce invariants."),
        source("b", "Command handlers change state."),
    ];
    let references = ReferenceCollection::lexical(&sources).unwrap();
    let gathered = Gathered {
        expanded: None,
        passages: vec![
            passage("a", 0, &sources[0].excerpt),
            passage("b", 0, &sources[1].excerpt),
        ],
        candidates: vec![vec![0, 1]],
        covering: vec![vec![0, 1]],
    };
    let llm = scripted(Vec::new());
    let cost = |value: Value| llm.count_tokens(&value.to_string()) + 1;
    let room: usize = sources
        .iter()
        .enumerate()
        .map(|(at, source)| {
            cost(json!(source.excerpt))
                + cost(json!({"sourceIndex":at,"title":source.title,"url":null,"excerpt":""}))
        })
        .sum();
    assert_eq!(pack(&llm, &gathered, &references, room), vec![0, 1]);
}

#[test]
fn topic_evidence_plans_are_keyed_by_a_versioned_protocol() {
    assert_eq!(POLICY.to_string(), "lesson-topic-evidence-v1");
}

#[test]
fn a_short_collection_is_measured_per_character_not_as_one_passage() {
    let llm = scripted(Vec::new());
    let short =
        ReferenceCollection::lexical(&[source("a", "Forty characters of saved reference text.")])
            .unwrap();
    let long_text = "Saved reference text. ".repeat(PASSAGE_CHARS);
    let long = ReferenceCollection::lexical(&[source("b", &long_text)]).unwrap();
    // A full passage of either text costs about the same tokens.
    let full = llm.count_tokens(&"x".repeat(PASSAGE_CHARS));
    assert!(passage_tokens(&llm, &short).abs_diff(full) <= full / 20);
    assert!(passage_tokens(&llm, &long).abs_diff(full) <= full / 20);
    let topics = vec![topic("Saved reference")];
    assert_eq!(depth(&llm, &short, &topics, 10 * full), 10);
}

#[test]
fn the_plan_schema_bounds_no_text_but_a_blank() {
    let bounds = crate::lessons::course_generation_tests::length_bounds(&plan_schema());
    assert!(bounds.is_empty(), "{bounds:?}");
}

#[test]
fn the_evidence_plan_contract_reads_what_it_renders() {
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        EvidencePlan,
    >(&vec![
        PlannedTopic {
            topic: "A topic".into(),
            queries: vec!["a search".into(), "another search".into()],
        },
        PlannedTopic {
            topic: "Another topic".into(),
            queries: vec![],
        },
    ]);
}

#[test]
fn the_coverage_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let at = ("source-harbour".to_owned(), 100_001_usize, 100_002_usize);
    let input = Input {
        topics: vec![Subject {
            at: marks.reference(100_003_usize),
            name: marks.told("Tide tables".to_owned()),
            candidates: marks.internal(vec![at.clone()]),
        }],
        passages: Packed::new(
            vec![Passage {
                at: marks.reference(at),
                title: marks.told("Harbour almanac".to_owned()),
                text: marks.told("Tide tables are revised each spring.".to_owned()),
            }],
            |_| 1,
        ),
    };
    holds::<EvidenceCoverage>(&input, &marks);
}

#[tokio::test]
async fn an_unjudged_topic_is_not_researched_on_a_guess() -> Result<()> {
    let db = Database::new(crate::tests::pool().await?);
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    program.sources[0].excerpt = "Random numbers decide the lottery draws in this course.".into();
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let topics = vec![
        Topic {
            topic: "Random assignment".into(),
            queries: vec!["random assignment confounding".into()],
        },
        Topic {
            topic: "Allocation concealment".into(),
            queries: vec!["random allocation concealment".into()],
        },
    ];
    let llm = scripted(vec![
        // The first topic is judged uncovered; the second is never answered,
        // not when asked again alone and not when corrected.
        json!({"entries":[{"of":"t1","covering":[]}]}).to_string(),
        json!({"entries":[]}).to_string(),
        json!({"entries":[]}).to_string(),
        crate::references::web_research::needs_answer(json!([{"gap":0,"query":"random assignment confounding official guidance","kind":"web"}])).to_string(),
        research_tests::selection(&[0]),
        json!({"entries":[{"of":"t1","covering":["p1"]}]}).to_string(),
    ]);

    let gathered = research::run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(research_tests::web(false)),
        gather(&llm, &references, &topics, 50_000),
    )
    .await?;

    assert!(!gathered.covering[0].is_empty());
    assert!(gathered.covering[1].is_empty());
    let prompts = llm.prompts.lock().unwrap();
    let planner = &prompts[3];
    assert!(planner.contains("Random assignment"));
    assert!(!planner.contains("Allocation concealment"));
    Ok(())
}
