#![cfg(test)]
use super::{
    dto::*,
    generation::{parse_cards, parse_conversation_cards},
    repository::StudyRepository,
    schedule::next_review,
    service,
};
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
use serde_json::json;

const PASSAGE: &str = "A petition and an appeal address different procedural issues. Study the stated conditions before selecting a procedure.";
fn source() -> StudySourceDto {
    StudySourceDto {
        chunk_id: "chunk".into(),
        document_id: uuid::Uuid::new_v4().to_string(),
        file_name: "chapter.pdf".into(),
        file_path: "/library/chapter.pdf".into(),
        excerpt: PASSAGE.into(),
    }
}
fn generated() -> serde_json::Value {
    json!({"cards":[{"question":"What should be studied before selecting a procedure?","options":["The stated conditions","Only the title","Nothing","Only the filing date","Only the fee"],"correctIndex":0,"explanation":"The passage explicitly asks the reader to study the stated conditions.","sourceIndex":0,"quote":"Study the stated conditions before selecting a procedure.","topic":"Procedures"}]})
}
fn deck() -> StudyDeckDto {
    let id = uuid::Uuid::new_v4().to_string();
    StudyDeckDto {
        cards: parse_cards(&generated().to_string(), &[source()], &id, 6, 1000).unwrap(),
        id,
        title: "Procedures".into(),
        focus: "procedures".into(),
        study_goal: "Understand the process".into(),
        model_name: "test".into(),
        created_at: 1000,
    }
}
#[test]
fn questions_require_an_exact_source_and_an_unambiguous_key() {
    let s = source();
    assert_eq!(
        parse_cards(
            &format!("```json\n{}\n```", generated()),
            std::slice::from_ref(&s),
            "deck",
            6,
            0
        )
        .unwrap()
        .len(),
        1
    );
    for (field, value) in [
        (
            "quote",
            json!("An invented rule not supported by the document passage."),
        ),
        ("sourceIndex", json!(42)),
        ("correctIndex", json!(5)),
        ("options", json!(["A", "A", "B", "C", "D"])),
    ] {
        let mut invalid = generated();
        invalid["cards"][0][field] = value;
        assert!(
            parse_cards(&invalid.to_string(), std::slice::from_ref(&s), "deck", 6, 0,).is_err(),
            "{field}"
        );
    }
}

