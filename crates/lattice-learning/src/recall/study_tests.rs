use super::{
    super::reference_collection::tests::FakeLibrary,
    schedule::DAY_MS,
    study_dto::*,
    study_generation::{
        ChoiceCard, Citation, Claim, ClaimCards, ConversationCards, DeckPassages, Passage,
        StudyDeck,
    },
    study_repository::StudyRepository,
    study_service as service,
};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::shared::error::Result as CoreResult;
use lattice_core::shared::persistence::Database;
use serde_json::json;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

async fn open(path: std::path::PathBuf) -> lattice_core::shared::error::Result<SqlitePool> {
    lattice_core::shared::testing::sqlite::write_migrated(&path).await?;
    let options = crate::test_support::file_options(path);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    Ok(pool)
}

const PASSAGE: &str = "A petition and an appeal address different procedural issues. Study the stated conditions before selecting a procedure.";
fn source() -> StudySourceDto {
    StudySourceDto {
        chunk_id: "chunk".into(),
        document_id: uuid::Uuid::new_v4().to_string(),
        file_name: "chapter.pdf".into(),
        file_path: "/library/chapter.pdf".into(),
        excerpt: PASSAGE.into(),
        url: None,
        citation_id: 1,
        support: lattice_core::application::contracts::replies::SupportDto::cites([PASSAGE]),
    }
}
fn deck() -> StudyDeckDto {
    let id = uuid::Uuid::new_v4().to_string();
    let mut source = source();
    source.excerpt = "Study the stated conditions before selecting a procedure.".into();
    let mut card = ChoiceCard {
        question: "What should be studied before selecting a procedure?",
        answer: "The stated conditions",
        distractors: [
            "Only the title",
            "Nothing",
            "Only the filing date",
            "Only the fee",
        ]
        .map(String::from)
        .to_vec(),
        explanation: "The passage explicitly asks the reader to study the stated conditions.",
        topic: "Procedures",
    }
    .into_card(&id, 1000, source.clone(), vec![source]);
    card.options = [
        "The stated conditions",
        "Only the title",
        "Nothing",
        "Only the filing date",
        "Only the fee",
    ]
    .map(String::from)
    .to_vec();
    card.correct_index = 0;
    StudyDeckDto {
        cards: vec![card],
        id,
        title: "Procedures".into(),
        focus: "procedures".into(),
        study_goal: "Understand the process".into(),
        model_name: "test".into(),
        created_at: 1000,
        target_at: None,
    }
}
/// The last task of `request` that holds `key`, as the request shows it.
fn task_holding(request: &CompletionRequest, key: &str) -> Option<serde_json::Value> {
    request.input().iter().rev().find_map(|input| match input {
        CompletionInput::Message { content, .. } => content.split("\n\n").find_map(|chunk| {
            serde_json::from_str::<serde_json::Value>(chunk)
                .ok()
                .filter(|task| task.get(key).is_some())
        }),
        _ => None,
    })
}

/// The ids of the claims a request shows.
fn claim_ids(sent: &Sent) -> Vec<String> {
    task_holding(&sent.request, "claims")
        .and_then(|task| {
            task["claims"].as_array().map(|claims| {
                claims
                    .iter()
                    .filter_map(|claim| claim["id"].as_str().map(str::to_owned))
                    .collect()
            })
        })
        .unwrap_or_default()
}

/// The handles each passage a deck request shows labels its sentences with.
fn passage_handles(sent: &Sent) -> Vec<Vec<String>> {
    let task = task_holding(&sent.request, "passages").unwrap_or_default();
    task["passages"]
        .as_array()
        .map(|passages| {
            passages
                .iter()
                .map(|passage| {
                    passage["text"]
                        .as_str()
                        .unwrap_or_default()
                        .lines()
                        .filter_map(|line| {
                            let (handle, _) = line.strip_prefix('[')?.split_once(']')?;
                            Some(handle.to_owned())
                        })
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn reply(body: serde_json::Value) -> BoxFuture<'static, CoreResult<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(
        body.to_string(),
    ))))
}

/// A reply whose text stops where the model's length ran out.
fn cut_off(text: String) -> BoxFuture<'static, CoreResult<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse {
        finish_reason: "length".into(),
        ..CompletionResponse::from_text(text)
    })))
}

fn texts(values: &[&str]) -> Vec<serde_json::Value> {
    values.iter().map(|text| json!({"text": text})).collect()
}

const FOUR: [&str; 4] = ["Wrong A", "Wrong B", "Wrong C", "Wrong D"];

fn card_entry(id: &str, question: &str, distractors: &[&str]) -> serde_json::Value {
    json!({
        "of": id,
        "question": question,
        "distractors": texts(distractors),
        "explanation": "The cited passage supports the claim.",
        "topic": "Harbour"
    })
}

fn deck_item(question: &str, answer: &str, cites: &[String]) -> serde_json::Value {
    json!({
        "question": question,
        "answer": answer,
        "distractors": texts(&["Only the title", "Nothing", "Only the filing date", "Only the fee"]),
        "explanation": "The passage explicitly asks the reader to study the stated conditions.",
        "sourceIds": cites,
        "topic": "Procedures"
    })
}

