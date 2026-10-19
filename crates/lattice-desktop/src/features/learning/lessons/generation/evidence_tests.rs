#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use crate::features::learning::content_verification::research::{self, tests as research_tests};
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use crate::features::learning::repository::LearningRepository;
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

fn topic(name: &str) -> Topic {
    Topic {
        topic: name.into(),
        queries: vec![name.into()],
    }
}

#[tokio::test]
async fn an_unusable_plan_falls_back_to_one_topic_from_the_lesson() -> Result<()> {
    let program = crate::features::learning::tests::fixture();
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

#[tokio::test]
async fn coverage_ignores_unknown_ids_and_a_missing_topic_is_uncovered() -> Result<()> {
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
    let lists = vec![vec![0], vec![1]];
    // t2 is left out; unknown topic and passage ids are ignored, and a
    // passage listed for the other topic still counts.
    let answer = json!({"topics":[
        {"id":"t1","covering":["p9","p2","bogus","p1"]},
        {"id":"t7","covering":["p2"]}
    ]});
    let llm = scripted(vec![answer.to_string()]);

    let covering = judge(&llm, &topics, &store, &lists, &references).await?;

    assert_eq!(covering, vec![vec![1, 0], vec![]]);
    // An unreadable answer leaves every topic uncovered and never fails.
    let llm = scripted(vec!["not a judgement".into()]);
    let covering = judge(&llm, &topics, &store, &lists, &references).await?;
    assert_eq!(covering, vec![Vec::<usize>::new(), Vec::new()]);
    Ok(())
}

#[tokio::test]
async fn an_uncovered_topic_triggers_one_research_round_before_writing() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    program.summary.status = crate::features::learning::dto::LearningProgramStatus::Active;
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
        json!({"topics":[{"id":"t1","covering":[]}]}).to_string(),
        json!({"needs":[{"gap":0,"query":"random assignment confounding official guidance","kind":"web"}]}).to_string(),
        research_tests::selection(&[0]),
        // Only the newly captured passage is judged again, and covers it.
        json!({"topics":[{"id":"t1","covering":["p1"]}]}).to_string(),
    ]);

    let gathered = research::run(
        pool,
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
    let passage_tokens = llm.count_tokens(&json!(texts[0]).to_string()) + 2;

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