#[test]
fn conversation_cards_preserve_every_verified_claim_and_its_citations() {
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
    let response = json!({"cards":[
        {"claimIndex":1,"question":"What is true of the second claim?","distractors":["False A","False B","False C","False D"],"explanation":"The cited passage supports the second claim.","topic":"Second"},
        {"claimIndex":0,"question":"What is true of the first claim?","distractors":["Wrong A","Wrong B","Wrong C","Wrong D"],"explanation":"The cited passage supports the first claim.","topic":"First"}
    ]});
    let cards =
        parse_conversation_cards(&response.to_string(), &[first, second], "deck", 10).unwrap();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].answer, "The first verified claim remains exact.");
    assert_eq!(
        cards[1].answer,
        "The second verified claim also remains exact."
    );
    assert_eq!(cards[1].citations[0].file_name, "second.pdf");
    assert!(cards.iter().all(|card| card.options.len() == 5));
}
#[test]
fn review_schedule_uses_recall_and_bounds_intervals() {
    assert_eq!(next_review(StudyRating::Again, 20, 1000), (601000, 0));
    assert_eq!(next_review(StudyRating::Good, 0, 0), (86400000, 1));
    assert_eq!(next_review(StudyRating::Easy, 0, 0), (345600000, 4));
    assert_eq!(next_review(StudyRating::Good, 6, 0).1, 12);
    assert_eq!(next_review(StudyRating::Easy, i64::MAX, 0).1, 365);
}
#[tokio::test]
async fn decks_reviews_edits_and_deletion_persist_atomically() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = DatabaseConnection::new(dir.path().join("study.db")).await?;
    initialize_database(db.pool()).await?;
    let repo = StudyRepository::new(db.pool().clone());
    let d = deck();
    repo.save(&d).await?;
    let c = &d.cards[0];
    sqlx::query("UPDATE study_cards SET source_json=? WHERE id=?")
        .bind(serde_json::to_string(&c.source)?)
        .bind(&c.id)
        .execute(db.pool())
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
    assert_eq!(
        (reviewed.review_count, reviewed.lapses, reviewed.due_at),
        (1, 1, 602000)
    );
    assert_eq!(
        repo.review(&request, 3000).await?.review_count,
        1,
        "retry is idempotent"
    );
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
    assert_eq!(repo.list(603000).await?[0].due_count, 1);
    let mut correct = stale;
    correct.expected_reviews = 1;
    correct.selected_option = Some(0);
    assert_eq!(repo.review(&correct, 603000).await?.interval_days, 1);
    let edit = UpdateStudyCardRequestDto {
        card_id: c.id.clone(),
        question: "Revised question".into(),
        answer: "The complete stated conditions".into(),
        explanation: "Revised explanation".into(),
    };
    service::update_card(&repo, edit).await?;
    let reopened = StudyRepository::new(db.pool().clone()).get(&d.id).await?;
    assert_eq!(
        reopened.cards[0].options[0],
        "The complete stated conditions"
    );
    assert_eq!(reopened.study_goal, "Understand the process");
    assert_eq!(reopened.cards[0].review_count, 2);
    assert_eq!(repo.list(603001).await?[0].quiz_correct, 1);
    repo.delete_deck(&d.id).await?;
    assert!(repo.get(&d.id).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM study_reviews")
            .fetch_one(db.pool())
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn conversation_claims_keep_verified_answers_and_resolve_their_citations(
) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = DatabaseConnection::new(dir.path().join("conversation-study.db")).await?;
    initialize_database(db.pool()).await?;
    let repo = StudyRepository::new(db.pool().clone());
    let conversation_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO conversations (id,title,model_name) VALUES (?,'Research notes','test')",
    )
    .bind(&conversation_id)
    .execute(db.pool())
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
        .execute(db.pool())
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
    let dir = tempfile::tempdir()?;
    let db = DatabaseConnection::new(dir.path().join("sources.db")).await?;
    initialize_database(db.pool()).await?;
    let repo = StudyRepository::new(db.pool().clone());
    let selected = uuid::Uuid::new_v4().to_string();
    let other = uuid::Uuid::new_v4().to_string();
    for id in [&selected, &other] {
        sqlx::query("INSERT INTO documents (id,file_path,file_name,size_bytes,modified_at,checksum) VALUES (?,?,?,100,'2026-09-15',?)")
            .bind(id).bind(format!("/library/{id}.pdf")).bind("chapter.pdf").bind(id).execute(db.pool()).await?;
        for i in 0..40 {
            sqlx::query(
                "INSERT INTO text_chunks (id,document_id,content,chunk_index) VALUES (?,?,?,?)",
            )
            .bind(format!("{id}-{i}"))
            .bind(id)
            .bind(if i == 30 {
                PASSAGE
            } else {
                "Another passage about disclosure."
            })
            .bind(i)
            .execute(db.pool())
            .await?;
        }
    }
    let sources = repo
        .sources(std::slice::from_ref(&selected), "petition")
        .await?;
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].document_id, selected);
    assert_eq!(sources[0].excerpt, PASSAGE);
    assert!(repo
        .sources(std::slice::from_ref(&selected), "unmatchedxyz")
        .await
        .is_err());
    assert!(repo.sources(&[selected], "").await?.len() <= 24);
    Ok(())
}