#[tokio::test]
async fn a_study_card_rests_on_the_sentences_it_cites() {
    let model = ScriptedModel::new().on_request::<StudyDeck>(|sent, _| {
        let cite = passage_handles(sent)[0][1].clone();
        reply(json!({"items": [deck_item("What should be studied before selecting a procedure?", "The stated conditions", &[cite])]}))
    });
    let cards = super::study_generation::generate(
        &model,
        &reaction_deck(String::new()),
        &[source()],
        "deck",
        10,
    )
    .await
    .unwrap();
    assert_eq!(cards.len(), 1);
    let excerpt = "Study the stated conditions before selecting a procedure.";
    assert_eq!(cards[0].source.excerpt, excerpt);
    assert_eq!(cards[0].citations[0].excerpt, excerpt);
    assert_eq!(cards[0].source.file_name, "chapter.pdf");
    assert_eq!(cards[0].options.len(), 5);
    assert_eq!(
        cards[0].options[cards[0].correct_index],
        "The stated conditions"
    );
}

#[tokio::test]
async fn a_deck_keeps_its_usable_cards_when_one_is_invalid() {
    let model = ScriptedModel::new().on_request::<StudyDeck>(|sent, _| {
        let cite = passage_handles(sent)[0][1].clone();
        reply(json!({"items": [
            deck_item("What should be studied before selecting a procedure?", "The stated conditions", &[cite]),
            deck_item("What is not in the passage?", "An invented rule", &[])
        ]}))
    });
    let cards = super::study_generation::generate(
        &model,
        &reaction_deck(String::new()),
        &[source()],
        "deck",
        10,
    )
    .await
    .unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].answer, "The stated conditions");
}

#[tokio::test]
async fn a_study_card_citing_two_passages_or_repeating_its_answer_is_left_out() {
    let mut other = source();
    other.file_name = "harbour.pdf".into();
    other.excerpt = "The harbour gate opens before the tide turns at dawn.".into();
    let model = ScriptedModel::new().on_request::<StudyDeck>(|sent, _| {
        let handles = passage_handles(sent);
        let across = vec![handles[0][1].clone(), handles[1][0].clone()];
        let mut repeating = deck_item(
            "Which gate opens first?",
            "The harbour gate",
            &[handles[1][0].clone()],
        );
        repeating["distractors"] = json!(texts(&[
            "The harbour gate",
            "The kiln door",
            "The tide gauge",
            "The pier lamp"
        ]));
        reply(json!({"items": [
            deck_item("What should be studied before selecting a procedure?", "The stated conditions", &across),
            repeating
        ]}))
    });
    let error = super::study_generation::generate(
        &model,
        &reaction_deck(String::new()),
        &[source(), other],
        "deck",
        10,
    )
    .await
    .err();
    assert!(
        matches!(
            error,
            Some(lattice_core::shared::error::AppError::InvalidInput(_))
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_study_deck_cut_off_after_the_cards_that_closed_is_asked_for_the_rest() {
    let model = ScriptedModel::new().on_request::<StudyDeck>(|sent, nth| {
        let cite = passage_handles(sent)[0][1].clone();
        if nth == 0 {
            let first = deck_item(
                "What should be studied before selecting a procedure?",
                "The stated conditions",
                &[cite],
            );
            let text = format!("{{\"items\":[{first},{{\"question\":\"Wh");
            return cut_off(text);
        }
        reply(json!({"items": [deck_item("When is a procedure selected?", "After the stated conditions are studied", &[cite])]}))
    });
    let cards = super::study_generation::generate(
        &model,
        &reaction_deck(String::new()),
        &[source()],
        "deck",
        10,
    )
    .await
    .unwrap();
    assert_eq!(cards.len(), 2);
    let sent = model.sent();
    let again = task_holding(&sent[1].request, "passages").unwrap();
    assert_eq!(again["count"], 1);
    assert_eq!(
        again["alreadyWritten"],
        json!(["What should be studied before selecting a procedure?"])
    );
}

#[tokio::test]
async fn a_question_written_twice_makes_one_card() {
    let model = ScriptedModel::new().on_request::<StudyDeck>(|sent, _| {
        let handles = &passage_handles(sent)[0];
        reply(json!({"items": [
            deck_item("What should be studied?", "The stated conditions", &[handles[1].clone()]),
            deck_item("What should be studied?", "The conditions", &[handles[1].clone()])
        ]}))
    });
    let cards = super::study_generation::generate(
        &model,
        &reaction_deck(String::new()),
        &[source()],
        "deck",
        10,
    )
    .await
    .unwrap();
    assert_eq!(cards.len(), 1);
}

#[tokio::test]
async fn conversation_cards_preserve_every_verified_claim_and_its_citations() {
    let first = VerifiedConversationClaim {
        answer: "The first verified claim remains exact.".into(),
        citations: vec![source()],
    };
    let mut second_source = source();
    second_source.file_name = "second.pdf".into();
    let second = VerifiedConversationClaim {
        answer: "The second verified claim also remains exact.".into(),
        citations: vec![second_source],
    };
    let model = ScriptedModel::new().on_request::<ConversationCards>(|sent, _| {
        let entries: Vec<_> = claim_ids(sent)
            .iter()
            .rev()
            .map(|id| card_entry(id, &format!("What is true of {id}?"), &FOUR))
            .collect();
        reply(json!({ "entries": entries }))
    });
    let cards =
        super::study_generation::generate_from_conversation(&model, &[first, second], "deck", 10)
            .await
            .unwrap();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].answer, "The first verified claim remains exact.");
    assert_eq!(
        cards[1].answer,
        "The second verified claim also remains exact."
    );
    assert_eq!(cards[1].citations[0].file_name, "second.pdf");
    assert!(cards
        .iter()
        .all(|card| card.options.len() == 5 && card.options[card.correct_index] == card.answer));
}

#[tokio::test]
async fn a_conversation_card_out_of_shape_is_left_out_and_the_others_kept() {
    let claims: Vec<_> = (0..2).map(|i| reaction_claim(i, 1)).collect();
    let model = ScriptedModel::new().on_request::<ConversationCards>(|sent, _| {
        let entries: Vec<_> = claim_ids(sent)
            .iter()
            .map(|id| {
                let three = ["A", "B", "C"];
                card_entry(
                    id,
                    "When is equilibrium reached?",
                    if id == "c1" { &three[..] } else { &FOUR[..] },
                )
            })
            .collect();
        reply(json!({ "entries": entries }))
    });
    let cards = super::study_generation::generate_from_conversation(&model, &claims, "deck", 10)
        .await
        .unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].answer, claims[1].answer);
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(claim_ids(&sent[1]), ["c1"]);
}

