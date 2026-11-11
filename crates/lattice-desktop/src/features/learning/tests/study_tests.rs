//! Stands in for `lattice_learning::recall::study_tests`, which declared this test.

use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
use lattice_learning::lessons::reference_collection::test_support::FakeLibrary;
use lattice_learning::recall::study_dto::GenerateStudyDeckRequestDto;
use lattice_learning::recall::study_repository::StudyRepository;
use lattice_learning::recall::study_service as service;
use serde_json::json;

#[tokio::test]
async fn generates_biology_cards_from_markdown_with_an_optional_learning_goal() -> anyhow::Result<()>
{
    use crate::features::llm::llama_cpp::LlamaCppLlm;
    use lattice_core::application::contracts::settings::{LLMSettingsDto, LlamaCppSettingsDto};
    use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
    let dir = tempfile::tempdir()?;
    let db = DatabaseConnection::new(dir.path().join("biology.db")).await?;
    initialize_database(db.database()).await?;
    let repo = StudyRepository::new(db.database().clone());
    let id = uuid::Uuid::new_v4().to_string();
    let passage = "Photosynthesis converts light energy into chemical energy. Chlorophyll absorbs the light used in photosynthesis.";
    let library = FakeLibrary::default().with_document(&id, &[passage]);
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
    })?
    .into_port()
    .await;
    for goal in ["", "High school biology exam"] {
        let deck = service::generate_deck(
            &repo,
            &library,
            llm.as_ref(),
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
            .all(|card| card.source.file_name == format!("{id}.md")
                && passage.contains(&card.source.excerpt)));
    }
    let requests = server.received_requests().await.unwrap();
    let completions: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path() == "/v1/chat/completions")
        .collect();
    let body: serde_json::Value = serde_json::from_slice(&completions[1].body)?;
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
    // Learning asks for "low" effort; on llama.cpp that is no switch, so the
    // request turns thinking off explicitly and sends no level with it.
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
    assert!(body.get("reasoning_effort").is_none());
    assert!(body["response_format"]["json_schema"]["schema"].is_object());
    Ok(())
}