#[tokio::test]
async fn generates_biology_cards_from_markdown_with_an_optional_learning_goal() -> anyhow::Result<()>
{
    use crate::{
        application::contracts::settings::{LLMSettingsDto, LlamaCppSettingsDto},
        features::llm::llama_cpp::LlamaCppLlm,
    };
    use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
    let dir = tempfile::tempdir()?;
    let db = DatabaseConnection::new(dir.path().join("biology.db")).await?;
    initialize_database(db.pool()).await?;
    let repo = StudyRepository::new(db.pool().clone());
    let id = uuid::Uuid::new_v4().to_string();
    let passage = "Photosynthesis converts light energy into chemical energy. Chlorophyll absorbs the light used in photosynthesis.";
    sqlx::query("INSERT INTO documents (id,file_path,file_name,size_bytes,modified_at,checksum) VALUES (?,'/library/biology.md','biology.md',100,'2026-09-15',?)").bind(&id).bind(&id).execute(db.pool()).await?;
    sqlx::query("INSERT INTO text_chunks (id,document_id,content,chunk_index) VALUES ('biology-chunk',?,?,0)").bind(&id).bind(passage).execute(db.pool()).await?;
    let response = json!({"cards":[
        {"question":"Which energy conversion occurs in photosynthesis?","options":["Light to chemical energy","Chemical to light energy","Heat to sound","Sound to light","Heat to motion"],"correctIndex":0,"explanation":"The first sentence names light as the input and chemical energy as the output.","sourceIndex":0,"quote":"Photosynthesis converts light energy into chemical energy.","topic":"Energy conversion"},
        {"question":"What absorbs the light used in photosynthesis?","options":["Water","Chlorophyll","Oxygen","Glucose","Carbon dioxide"],"correctIndex":1,"explanation":"The second sentence identifies chlorophyll as the light absorber.","sourceIndex":0,"quote":"Chlorophyll absorbs the light used in photosynthesis.","topic":"Chlorophyll"}
    ]}).to_string();
    let server = MockServer::start().await;
    Mock::given(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":{"content":response},"finish_reason":"stop"}]})
        )))
        .expect(2)
        .mount(&server)
        .await;
    let llm = LlamaCppLlm::new(&LLMSettingsDto {
        llama_cpp: LlamaCppSettingsDto {
            url: server.uri(),
            model: "test".into(),
            auth_header_name: String::new(),
            auth_header_value: String::new(),
        },
        ..Default::default()
    })?;
    for goal in ["", "High school biology exam"] {
        let deck = service::generate_deck(
            &repo,
            &llm,
            GenerateStudyDeckRequestDto {
                title: "Biology".into(),
                document_ids: vec![id.clone()],
                focus: "photosynthesis".into(),
                study_goal: goal.into(),
                count: 2,
            },
        )
        .await?;
        let saved = repo.get(&deck.id).await?;
        assert_eq!(saved.study_goal, goal);
        assert_eq!(saved.cards.len(), 2);
        assert!(saved
            .cards
            .iter()
            .all(|card| card.source.file_name == "biology.md"
                && passage.contains(&card.source.excerpt)));
    }
    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[1].body)?;
    let prompt: serde_json::Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap())?;
    assert_eq!(prompt["learningGoal"], "High school biology exam");
    assert_eq!(prompt["passages"][0]["text"], passage);
    let system = body["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(!system.contains("patent") && !system.contains("mpep"));
    assert_eq!(body["stream"], true);
    assert_eq!(body["chat_template_kwargs"]["reasoning_effort"], "low");
    assert!(body["response_format"]["json_schema"]["schema"].is_object());
    Ok(())
}

#[test]
fn source_budget_shares_context_between_documents_and_reserves_output_space() {
    let mut first = source();
    first.document_id = "first".into();
    let mut second = source();
    second.document_id = "second".into();
    let sources = vec![first.clone(), first, second];
    let selected = super::generation::bounded_sources(&sources, 2, 328, |_| 50);
    assert_eq!(
        selected
            .iter()
            .map(|s| s.document_id.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert!(super::generation::bounded_sources(&sources, 2, 100, |_| 50).is_empty());
}

#[test]
fn a_reply_whose_only_closing_brace_precedes_the_opening_one_is_rejected_not_a_panic() {
    let s = source();
    let raw = "} and then {\"cards\": [";
    assert!(parse_cards(raw, std::slice::from_ref(&s), "deck", 6, 0).is_err());
    assert!(parse_conversation_cards(raw, &[], "deck", 0).is_err());
}