#[tokio::test]
async fn a_claim_the_answer_leaves_out_is_asked_for_again_and_the_others_stand() {
    let claims: Vec<_> = (0..3).map(|i| reaction_claim(i, 1)).collect();
    let model = ScriptedModel::new().on_request::<ConversationCards>(|sent, nth| {
        let ids = claim_ids(sent);
        let mut entries: Vec<_> = if nth == 0 {
            ids.iter()
                .take(1)
                .map(|id| card_entry(id, "When is equilibrium reached?", &FOUR))
                .collect()
        } else {
            ids.iter()
                .map(|id| card_entry(id, &format!("What does {id} state?"), &FOUR))
                .collect()
        };
        if nth == 0 {
            entries.push(card_entry("c9", "Which claim is this?", &FOUR));
        }
        reply(json!({ "entries": entries }))
    });
    let cards = super::study_generation::generate_from_conversation(&model, &claims, "deck", 10)
        .await
        .unwrap();
    assert_eq!(cards.len(), 3);
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(claim_ids(&sent[1]), ["c2", "c3"]);
}

#[tokio::test]
async fn a_distractor_repeating_the_claim_leaves_the_claim_without_a_card() {
    let claims = vec![reaction_claim(0, 1)];
    let answer = claims[0].answer.clone();
    let model = ScriptedModel::new().on_request::<ConversationCards>(move |sent, _| {
        let entries: Vec<_> = claim_ids(sent)
            .iter()
            .map(|id| {
                card_entry(
                    id,
                    "When is equilibrium reached?",
                    &[answer.as_str(), "B", "C", "D"],
                )
            })
            .collect();
        reply(json!({ "entries": entries }))
    });
    let error = super::study_generation::generate_from_conversation(&model, &claims, "deck", 10)
        .await
        .err();
    assert!(
        matches!(
            error,
            Some(lattice_core::shared::error::AppError::InvalidInput(_))
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_card_is_read_at_any_length() {
    let excerpt = "Study the stated conditions before selecting a procedure. ".repeat(30);
    let question = "What should be studied before selecting a procedure? ".repeat(50);
    let explanation =
        "The passage explicitly asks the reader to study the stated conditions. ".repeat(50);
    let topic = "Procedures ".repeat(12);
    let option = "Only the title of the document, and nothing else in it. ".repeat(20);
    let mut source = source();
    source.excerpt = excerpt.clone();
    let model = ScriptedModel::new().on_request::<StudyDeck>({
        let (question, explanation, topic, option) = (
            question.clone(),
            explanation.clone(),
            topic.clone(),
            option.clone(),
        );
        move |sent, _| {
            let cites = passage_handles(sent)[0].clone();
            let mut item = deck_item(&question, "The stated conditions", &cites);
            item["explanation"] = json!(explanation);
            item["topic"] = json!(topic);
            item["distractors"] = json!(texts(&[
                option.as_str(),
                "Nothing",
                "Only the filing date",
                "Only the fee"
            ]));
            reply(json!({"items": [item]}))
        }
    });
    let cards = super::study_generation::generate(
        &model,
        &reaction_deck(String::new()),
        std::slice::from_ref(&source),
        "deck",
        10,
    )
    .await
    .unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].question, question.trim());
    assert_eq!(cards[0].explanation, explanation.trim());
    assert_eq!(cards[0].topic, topic.trim());
    assert!(cards[0]
        .options
        .iter()
        .any(|choice| choice == option.trim()));
    assert_eq!(cards[0].source.excerpt, excerpt.trim());
}

#[tokio::test]
async fn a_conversation_card_is_read_at_any_length() {
    let claim = VerifiedConversationClaim {
        answer: "The first verified claim remains exact.".into(),
        citations: vec![source()],
    };
    let question = "What is true of the first claim? ".repeat(70);
    let distractor = "A plausible but false statement about it. ".repeat(30);
    let explanation = "The cited passage supports the first claim. ".repeat(80);
    let topic = "First claim ".repeat(10);
    let model = ScriptedModel::new().on_request::<ConversationCards>({
        let (question, distractor, explanation, topic) = (
            question.clone(),
            distractor.clone(),
            explanation.clone(),
            topic.clone(),
        );
        move |sent, _| {
            let entries: Vec<_> = claim_ids(sent)
                .iter()
                .map(|id| {
                    let mut entry = card_entry(
                        id,
                        &question,
                        &[distractor.as_str(), "Wrong B", "Wrong C", "Wrong D"],
                    );
                    entry["explanation"] = json!(explanation);
                    entry["topic"] = json!(topic);
                    entry
                })
                .collect();
            reply(json!({ "entries": entries }))
        }
    });
    let cards = super::study_generation::generate_from_conversation(&model, &[claim], "deck", 10)
        .await
        .unwrap();
    assert_eq!(cards.len(), 1);
    let card = &cards[0];
    assert_eq!(card.question, question.trim());
    assert_eq!(card.explanation, explanation.trim());
    assert_eq!(card.topic, topic.trim());
    assert!(card
        .options
        .iter()
        .any(|option| option == distractor.trim()));
}

#[test]
fn the_card_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = ClaimCards {
        claims: vec![Claim {
            at: marks.reference(100_001_usize),
            answer: marks.told("The harbour gate opens at first light.".to_owned()),
            citations: vec![Citation {
                document: marks.told("tide-table.md".to_owned()),
                excerpt: marks.told("The gate opens when the tide has turned.".to_owned()),
            }],
        }],
    };
    holds::<ConversationCards>(&input, &marks);
}

