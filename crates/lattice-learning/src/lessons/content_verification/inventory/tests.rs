#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::{fixture_prompts, fixture_task};
use crate::lessons::course_generation_tests::ScriptedModel;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::shape::Pointers;
use lattice_core::application::services::model_contract::test_support::samples::schema_of;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{
    ScriptedModel as Layered, Sent,
};
use lattice_core::shared::persistence::Database;
use std::sync::Mutex;

/// An answer that locates one claim at the sentence `sentence` of the section shown as `of`.
fn answer(of: &str, sentence: &str) -> Value {
    json!({"of":of,"claims":[{"sentenceId":sentence,"statement":"A scoped factual statement."}],"nonFactualReason":""})
}
fn model(responses: Vec<Value>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(responses.into_iter().map(|v| v.to_string()).collect()),
        prompts: Mutex::new(Vec::new()),
    }
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

/// The sections a request shows: each one's handle, and the handle and text of each of its sentences.
fn shown(sent: &Sent) -> Vec<(String, Vec<(String, String)>)> {
    let prompt = fixture_prompts(&sent.request)
        .into_iter()
        .next()
        .unwrap_or_default();
    let task = fixture_task(&prompt).unwrap_or(Value::Null);
    task["units"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|unit| {
            let sentences = unit["passages"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|passage| {
                    passage["sentences"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|sentence| {
                            (
                                sentence["id"].as_str().unwrap_or_default().to_owned(),
                                sentence["text"].as_str().unwrap_or_default().to_owned(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            (
                unit["id"].as_str().unwrap_or_default().to_owned(),
                sentences,
            )
        })
        .collect()
}

/// An answer that gives each section the request shows the claims `claims_of` makes of its sentences, and none to a section it gives none.
fn answering(
    sent: &Sent,
    claims_of: impl Fn(&[(String, String)]) -> Option<Vec<Value>>,
) -> String {
    let entries: Vec<Value> = shown(sent)
        .iter()
        .filter_map(|(of, sentences)| {
            claims_of(sentences)
                .map(|claims| json!({"of":of,"claims":claims,"nonFactualReason":""}))
        })
        .collect();
    fixture_response(entries).to_string()
}

/// The claim `statement`, located at the first sentence that holds `phrase`.
fn claim_at(sentences: &[(String, String)], phrase: &str, statement: &str) -> Value {
    let id = sentences
        .iter()
        .find(|(_, text)| text.contains(phrase))
        .map(|(id, _)| id.as_str())
        .unwrap_or_default();
    json!({"sentenceId":id,"statement":statement})
}

/// The claims `statement` located at the first sentence of every section.
fn at_the_first(sentences: &[(String, String)]) -> Option<Vec<Value>> {
    let (id, _) = sentences.first()?;
    Some(vec![
        json!({"sentenceId":id,"statement":"A scoped factual statement."}),
    ])
}

#[test]
fn the_extraction_request_shows_what_it_tells_and_hides_the_internal_ids() {
    let mut marks = Marks::new();
    let passages = |marks: &mut Marks, ids: [&str; 2], texts: [&str; 2]| -> Vec<Passage> {
        ids.iter()
            .zip(texts)
            .map(|(id, text)| Passage {
                id: marks.internal((*id).to_owned()),
                field: marks.told("/explanation".to_owned()),
                text: marks.told(text.to_owned()),
            })
            .collect()
    };
    let first = Section {
        index: marks.internal(7_340_219_usize),
        kind: Some(marks.told("teaching".to_owned())),
        correct_index: None,
        check_role: None,
        context: Vec::new(),
        context_role: None,
        passages: passages(
            &mut marks,
            ["unit-7340219-passage-0", "unit-7340219-passage-1"],
            [
                "The harbour kiln heats before a firing.",
                "Tide tables list high water.",
            ],
        ),
    };
    let second = Section {
        index: marks.internal(7_340_220_usize),
        kind: Some(marks.told("assessment".to_owned())),
        correct_index: Some(marks.told(2_u64)),
        check_role: Some(marks.told("A check poses a case first.".to_owned())),
        context: vec![Part {
            field: marks.told("/options/0".to_owned()),
            text: marks.told("Which kiln heats the glaze fastest?".to_owned()),
        }],
        context_role: Some(marks.told("The question and its options.".to_owned())),
        passages: passages(
            &mut marks,
            ["unit-7340220-passage-0", "unit-7340220-passage-1"],
            ["The glaze kiln is the fastest.", "The salt kiln is slower."],
        ),
    };
    holds::<InventoryExtraction>(
        &Input {
            sections: vec![first, second],
        },
        &marks,
    );
}
#[test]
fn equal_sentences_of_a_section_are_two_places_a_claim_can_name() {
    let body = format!(
        "{}\n\n{}",
        "Tides follow the moon.", "Tides follow the moon."
    );
    let section = Section::of(0, &inputs(&[json!({"body":body})])[0]);
    let texts = section.texts();
    let units: Vec<_> = texts
        .iter()
        .flat_map(|(_, units)| units.units().iter().map(Unit::reference))
        .collect();
    assert_eq!(units.len(), 2);
    assert_ne!(units[0], units[1]);
    for unit in units {
        assert_eq!(
            texts.found(unit).map(|(_, text)| text),
            Some("Tides follow the moon.")
        );
    }
}

#[tokio::test]
async fn an_unreadable_inventory_fails_the_job_instead_of_deferring() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let content = vec![json!({"body":"The first section states a fact."})];
    let unreadable = model(vec![json!({"entries":[]}), json!({"entries":[]})]);
    crate::lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let error = extract_remaining(&unreadable, &content, vec![])
            .await
            .err()
            .unwrap();
        assert!(matches!(error, AppError::ModelAnswerUnusable(_)), "{error}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn lesson_internal_statements_are_nonfactual_but_the_facts_they_rely_on_are_claims(
) -> Result<()> {
    let content = vec![
        json!({"body":"In the provided code example, the handler calls UpsertOrderSummary."}),
        json!({"body":"The handler awaits SaveChangesAsync, which writes tracked changes to the database."}),
    ];
    let llm = model(vec![fixture_response(vec![
        json!({"of":"s1","claims":[],"nonFactualReason":"lesson-internal"}),
        answer_with(
            "s2",
            "u1",
            "SaveChangesAsync writes tracked changes to the database.",
        ),
    ])]);

    let inventory = extract_remaining(&llm, &content, vec![]).await?;

    assert!(inventory.units[0].claims.is_empty());
    assert_eq!(inventory.units[0].non_factual_reason, "lesson-internal");
    assert_eq!(inventory.units[1].claims.len(), 1);
    let prompts = llm.prompts.lock().unwrap();
    assert!(prompts[0].contains("with the reason \"lesson-internal\""));
    assert!(prompts[0].contains("still extract as claims any real-world fact they rely on"));
    Ok(())
}

/// An answer that locates `statement` at the sentence `sentence` of the section shown as `of`.
fn answer_with(of: &str, sentence: &str, statement: &str) -> Value {
    json!({"of":of,"claims":[{"sentenceId":sentence,"statement":statement}],"nonFactualReason":""})
}

#[tokio::test]
async fn prompt_snapshot_inventory_extraction() -> Result<()> {
    let content = vec![
        json!({"body":"The kiln in the town of Marrow heats to a set temperature before each firing."}),
        json!({"body":"The handler awaits SaveChangesAsync, which writes tracked changes to the database."}),
    ];
    let inner = model(vec![fixture_response(vec![
        answer("s1", "u1"),
        answer("s2", "u1"),
    ])]);
    let llm = crate::lessons::course_generation_tests::RequestLog::new(&inner);

    extract_remaining(&llm, &content, vec![]).await?;

    llm.assert_snapshot::<InventoryExtraction>();
    Ok(())
}

#[tokio::test]
async fn a_fact_mixed_with_example_names_is_extracted_without_them() -> Result<()> {
    let content = vec![
        json!({"body":"The Infrastructure layer implements data access interfaces defined by the Application Core, such as IOrderRepository, using technologies like EF Core."}),
    ];
    let llm = model(vec![fixture_response(vec![answer_with(
        "s1",
        "u1",
        "In Clean Architecture, the Infrastructure layer implements data access interfaces that the Application Core defines, using technologies such as EF Core.",
    )])]);

    let inventory = extract_remaining(&llm, &content, vec![]).await?;

    let statement = &inventory.units[0].claims[0].statement;
    assert!(!statement.contains("IOrderRepository"), "{statement}");
    let prompts = llm.prompts.lock().unwrap();
    assert!(prompts[0].contains("names this lesson invented for its examples"));
    assert!(
        prompts[0].contains("extract the real-world fact in general terms without those names")
    );
    Ok(())
}

#[tokio::test]
async fn dense_sections_keep_every_claim_through_reading_and_validation() -> Result<()> {
    let content = vec![json!({"body":"The measurements describe individual samples."})];
    let claims: Vec<_> = (0..64)
        .map(|i| json!({"sentenceId":"u1","statement":format!("Sample {i} has measured value {i}.")}))
        .collect();
    let llm = model(vec![fixture_response(vec![
        json!({"of":"s1","claims":claims,"nonFactualReason":""}),
    ])]);
    let inventory = extract_remaining(&llm, &content, vec![]).await?;
    assert_eq!(inventory.units[0].claims.len(), 64);
    validate_inventory(&inventory, &content).unwrap();
    Ok(())
}

#[tokio::test]
async fn a_sentence_of_another_section_is_no_place_for_a_claim() -> Result<()> {
    let content = vec![
        json!({"body":"The first section has one sentence."}),
        json!({"body":"The second section has two sentences. It is the longer one."}),
    ];
    let llm = Layered::new().on_request::<InventoryExtraction>(|sent, nth| {
        reply(answering(sent, |sentences| {
            let wrong = nth == 0 && sentences.len() == 1;
            let (id, _) = if wrong { None } else { sentences.first() }?;
            Some(vec![
                json!({"sentenceId":if wrong { "u2" } else { id.as_str() },"statement":"A scoped factual statement."}),
            ])
        }))
    });
    let inventory = extract_remaining(&llm, &content, vec![]).await?;
    assert_eq!(inventory.units.len(), 2);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(shown(&sent[0]).len(), 2);
    assert_eq!(shown(&sent[1]).len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_claims_quote_is_the_sentence_it_names_and_never_the_passage() -> Result<()> {
    let body = "Alpha holds the line. Bravo stands firm today. Charlie rests in the shade.";
    let content = vec![json!({"body":body})];
    let llm = Layered::new().on_request::<InventoryExtraction>(|sent, _| {
        reply(answering(sent, |sentences| {
            Some(vec![claim_at(
                sentences,
                "Bravo",
                "Bravo stands firm today.",
            )])
        }))
    });
    let inventory = extract_remaining(&llm, &content, vec![]).await?;
    assert_eq!(
        inventory.units[0].claims[0].quote,
        "Bravo stands firm today."
    );
    validate_inventory(&inventory, &content).unwrap();

    let history = "The Peace of Westphalia was signed in 1648 in the cities of Osnabr\u{fc}ck and M\u{fc}nster. It ended the Thirty Years' War in the Holy Roman Empire. It also closed the Eighty Years' War between Spain and the Dutch Republic. Historians often date the modern order of sovereign states from these treaties.";
    let llm = Layered::new().on_request::<InventoryExtraction>(|sent, _| {
        reply(answering(sent, |sentences| {
            Some(vec![claim_at(
                sentences,
                "Historians",
                "Signed in 1648, the Peace of Westphalia ended the Thirty Years' War and is often seen by historians as the start of the order of sovereign states.",
            )])
        }))
    });
    let inventory = extract_remaining(&llm, &[json!({"body":history})], vec![]).await?;
    assert_eq!(
        inventory.units[0].claims[0].quote,
        "Historians often date the modern order of sovereign states from these treaties."
    );
    Ok(())
}

#[tokio::test]
async fn the_claims_of_a_long_passage_point_at_sentences_and_keep_every_byte() -> Result<()> {
    let body = "\u{1f980}  code\r\n    print(' a  b ')\n".repeat(110);
    let content = vec![
        json!({"kind":"teaching","body":body}),
        json!({"body":"Other unit"}),
    ];
    let input = inputs(&content);
    let reconstructed: String = input[0]["passages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["text"].as_str().unwrap())
        .collect();
    assert_eq!(reconstructed, body);
    let llm = Layered::new()
        .on_request::<InventoryExtraction>(|sent, _| reply(answering(sent, at_the_first)));
    let inventory = extract_remaining(&llm, &content, vec![]).await?;
    assert!(claim_ledger::stands_in_part(
        &content[0],
        &inventory.units[0].claims[0].quote
    ));
    assert_eq!(inventory.units[1].claims[0].quote, "Other unit");
    Ok(())
}

/// A check's reveal names misconceptions as wrong; the extractor is told
/// so for that unit, and only for a body that is a check.
#[test]
fn a_check_tells_the_extractor_its_misconceptions_are_named_as_wrong() {
    let check = format!(
        "A sealed jar of air is carried from the beach up a mountain. What happens to its lid?\n{}\nIt bulges. If you said it caves in, you assumed the air inside loses pressure; it is the outside that does.",
        crate::generation::CHECK_REVEAL
    );
    let content = vec![
        json!({"kind":"teaching","title":"Your turn","body":check}),
        json!({"kind":"teaching","title":"The idea","body":"Pressure falls with altitude."}),
        json!({"kind":"assessment","correctIndex":0,"prompt":"Check your thinking?","options":["A","B"],"explanation":"Because."}),
    ];
    let input = inputs(&content);
    assert_eq!(input[0]["checkRole"], CHECK_ROLE);
    assert!(input[0]["passages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["text"]
            .as_str()
            .unwrap()
            .contains("If you said it caves in")));
    assert!(input[1].get("checkRole").is_none());
    assert!(input[2].get("checkRole").is_none());
}

#[test]
fn only_bodies_and_explanations_are_read_and_a_reflection_is_not() {
    let content = vec![
        json!({"kind":"teaching","role":"explanation","title":"A heading that states a fact.",
            "body":"The body states the lesson.","rubric":[{"title":"Cites","description":"A rubric line."}]}),
        json!({"kind":"teaching","role":"reflection","title":"Reflect",
            "body":"Explain from memory what the lesson taught."}),
        json!({"kind":"assessment","correctIndex":0,"prompt":"Stem?","options":["A","B"],
            "explanation":"Because the lesson says so."}),
    ];
    let input = inputs(&content);
    let fields = |unit: usize| -> Vec<&str> {
        input[unit]["passages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["field"].as_str().unwrap())
            .collect()
    };
    assert_eq!(fields(0), ["/body"]);
    assert!(fields(1).is_empty(), "a reflection holds no claims");
    assert_eq!(fields(2), ["/explanation"]);
    assert!(input[0].get("context").is_none());
}

#[test]
fn a_wrong_choices_reason_is_a_passage_of_its_own_beside_the_explanation() {
    let content = vec![
        json!({"kind":"assessment","correctIndex":0,"prompt":"Stem?",
        "options":["A","B","C","D"],
        "explanation":"A holds because the lesson says so.",
        "rationales":["","B fails for one reason.","C fails for another.","D fails for a third."]}),
    ];
    let input = inputs(&content);
    let found: Vec<(&str, &str)> = input[0]["passages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["field"].as_str().unwrap(), p["text"].as_str().unwrap()))
        .collect();
    assert_eq!(
        found,
        [
            ("/explanation", "A holds because the lesson says so."),
            ("/rationales/1", "B fails for one reason."),
            ("/rationales/2", "C fails for another."),
            ("/rationales/3", "D fails for a third."),
        ]
    );
    assert!(input[0]["context"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| !p["field"].as_str().unwrap().starts_with("/rationales")));
}

#[test]
fn an_assessments_question_and_options_are_context_and_never_passages() {
    let wrong = "Unendorsed alternative. ".repeat(150);
    let content = vec![json!({"kind":"assessment","correctIndex":1,
        "prompt":"Which result follows under the stated conditions?",
        "options":[wrong,"The selected factual answer."],
        "explanation":"An empirical reason the other answer fails."})];
    let input = inputs(&content);
    let targets = input[0]["passages"].as_array().unwrap();
    assert!(targets.iter().all(|p| p["field"] == "/explanation"));
    assert!(!targets.is_empty());
    let context = input[0]["context"].as_array().unwrap();
    let fields: Vec<&str> = context
        .iter()
        .map(|p| p["field"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["/prompt", "/options/0", "/options/1"]);
    assert_eq!(context[1]["text"], wrong);
    assert_eq!(input[0]["contextRole"], CONTEXT_ROLE);
}

#[tokio::test]
async fn a_section_with_nothing_to_read_is_not_put_to_the_model() -> Result<()> {
    let content = vec![
        json!({"kind":"teaching","role":"reflection","body":"Explain from memory."}),
        json!({"kind":"teaching","role":"explanation","body":"The second section states a fact."}),
    ];
    let llm = model(vec![fixture_response(vec![answer("s1", "u1")])]);

    let inventory = extract_remaining(&llm, &content, vec![]).await?;

    assert!(inventory.units[0].claims.is_empty());
    assert_eq!(inventory.units[0].non_factual_reason, NOTHING_TO_READ);
    assert_eq!(inventory.units[1].claims.len(), 1);
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(!prompts[0].contains("Explain from memory."));
    assert!(prompts[0].contains("The second section states a fact."));
    Ok(())
}

fn two_sections() -> Vec<Value> {
    vec![
        json!({"body":"First assertion."}),
        json!({"body":"Second assertion."}),
    ]
}

#[tokio::test]
async fn a_section_left_out_is_asked_for_again_alone_and_the_others_stand() -> Result<()> {
    let llm = Layered::new().on_request::<InventoryExtraction>(|sent, nth| {
        reply(answering(sent, |sentences| {
            let first = sentences
                .first()
                .is_some_and(|(_, text)| text.starts_with("First"));
            if nth == 0 && first {
                None
            } else {
                at_the_first(sentences)
            }
        }))
    });
    let inventory = extract(&llm, &two_sections()).await?;
    assert_eq!(inventory.units.len(), 2);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(shown(&sent[0]).len(), 2);
    let again = shown(&sent[1]);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].1[0].1, "First assertion.");
    Ok(())
}

#[tokio::test]
async fn an_unusable_entry_is_corrected_for_that_section_alone() -> Result<()> {
    let llm = Layered::new().on_request::<InventoryExtraction>(|sent, nth| {
        reply(answering(sent, |sentences| {
            let first = sentences
                .first()
                .is_some_and(|(_, text)| text.starts_with("First"));
            if nth == 0 && first {
                Some(Vec::new())
            } else {
                at_the_first(sentences)
            }
        }))
    });
    let inventory = extract(&llm, &two_sections()).await?;
    assert_eq!(inventory.units.len(), 2);
    assert_eq!(inventory.units[0].claims.len(), 1);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(shown(&sent[1]).len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_section_that_stays_unanswered_fails_the_extraction_and_the_others_are_kept() {
    let llm = Layered::new().on_request::<InventoryExtraction>(|sent, _| {
        reply(answering(sent, |sentences| {
            let first = sentences
                .first()
                .is_some_and(|(_, text)| text.starts_with("First"));
            if first {
                None
            } else {
                at_the_first(sentences)
            }
        }))
    });
    let error = extract(&llm, &two_sections()).await.err().unwrap();
    assert!(matches!(error, AppError::ModelAnswerUnusable(_)), "{error}");
    assert!(error.to_string().contains("Unit 0 needs an inventory"));
    assert!(!error.to_string().contains("Unit 1"), "{error}");
}

#[tokio::test]
async fn an_unplaced_or_conflicting_answer_cannot_approve() {
    let two_for_one = json!({"of":"s1","claims":[{"sentenceId":"u1","statement":"One statement."}],"nonFactualReason":""});
    let other = json!({"of":"s1","claims":[{"sentenceId":"u1","statement":"Another statement."}],"nonFactualReason":""});
    for response in [
        fixture_response(vec![]),
        fixture_response(vec![answer("s1", "u9")]),
        fixture_response(vec![answer("s1", "invented")]),
        fixture_response(vec![two_for_one, other]),
        fixture_response(vec![json!({"of":"s1","claims":[],"nonFactualReason":""})]),
    ] {
        assert!(extract(
            &model(vec![response.clone(), response]),
            &[json!({"body":"A fact."})]
        )
        .await
        .is_err());
    }
}

/// The passages the extractor is given for a section whose body is `body`.
fn body_passages(body: &str) -> Vec<String> {
    let input = inputs(&[json!({"body":body})]);
    input[0]["passages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["text"].as_str().unwrap().to_owned())
        .collect()
}

/// Sentence `n` of a body: 83 characters.
fn sentence(n: usize) -> String {
    format!(
        "Sentence {n:02} of the lesson states one more checkable fact about the subject at hand."
    )
}

/// `count` sentences from sentence `first` on, in one paragraph.
fn paragraph(first: usize, count: usize) -> String {
    (first..first + count)
        .map(sentence)
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_field_is_cut_on_the_latest_paragraph_that_leaves_a_passage_past_600_characters() {
    // Nine paragraphs of 335 characters, a blank line between each.
    let body = (0..9)
        .map(|k| paragraph(4 * k, 4))
        .collect::<Vec<_>>()
        .join("\n\n");
    assert_eq!(body.chars().count(), 3_031);
    let passages = body_passages(&body);
    assert_eq!(passages.concat(), body);
    let sizes: Vec<usize> = passages.iter().map(|p| p.chars().count()).collect();
    assert_eq!(sizes.len(), 3, "{sizes:?}");
    for passage in &passages[..2] {
        let size = passage.chars().count();
        assert!(
            passage.ends_with("\n\n"),
            "a paragraph ends it: {passage:?}"
        );
        assert!((602..=1_400).contains(&size), "{size}");
        assert!(
            size + 337 > 1_400,
            "the next paragraph would have fit: {size}"
        );
    }
    let input = inputs(&[json!({"body":body})]);
    let named: Vec<_> = input[0]["passages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["field"].as_str().unwrap()))
        .collect();
    assert_eq!(
        named,
        [
            ("unit-0-passage-0", "/body"),
            ("unit-0-passage-1", "/body"),
            ("unit-0-passage-2", "/body")
        ]
    );
}

#[test]
fn with_no_paragraph_past_600_characters_to_cut_on_a_passage_ends_on_a_sentence() {
    // A paragraph of 503 characters, then one of thirty sentences.
    let body = format!("{}\n\n{}", paragraph(0, 6), paragraph(6, 30));
    let passages = body_passages(&body);
    assert_eq!(passages.concat(), body);
    assert!(passages.len() >= 3, "{}", passages.len());
    for (n, passage) in passages.iter().enumerate() {
        assert!(passage.chars().count() <= 1_400, "passage {n}");
        assert!(
            passage.trim_end().ends_with('.'),
            "passage {n} ends on a sentence: {passage:?}"
        );
        assert!(
            n == 0 || passage.starts_with("Sentence"),
            "passage {n} starts on a sentence: {passage:?}"
        );
    }
    assert!(
        passages[0].chars().count() > 600,
        "the blank line at 505 is too early to cut on"
    );
}

#[test]
fn a_sentence_longer_than_the_room_is_one_passage() {
    let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
    assert_eq!(long.chars().count(), 1_712);
    let body = format!("{long} Next sentence follows here.");
    assert_eq!(
        body_passages(&body),
        [format!("{long} "), "Next sentence follows here.".to_owned()]
    );
}

#[tokio::test]
async fn a_long_statement_or_nonfactual_reason_is_read_not_refused() -> Result<()> {
    let body = "Alpha holds the line. Bravo stands firm today.";
    let content = vec![json!({"body":body})];
    let statement = format!(
        "Alpha holds the line {}today.",
        "under every condition the section states ".repeat(60)
    );
    let reason = "The section frames its scope first. ".repeat(20);
    let llm = model(vec![fixture_response(vec![
        json!({"of":"s1","claims":[{"sentenceId":"u1","statement":statement}],"nonFactualReason":reason}),
    ])]);
    let inventory = extract_remaining(&llm, &content, vec![]).await?;
    assert_eq!(inventory.units[0].claims[0].statement, statement);
    assert_eq!(inventory.units[0].non_factual_reason, reason);
    let validated = validate_inventory(&inventory, &content);
    assert!(validated.is_ok(), "{:?}", validated.as_ref().err());
    Ok(())
}

#[test]
fn the_inventory_schema_bounds_no_text_but_a_blank() {
    let input = Input {
        sections: vec![Section::of(
            0,
            &inputs(&[json!({"body":"The measurements describe individual samples."})])[0],
        )],
    };
    let schema = schema_of::<InventoryExtraction>(&input, Pointers::Enum).unwrap();
    let bounds = crate::lessons::course_generation_tests::length_bounds(schema.as_json());
    assert!(bounds.is_empty(), "{bounds:?}");
}

#[tokio::test]
async fn a_long_sentence_is_quoted_whole() -> Result<()> {
    let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
    let content = vec![json!({"body":long})];
    // The named sentence is the quote, whatever the statement says.
    for statement in [
        "Alpha beta gamma delta omega.",
        "A scoped factual statement.",
    ] {
        let llm = model(vec![fixture_response(vec![answer_with(
            "s1", "u1", statement,
        )])]);
        let inventory = extract_remaining(&llm, &content, vec![]).await?;
        assert_eq!(inventory.units[0].claims[0].quote, long, "{statement}");
        validate_inventory(&inventory, &content).unwrap();
    }
    Ok(())
}

#[test]
fn an_inventory_may_quote_a_long_contained_span() {
    let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
    let content = vec![json!({"body":long})];
    let inventory = Inventory {
        units: vec![UnitClaims {
            index: 0,
            claims: vec![Claim {
                quote: long.clone(),
                statement: "A scoped factual statement.".into(),
            }],
            non_factual_reason: String::new(),
        }],
    };
    let validated = validate_inventory(&inventory, &content);
    assert!(validated.is_ok(), "{:?}", validated.as_ref().err());
}