#[test]
fn the_study_deck_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = DeckPassages {
        count: marks.told(3_usize),
        focus: marks.told("harbour tide tables".to_owned()),
        study_goal: marks.told("Read the tide tables unaided".to_owned()),
        passages: Packed::new(
            vec![Passage {
                document: marks.told("tide-table.md".to_owned()),
                text: marks.told("Tide tables are revised each spring.".to_owned()),
            }],
            |_| 1,
        ),
        written: vec![marks.told("When are tide tables revised?".to_owned())],
    };
    holds::<StudyDeck>(&input, &marks);
}

#[test]
fn study_rejects_unknown_persisted_card_formats() {
    assert!(super::study_repository::parse_card_format("future_format").is_err());
}
#[tokio::test]
async fn decks_reviews_edits_and_deletion_persist_atomically() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let pool = open(dir.path().join("study.db")).await?;
    let db = Database::new(pool.clone());
    let repo = StudyRepository::new(db.clone());
    let d = deck();
    repo.save(&d).await?;
    let c = &d.cards[0];
    sqlx::query("UPDATE study_cards SET source_json=? WHERE id=?")
        .bind(serde_json::to_string(&c.source)?)
        .bind(&c.id)
        .execute(&pool)
        .await?;
    assert_eq!(
        repo.get(&d.id).await?.cards[0].citations.len(),
        1,
        "legacy single-source cards still open"
    );
    let request = ReviewStudyCardRequestDto {
        review_id: uuid::Uuid::new_v4().to_string(),
        card_id: c.id.clone(),
        expected_reviews: 0,
        selected_option: Some(1),
        rating: StudyRating::Easy,
    };
    let reviewed = repo.review(&request, 2000).await?;
    assert_eq!((reviewed.review_count, reviewed.lapses), (1, 1));
    assert!(
        reviewed.interval_days >= 1,
        "FSRS schedules a wrong answer for a later day"
    );
    assert_eq!(reviewed.due_at, 2000 + reviewed.interval_days * DAY_MS);
    assert_eq!(
        repo.review(&request, 3000).await?.review_count,
        1,
        "retry is idempotent"
    );
    let mut changed_retry = request.clone();
    changed_retry.selected_option = Some(0);
    assert!(repo.review(&changed_retry, 3000).await.is_err());
    let mut stale = request.clone();
    stale.review_id = uuid::Uuid::new_v4().to_string();
    assert!(repo.review(&stale, 3000).await.is_err());
    let summary = repo.list(3000).await?;
    assert_eq!(
        (
            summary[0].due_count,
            summary[0].quiz_attempts,
            summary[0].quiz_correct
        ),
        (0, 1, 0)
    );
    assert_eq!(repo.list(reviewed.due_at).await?[0].due_count, 1);
    let mut correct = stale;
    correct.expected_reviews = 1;
    correct.selected_option = Some(0);
    let recalled = repo.review(&correct, reviewed.due_at).await?;
    assert!(recalled.due_at > reviewed.due_at);
    let edit = UpdateStudyCardRequestDto {
        card_id: c.id.clone(),
        question: "Revised question".into(),
        answer: "The complete stated conditions".into(),
        explanation: "Revised explanation".into(),
    };
    service::update_card(&repo, edit).await?;
    let reopened = StudyRepository::new(db.clone()).get(&d.id).await?;
    assert_eq!(
        reopened.cards[0].options[0],
        "The complete stated conditions"
    );
    assert_eq!(reopened.study_goal, "Understand the process");
    assert_eq!(reopened.cards[0].review_count, 2);
    assert_eq!(repo.list(reviewed.due_at + 1).await?[0].quiz_correct, 1);
    repo.delete_deck(&d.id).await?;
    assert!(repo.get(&d.id).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM study_reviews")
            .fetch_one(&pool)
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn a_card_is_edited_at_any_length() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let pool = open(dir.path().join("long-edit.db")).await?;
    let db = Database::new(pool.clone());
    let repo = StudyRepository::new(db.clone());
    let d = deck();
    repo.save(&d).await?;
    let edit = UpdateStudyCardRequestDto {
        card_id: d.cards[0].id.clone(),
        question: "Which condition does the passage ask the reader to study first? ".repeat(40),
        answer: "The complete stated conditions of the procedure, read in full. ".repeat(25),
        explanation: "The passage says the reader studies them before choosing. ".repeat(60),
    };
    service::update_card(&repo, edit.clone()).await?;
    let reopened = repo.get(&d.id).await?;
    assert_eq!(reopened.cards[0].question, edit.question.trim());
    assert_eq!(reopened.cards[0].explanation, edit.explanation.trim());
    assert_eq!(reopened.cards[0].options[0], edit.answer.trim());
    Ok(())
}

#[tokio::test]
async fn question_answer_reviews_reject_choices_and_replay_only_matching_payloads(
) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let pool = open(dir.path().join("qa-study.db")).await?;
    let db = Database::new(pool.clone());
    let repo = StudyRepository::new(db.clone());
    let mut d = deck();
    let card = d
        .cards
        .first_mut()
        .ok_or_else(|| anyhow::anyhow!("fixture card missing"))?;
    card.format = StudyCardFormat::QuestionAnswer;
    card.options.clear();
    card.correct_index = 0;
    repo.save(&d).await?;
    let card_id = d
        .cards
        .first()
        .ok_or_else(|| anyhow::anyhow!("fixture card missing"))?
        .id
        .clone();
    let review_id = uuid::Uuid::new_v4().to_string();
    let request = ReviewStudyCardRequestDto {
        review_id: review_id.clone(),
        card_id: card_id.clone(),
        expected_reviews: 0,
        selected_option: None,
        rating: StudyRating::Good,
    };
    let mut quiz_request = request.clone();
    quiz_request.selected_option = Some(0);
    assert!(repo.review(&quiz_request, 10_000).await.is_err());
    let first = repo.review(&request, 10_000).await?;
    assert_eq!(
        (first.format, first.options.len(), first.review_count),
        (StudyCardFormat::QuestionAnswer, 0, 1)
    );
    let replay = repo.review(&request, 20_000).await?;
    assert_eq!(replay.review_count, 1);
    assert_eq!(replay.due_at, first.due_at);
    let mut mismatched = request;
    mismatched.rating = StudyRating::Easy;
    assert!(repo.review(&mismatched, 20_000).await.is_err());
    Ok(())
}

#[tokio::test]
async fn conversation_claims_keep_verified_answers_and_resolve_their_citations(
) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let pool = open(dir.path().join("conversation-study.db")).await?;
    let db = Database::new(pool.clone());
    let repo = StudyRepository::new(db.clone());
    let conversation_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO conversations (id,title) VALUES (?,'Research notes')")
        .bind(&conversation_id)
        .execute(&pool)
        .await?;
    let source = |number: i64, name: &str, content: &str| {
        json!({
            "documentId": format!("document-{number}"),
            "chunkId": format!("chunk-{number}"),
            "content": content,
            "score": 0.9,
            "path": null,
            "position": null,
            "fileName": name,
            "filePath": format!("/library/{name}"),
            "mimeType": "text/plain",
            "category": "Text File",
            "fileSizeBytes": 100,
            "modifiedAt": "2026-09-16T00:00:00Z",
            "excerpt": content,
            "citationId": number
        })
    };
    let metadata = json!({
        "sources": [
            source(1, "alpha.txt", "Alpha evidence establishes the first supported fact."),
            source(2, "beta.txt", "Beta evidence establishes the second supported fact.")
        ],
        "verification": {
            "enabled": true,
            "claimsEvaluated": 3,
            "supportedClaims": 2,
            "supportedClaimNotes": [
                "The second supported fact follows from beta evidence.",
                "The first supported fact follows from alpha evidence."
            ],
            "unsupportedClaims": ["An unsupported assertion."]
        }
    });
    sqlx::query("INSERT INTO conversation_messages (id,conversation_id,role,content,metadata,status) VALUES (?,?, 'assistant', ?, ?, 'completed')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&conversation_id)
        .bind("The second supported fact follows from beta evidence [2]. The first supported fact follows from alpha evidence [1]. An unsupported assertion [1].")
        .bind(metadata.to_string())
        .execute(&pool)
        .await?;

    let claims = repo.conversation_claims(&conversation_id).await?;
    assert_eq!(claims.len(), 2);
    assert_eq!(
        claims[0].answer,
        "The second supported fact follows from beta evidence."
    );
    assert_eq!(claims[0].citations[0].file_name, "beta.txt");
    assert_eq!(claims[1].citations[0].file_name, "alpha.txt");
    assert!(claims
        .iter()
        .all(|claim| !claim.answer.contains("unsupported")));
    Ok(())
}
#[tokio::test]
async fn passages_stay_within_selected_documents_and_focus() -> anyhow::Result<()> {
    let selected = uuid::Uuid::new_v4().to_string();
    let other = uuid::Uuid::new_v4().to_string();
    let chunks: Vec<&str> = (0..40)
        .map(|i| {
            if i == 30 {
                PASSAGE
            } else {
                "Another passage about disclosure."
            }
        })
        .collect();
    let library = FakeLibrary::default()
        .with_document(&selected, &chunks)
        .with_document(&other, &chunks);
    let sources = service::passages(&library, std::slice::from_ref(&selected), "petition").await?;
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].document_id, selected);
    assert_eq!(sources[0].excerpt, PASSAGE);
    assert!(
        service::passages(&library, std::slice::from_ref(&selected), "unmatchedxyz")
            .await
            .is_err()
    );
    // Without a focus every chunk is offered, the middle first and any
    // prefix spread across the document; the deck's window decides how many.
    let sampled = service::passages(&library, std::slice::from_ref(&selected), "").await?;
    assert_eq!(sampled.len(), 40);
    assert_eq!(sampled[0].excerpt, chunks[20]);
    assert_eq!(service::spread(7), [3, 1, 5, 0, 2, 4, 6]);
    assert!(sampled.iter().all(|source| source.document_id == selected));
    assert!(
        service::passages(&library, &[uuid::Uuid::new_v4().to_string()], "")
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn passages_are_offered_one_document_after_another() {
    let mut first = source();
    first.document_id = "first".into();
    let mut second = source();
    second.document_id = "second".into();
    let sources = vec![first.clone(), first, second];
    let ordered = super::study_generation::by_document_turns(&sources);
    assert_eq!(
        ordered
            .iter()
            .map(|s| s.document_id.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "first"]
    );
}

#[tokio::test]
async fn the_consolidation_migration_moves_every_card_onto_fsrs() -> anyhow::Result<()> {
    const CONSOLIDATION: &str = "20261009150000";
    let dir = tempfile::tempdir()?;
    let earlier = dir.path().join("migrations");
    std::fs::create_dir(&earlier)?;
    for entry in std::fs::read_dir(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../lattice-desktop/migrations"
    ))? {
        let path = entry?.path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.as_str() < CONSOLIDATION {
            std::fs::copy(&path, earlier.join(&name))?;
        }
    }
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await?;
    let db = Database::new(pool.clone());
    sqlx::migrate::Migrator::new(earlier.as_path())
        .await?
        .run(&pool)
        .await?;
    for statement in [
        "INSERT INTO learning_programs(id,title,goal,status,revision,prior_knowledge,minutes_per_session,model_name,created_at) VALUES('program','Recall','Remember','active',1,'',30,'model',0)",
        "INSERT INTO study_decks(id,title,focus,study_goal,model_name,created_at) VALUES('deck','Deck','Focus','','model',0)",
        "INSERT INTO study_cards(id,deck_id,question,answer,options_json,correct_index,explanation,source_json,topic,due_at,interval_days,review_count,lapses,format,scheduler_version) VALUES('expanding','deck','Q','A','[]',0,'E','{\"chunkId\":\"c\",\"documentId\":\"d\",\"fileName\":\"f\",\"filePath\":\"p\",\"excerpt\":\"e\"}','t',5000,3,2,1,'question_answer','expanding_v1')",
        "INSERT INTO study_cards(id,deck_id,question,answer,options_json,correct_index,explanation,source_json,topic,due_at,interval_days,review_count,lapses,format,scheduler_version) VALUES('fsrs','deck','Q','A','[]',0,'E','{}','t',9000,2,1,0,'question_answer','fsrs_6_v1')",
        "INSERT INTO study_reviews(id,card_id,reviewed_at,rating,mode,correct,selected_option,scheduler_version) VALUES('r1','expanding',1000,'good','flashcard',NULL,NULL,'expanding_v1'),('r2','expanding',2000,'again','flashcard',NULL,NULL,'expanding_v1')",
        "INSERT INTO learning_recall_card_profiles(card_id,program_id,format,prompt_json,answer_json,source_version_ids_json,scheduler_version,scheduler_state_json,content_revision,created_at,updated_at) VALUES('fsrs','program','question_answer','\"Q\"','{}','[]','fsrs_6_v1','{\"version\":\"fsrs_6_v1\",\"stability\":4.5,\"difficulty\":5.25,\"last_reviewed_at\":7000,\"due_at\":9000,\"interval_days\":2,\"review_count\":1}',1,0,0)",
    ] {
        sqlx::query(statement).execute(&pool).await?;
    }
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await?;

    let card = |id: &'static str| {
        sqlx::query_as::<_, (Option<f64>, Option<f64>, Option<i64>, i64, i64, i64)>(
            "SELECT stability,difficulty,last_reviewed_at,due_at,review_count,lapses FROM study_cards WHERE id=?",
        )
        .bind(id)
        .fetch_one(&pool)
    };
    // The retired scheduler's card starts FSRS as a new card, keeping its
    // due date, counts and history.
    assert_eq!(
        card("expanding").await?,
        (None, None, Some(2000), 5000, 2, 1)
    );
    // A card already on FSRS keeps its memory.
    assert_eq!(
        card("fsrs").await?,
        (Some(4.5), Some(5.25), Some(7000), 9000, 1, 0)
    );
    let reviews: i64 =
        sqlx::query_scalar("SELECT count(*) FROM study_reviews WHERE card_id='expanding'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(reviews, 2);
    let scheduler_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('study_cards') WHERE name='scheduler_version'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(scheduler_columns, 0);

    let reviewed = StudyRepository::new(db.clone())
        .review(
            &ReviewStudyCardRequestDto {
                review_id: uuid::Uuid::new_v4().to_string(),
                card_id: "expanding".into(),
                expected_reviews: 2,
                selected_option: None,
                rating: StudyRating::Good,
            },
            6000,
        )
        .await?;
    assert_eq!(reviewed.review_count, 3);
    assert!(card("expanding").await?.0.is_some());
    Ok(())
}

/// Counts exactly at two characters a token where the estimate assumes
/// four, as on reaction notation, and refuses, as the scheduler's gate does,
/// a prompt its exact count says fills the window. Answers every flashcard
/// or deck request it accepts with cards built from that request.
struct Notation {
    calls: std::sync::Mutex<Vec<serde_json::Value>>,
    requests: std::sync::Mutex<Vec<lattice_core::application::ports::llm_port::CompletionRequest>>,
    refusals: std::sync::atomic::AtomicUsize,
    /// Whether the next flashcard request's first card is out of shape.
    spoiled: std::sync::atomic::AtomicBool,
}

const NOTATION_WINDOW: usize = 8_192;

fn notation() -> Notation {
    Notation {
        calls: std::sync::Mutex::new(Vec::new()),
        requests: std::sync::Mutex::new(Vec::new()),
        refusals: std::sync::atomic::AtomicUsize::new(0),
        spoiled: std::sync::atomic::AtomicBool::new(false),
    }
}

fn exact(text: &str) -> usize {
    text.chars().count() / 2
}

#[async_trait::async_trait]
impl lattice_core::application::ports::LLMPort for Notation {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> lattice_core::shared::error::Result<
        lattice_core::application::ports::llm_port::CompletionResponse,
    > {
        use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        let rendered = request
            .input()
            .iter()
            .flat_map(|item| match item {
                CompletionInput::Message { role, content } => vec![role.clone(), content.clone()],
                _ => Vec::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        if exact(&rendered) >= NOTATION_WINDOW {
            self.refusals
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(lattice_core::shared::error::AppError::PromptExceedsWindow {
                prompt_tokens: exact(&rendered),
                window: NOTATION_WINDOW,
                counted_exactly: true,
                message: "over the window".into(),
            });
        }
        let prompt = task_holding(request, "claims")
            .or_else(|| task_holding(request, "passages"))
            .ok_or_else(|| {
                lattice_core::shared::error::AppError::InternalError("no task".into())
            })?;
        let reply = match prompt["claims"].as_array() {
            Some(claims) => {
                let spoiled = self
                    .spoiled
                    .swap(false, std::sync::atomic::Ordering::SeqCst);
                json!({"entries": claims.iter().enumerate().map(|(i, claim)| json!({
                    "of": claim["id"],
                    "question": format!("Which condition does claim {i} state for equilibrium?"),
                    "distractors": if spoiled && i == 0 {
                        vec!["ΔG° doubles", "Fe₂O₃ melts", "CO₂ freezes"]
                    } else {
                        vec!["ΔG° doubles", "Fe₂O₃ melts", "CO₂ freezes", "Fe dissolves"]
                    }.into_iter().map(|text| json!({"text": text})).collect::<Vec<_>>(),
                    "explanation": "The citation states the condition.",
                    "topic": "Equilibrium"
                })).collect::<Vec<_>>()})
            }
            None => {
                let text = prompt["passages"][0]["text"].as_str().unwrap_or_default();
                let handle = text
                    .strip_prefix('[')
                    .and_then(|rest| rest.split_once(']'))
                    .map_or("", |(handle, _)| handle);
                json!({"items":[{"question":"When does iron oxide reduction reach equilibrium?","answer":"When ΔG° falls to zero","distractors":[{"text":"When ΔG° doubles"},{"text":"When Fe₂O₃ melts"},{"text":"When CO₂ freezes"},{"text":"When Fe dissolves"}],"explanation":"The passage states equilibrium is reached when ΔG° falls to zero.","sourceIds":[handle],"topic":"Equilibrium"}]})
            }
        };
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(prompt);
        }
        Ok(CompletionResponse::from_text(reply.to_string()))
    }
    fn model_name(&self) -> &str {
        "notation"
    }
    fn max_context_tokens(&self) -> usize {
        NOTATION_WINDOW
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
    async fn count_tokens_exact(&self, text: &str) -> lattice_core::shared::error::Result<usize> {
        Ok(exact(text))
    }
    fn counts_tokens_exactly(&self) -> bool {
        true
    }
    async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
        Ok(true)
    }
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

/// About 1,100 characters of reaction notation, `sentences` of them more.
fn reaction_notes(i: usize, sentences: usize) -> String {
    (0..sentences)
        .map(|k| format!("In trial {i}-{k}, iron oxide reduction Fe₂O₃ + 3CO → 2Fe + 3CO₂ reaches equilibrium when ΔG° falls to zero."))
        .collect::<Vec<_>>()
        .join(" ")
}

fn reaction_source(i: usize, sentences: usize) -> StudySourceDto {
    let excerpt = reaction_notes(i, sentences);
    StudySourceDto {
        chunk_id: format!("chunk-{i}"),
        document_id: format!("run-{i}"),
        file_name: format!("run-{i}.md"),
        file_path: format!("/library/run-{i}.md"),
        support: lattice_core::application::contracts::replies::SupportDto::cites([
            excerpt.as_str()
        ]),
        excerpt,
        url: None,
        citation_id: 1,
    }
}

fn reaction_claim(i: usize, sentences: usize) -> super::study_dto::VerifiedConversationClaim {
    super::study_dto::VerifiedConversationClaim {
        answer: format!(
            "Claim {i}: iron oxide reduction reaches equilibrium when ΔG° falls to zero."
        ),
        citations: vec![reaction_source(i, sentences)],
    }
}

#[tokio::test]
async fn claims_twice_as_dense_as_estimated_are_split_until_each_request_fits() {
    let claims: Vec<_> = (0..8).map(|i| reaction_claim(i, 11)).collect();
    let llm = notation();
    let cards = super::study_generation::generate_from_conversation(&llm, &claims, "deck", 10)
        .await
        .unwrap();
    // Every claim keeps its card, in order, though no request held all eight.
    assert_eq!(cards.len(), 8);
    assert!(cards
        .iter()
        .zip(&claims)
        .all(|(card, claim)| card.answer == claim.answer && card.deck_id == "deck"));
    let calls = llm.calls.lock().unwrap();
    assert!(
        calls.len() > 1,
        "the eight claims needed more than one request"
    );
    assert_eq!(llm.refusals.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_claim_whose_card_came_back_out_of_shape_is_asked_for_again() {
    let claims: Vec<_> = (0..3).map(|i| reaction_claim(i, 1)).collect();
    let llm = notation();
    llm.spoiled.store(true, std::sync::atomic::Ordering::SeqCst);
    let cards = super::study_generation::generate_from_conversation(&llm, &claims, "deck", 10)
        .await
        .unwrap();
    let mut answers: Vec<_> = cards.iter().map(|card| card.answer.clone()).collect();
    answers.sort();
    assert_eq!(
        answers,
        claims
            .iter()
            .map(|claim| claim.answer.clone())
            .collect::<Vec<_>>()
    );
    let calls = llm.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1]["claims"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn one_claim_the_window_cannot_hold_by_exact_count_names_the_window() {
    // Within the window by the estimate, over it by the exact count.
    let claims = vec![reaction_claim(0, 150)];
    let llm = notation();
    let error = super::study_generation::generate_from_conversation(&llm, &claims, "deck", 10)
        .await
        .err();
    let Some(lattice_core::shared::error::AppError::WindowTooSmall(message)) = error else {
        panic!("expected the window to be named, got {error:?}");
    };
    assert!(
        message.contains("notation's 8192-token context window"),
        "{message}"
    );
    assert!(llm.calls.lock().unwrap().is_empty());
}

fn requests_of(
    llm: &Notation,
    id: &str,
) -> Vec<lattice_core::application::ports::llm_port::CompletionRequest> {
    llm.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| {
            request
                .contract()
                .is_some_and(|call| call.id.as_str() == id)
        })
        .cloned()
        .collect()
}

#[tokio::test]
async fn prompt_snapshot_recall_conversation_cards() {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let claims: Vec<_> = (0..3).map(|i| reaction_claim(i, 1)).collect();
    let llm = notation();
    super::study_generation::generate_from_conversation(&llm, &claims, "deck", 10)
        .await
        .unwrap();
    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "recall-conversation-cards",
        &requests_of(&llm, "recall-conversation-cards"),
    );
}

#[tokio::test]
async fn prompt_snapshot_recall_study_deck() {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let sources: Vec<_> = (0..2).map(|i| reaction_source(i, 1)).collect();
    let llm = notation();
    super::study_generation::generate(
        &llm,
        &reaction_deck("Understand the equilibrium condition".into()),
        &sources,
        "deck",
        10,
    )
    .await
    .unwrap();
    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "recall-study-deck",
        &requests_of(&llm, "recall-study-deck"),
    );
}

fn reaction_deck(study_goal: String) -> GenerateStudyDeckRequestDto {
    GenerateStudyDeckRequestDto {
        title: "Iron".into(),
        document_ids: vec![],
        focus: "iron oxide reduction".into(),
        study_goal,
        count: 2,
    }
}

#[tokio::test]
async fn deck_passages_twice_as_dense_as_estimated_are_fewer_but_sent() {
    let sources: Vec<_> = (0..16).map(|i| reaction_source(i, 11)).collect();
    let llm = notation();
    let cards = super::study_generation::generate(
        &llm,
        &reaction_deck(String::new()),
        &sources,
        "deck",
        10,
    )
    .await
    .unwrap();
    assert_eq!(cards.len(), 1);
    let calls = llm.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let carried = calls[0]["passages"].as_array().map_or(0, Vec::len);
    assert!(carried > 0 && carried < sources.len(), "{carried} passages");
    assert_eq!(llm.refusals.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_deck_whose_questions_leave_no_room_for_a_passage_is_refused_unsent() {
    let sources: Vec<_> = (0..4).map(|i| reaction_source(i, 11)).collect();
    // The learner's goal is within the window by the estimate, over it by
    // the exact count.
    let goal = "Fe₂O₃ ⇌ 2Fe³⁺ + 3O²⁻ ".repeat(700);
    let llm = notation();
    let error = super::study_generation::generate(&llm, &reaction_deck(goal), &sources, "deck", 10)
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(error.contains("Try fewer cards"), "{error}");
    assert!(llm.calls.lock().unwrap().is_empty());
}
