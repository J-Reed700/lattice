use super::*;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};

struct MockLLM {
    response: String,
    window: usize,
}

impl MockLLM {
    fn new(response: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            response: response.into(),
            window: 4096,
        })
    }

    /// A 32K window, whose history share holds a long chat whole.
    fn wide(response: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            response: response.into(),
            window: 32_768,
        })
    }
}

#[async_trait]
impl LLMPort for MockLLM {
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> lattice_core::shared::error::Result<CompletionResponse> {
        Ok(CompletionResponse::from_text(
            self.respond(request.user_text()).await?,
        ))
    }
    fn model_name(&self) -> &str {
        "mock-llm"
    }

    fn max_context_tokens(&self) -> usize {
        self.window
    }

    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
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
impl MockLLM {
    async fn respond(&self, _prompt: &str) -> lattice_core::shared::error::Result<String> {
        Ok(self.response.clone())
    }
}

fn input() -> IntentInput {
    IntentInput {
        message: "What did my notes say about the launch?".to_string(),
        recent_context: vec!["User: hello".to_string(), "Assistant: hi".to_string()],
    }
}

#[test]
fn prompt_carries_the_message_and_recent_context() {
    let prompt = render_intent_prompt(DEFAULT_INTENT_PROMPT_TEMPLATE, &input());
    assert!(prompt.contains("What did my notes say about the launch?"));
    assert!(prompt.contains("User: hello"));
}

/// A question the company's own documents answer is not read as a public one.
#[test]
fn needs_web_is_for_what_the_users_files_cannot_hold() {
    let prompt = render_intent_prompt(DEFAULT_INTENT_PROMPT_TEMPLATE, &input());
    let definition = prompt
        .lines()
        .find(|line| line.starts_with("- needsWeb:"))
        .expect("the prompt defines needsWeb");

    for wanted in [
        "cannot be in the user's own files",
        "current events",
        "live data",
        "public facts about the outside world",
        "False for questions about the user's own documents, notes, policies, projects",
        "\"our\", \"my\" or \"the company's\"",
        "When unsure, false.",
    ] {
        assert!(definition.contains(wanted), "needsWeb lost: {wanted}");
    }
    assert!(
        !definition.contains("public information from the web"),
        "the old definition sent any public-sounding question out"
    );
}

#[test]
fn prompt_without_context_says_so() {
    let bare = IntentInput {
        message: "hi".to_string(),
        recent_context: Vec::new(),
    };
    let prompt = render_intent_prompt(DEFAULT_INTENT_PROMPT_TEMPLATE, &bare);
    assert!(prompt.contains("(no prior turns)"));
}

#[tokio::test]
async fn valid_json_parses_into_the_intent() {
    let classifier = IntentClassifier::new(MockLLM::new(
        r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"confidence":0.9}"#,
    ));

    let intent = classifier.classify(&input()).await;

    assert!(intent.needs_knowledge_base);
    assert!(!intent.needs_web);
    assert!(intent.is_followup);
    assert!((intent.confidence - 0.9).abs() < f32::EPSILON);
}

#[tokio::test]
async fn prose_around_the_json_is_ignored() {
    let classifier = IntentClassifier::new(MockLLM::new(
        "Sure! {\"needsKnowledgeBase\":false,\"needsWeb\":true,\"isFollowup\":false,\"confidence\":0.8} done",
    ));

    let intent = classifier.classify(&input()).await;

    assert!(intent.needs_web);
    assert!(!intent.needs_knowledge_base);
}

#[derive(Default)]
struct RecordingLLM {
    sent: std::sync::Mutex<Vec<CompletionRequest>>,
}

#[async_trait]
impl LLMPort for RecordingLLM {
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> lattice_core::shared::error::Result<CompletionResponse> {
        self.sent.lock().unwrap().push(request.clone());
        Ok(CompletionResponse::from_text(
            r#"{"confidence":0.9,"isFollowup":false,"needsKnowledgeBase":true,"needsWeb":false}"#,
        ))
    }
    fn model_name(&self) -> &str {
        "recording-llm"
    }
    fn max_context_tokens(&self) -> usize {
        4096
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
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

#[tokio::test]
async fn the_schema_travels_with_the_request_and_the_prompt_demands_no_format() {
    let llm = Arc::new(RecordingLLM::default());
    let intent = IntentClassifier::new(llm.clone()).classify(&input()).await;

    assert!(intent.needs_knowledge_base);
    let sent = llm.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    let request = sent.first().unwrap();
    assert_eq!(
        request.schema().map(|schema| schema.as_json()),
        Some(&intent_schema())
    );
    assert!(request.no_time_limit, "no clock cuts the classifier off");
    let prompt = request.user_text();
    for demand in ["JSON", "strict", "Respond with", "no other text", "bool"] {
        assert!(!prompt.contains(demand), "prompt still asks for: {demand}");
    }
    for field in [
        "needsKnowledgeBase",
        "needsWeb",
        "isFollowup",
        "standaloneQuestion",
        "confidence",
    ] {
        assert!(prompt.contains(field), "prompt no longer explains {field}");
    }
    assert_eq!(request.sampling, Some(SamplingOverride::deterministic()));
}

#[test]
fn the_schema_requires_every_field_the_parser_reads() {
    let schema = intent_schema();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    let parsed: TurnIntent = serde_json::from_value(serde_json::json!({
        "needsKnowledgeBase": true, "needsWeb": false, "isFollowup": false, "confidence": 0.5
    }))
    .unwrap();
    assert!(parsed.needs_knowledge_base);
    assert_eq!(
        required,
        [
            "needsKnowledgeBase",
            "needsWeb",
            "isFollowup",
            "standaloneQuestion",
            "confidence"
        ]
    );
    assert_eq!(schema["additionalProperties"], false);
}

async fn standalone_question_of(reply: &str) -> Option<String> {
    IntentClassifier::new(MockLLM::new(reply))
        .classify(&IntentInput {
            message: "And on the edge ring?".into(),
            recent_context: Vec::new(),
        })
        .await
        .standalone_question
}

#[tokio::test]
async fn a_followup_keeps_the_question_it_stands_for() {
    let question = standalone_question_of(
        r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"standaloneQuestion":"  What is the hold on the edge ring? ","confidence":0.9}"#,
    )
    .await;
    assert_eq!(
        question.as_deref(),
        Some("What is the hold on the edge ring?")
    );
}

#[tokio::test]
async fn only_a_believed_followup_has_a_standalone_question() {
    for reply in [
        r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":false,"standaloneQuestion":"A rewrite","confidence":0.9}"#,
        r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"standaloneQuestion":"A rewrite","confidence":0.2}"#,
        r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"standaloneQuestion":"  ","confidence":0.9}"#,
        r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"confidence":0.9}"#,
    ] {
        assert_eq!(standalone_question_of(reply).await, None, "{reply}");
    }
}

#[tokio::test]
async fn garbage_falls_back_to_all_false() {
    let classifier = IntentClassifier::new(MockLLM::new("I have no idea."));

    assert_eq!(classifier.classify(&input()).await, TurnIntent::fallback());
}

#[tokio::test]
async fn missing_fields_fall_back_to_all_false() {
    let classifier = IntentClassifier::new(MockLLM::new(
        r#"{"needsKnowledgeBase":true,"confidence":0.9}"#,
    ));

    assert_eq!(classifier.classify(&input()).await, TurnIntent::fallback());
}

#[tokio::test]
async fn low_confidence_clears_the_booleans_but_keeps_the_confidence() {
    let classifier = IntentClassifier::new(MockLLM::new(
        r#"{"needsKnowledgeBase":true,"needsWeb":true,"isFollowup":true,"confidence":0.2}"#,
    ));

    let intent = classifier.classify(&input()).await;

    assert!(!intent.needs_knowledge_base);
    assert!(!intent.needs_web);
    assert!(!intent.is_followup);
    assert!((intent.confidence - 0.2).abs() < f32::EPSILON);
}

#[tokio::test]
async fn out_of_range_confidence_is_clamped() {
    let classifier = IntentClassifier::new(MockLLM::new(
        r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":false,"confidence":7}"#,
    ));

    let intent = classifier.classify(&input()).await;

    assert_eq!(intent.confidence, 1.0);
    assert!(intent.needs_knowledge_base);
}

/// The history is what the classifier's window gives it: newest turns whole
/// and the rest counted.
#[test]
fn history_is_sized_from_the_classifiers_window() {
    use lattice_core::application::services::history_share::auxiliary_history_room;
    let context: Vec<String> = (0..60)
        .map(|turn| format!("turn {turn} {}", "word ".repeat(20)))
        .collect();

    let wide = IntentClassifier::new(MockLLM::wide("{}")).input_for("and then?", &context);
    assert_eq!(wide.recent_context, context, "a 32K window holds it all");

    let narrow = IntentClassifier::new(MockLLM::new("{}"));
    let input = narrow.input_for("and then?", &context);
    assert!(input.recent_context.len() > 1 && input.recent_context.len() < context.len());
    assert!(
        input.recent_context[0].contains("earlier messages not shown"),
        "{:?}",
        input.recent_context
    );
    assert!(input.recent_context.last().unwrap().starts_with("turn 59 "));
    let room = auxiliary_history_room(
        narrow.llm.as_ref(),
        &render_intent_prompt(
            DEFAULT_INTENT_PROMPT_TEMPLATE,
            &IntentInput {
                message: "and then?".into(),
                recent_context: Vec::new(),
            },
        ),
    );
    let carried: usize = input.recent_context[1..]
        .iter()
        .map(|turn| narrow.llm.count_tokens(turn) + 1)
        .sum();
    assert!(carried <= room, "{carried} of {room}");
}

#[test]
fn the_conversations_system_prompt_is_not_shown_as_a_turn() {
    let context = [
        "System: You are Lattice, a precise research assistant.".to_string(),
        "User: When does the agreement renew?".to_string(),
        "Assistant: On 2025-10-14 [1].".to_string(),
    ];
    let input = IntentClassifier::new(MockLLM::wide("{}")).input_for("And the notice?", &context);
    assert_eq!(input.recent_context, context[1..]);
    let alone = IntentClassifier::new(MockLLM::wide("{}")).input_for("Hi there", &context[..1]);
    assert!(
        alone.recent_context.is_empty(),
        "{:?}",
        alone.recent_context
    );
}

/// Answers after `delay`, as a slow local model does.
struct SlowLLM {
    delay: std::time::Duration,
}

#[async_trait]
impl LLMPort for SlowLLM {
    async fn complete(
        &self,
        _request: &CompletionRequest,
    ) -> lattice_core::shared::error::Result<CompletionResponse> {
        tokio::time::sleep(self.delay).await;
        Ok(CompletionResponse::from_text(
            r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"confidence":0.9}"#,
        ))
    }
    fn model_name(&self) -> &str {
        "slow-llm"
    }
    fn max_context_tokens(&self) -> usize {
        4096
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
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

/// A model that answers after 20 s is slow, not dead: its answer is used.
#[tokio::test(start_paused = true)]
async fn a_slow_classifier_is_waited_for() {
    let classifier = IntentClassifier::new(Arc::new(SlowLLM {
        delay: std::time::Duration::from_secs(20),
    }));
    let started = tokio::time::Instant::now();
    let intent = classifier.classify(&input()).await;
    assert!(started.elapsed() >= std::time::Duration::from_secs(20));
    assert!(intent.needs_knowledge_base);
    assert!(intent.is_followup);
    assert!((intent.confidence - 0.9).abs() < f32::EPSILON);
}

/// A newest turn longer than the whole room goes in by its start, marked as
/// continuing, and nothing older.
#[test]
fn a_newest_turn_too_long_for_the_room_goes_in_partially_and_marked() {
    use lattice_core::application::services::history_share::auxiliary_history_room;
    let classifier = IntentClassifier::new(MockLLM::new("{}"));
    let room = auxiliary_history_room(
        classifier.llm.as_ref(),
        &render_intent_prompt(
            DEFAULT_INTENT_PROMPT_TEMPLATE,
            &IntentInput {
                message: "and then?".into(),
                recent_context: Vec::new(),
            },
        ),
    );
    let context = vec![
        "Assistant: an earlier answer".to_string(),
        format!("Assistant: {}", "word ".repeat(room * 3)),
    ];
    let input = classifier.input_for("and then?", &context);
    assert_eq!(input.recent_context.len(), 2, "{:?}", input.recent_context);
    assert!(input.recent_context[0].contains("1 earlier message not shown"));
    let partial = &input.recent_context[1];
    assert!(partial.starts_with("Assistant: word word"));
    assert!(partial.contains("more characters not shown"), "{partial}");
    assert!(classifier.llm.count_tokens(partial) < room);
}

#[test]
fn unified_triage_schema_and_prompt_have_expected_contract() {
    let schema = unified_triage_schema();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["required"],
        serde_json::json!([
            "needsKnowledgeBase",
            "needsWeb",
            "isFollowup",
            "standaloneQuestion",
            "documentAction",
            "clarifyQuestion",
            "rationale",
            "confidence"
        ])
    );
    assert_eq!(
        schema["properties"]["documentAction"]["enum"],
        serde_json::json!(["none", "use_last_document", "new_search", "clarify"])
    );
    let prompt =
        render_unified_triage_prompt(DEFAULT_UNIFIED_TRIAGE_PROMPT_TEMPLATE, &input(), "Q4.md");
    assert!(prompt.contains("Recent document title: Q4.md"));
    assert_eq!(
        prompt.matches("Q4.md").count(),
        1,
        "recent document title should be interpolated only once in the data block"
    );
    for demand in ["JSON", "strict", "Respond with", "no other text", "bool"] {
        assert!(!prompt.contains(demand), "prompt still asks for: {demand}");
    }
}

async fn run_unified_triage(
    kb: bool,
    is_followup: bool,
    action: Option<&str>,
    extra: &str,
) -> UnifiedTurnTriage {
    let act_kv = action.map_or(String::new(), |a| format!(r#""documentAction":"{a}","#));
    let s = format!(
        r#"{{"needsKnowledgeBase":{kb},"needsWeb":false,"isFollowup":{is_followup},{act_kv}{extra}"confidence":0.9}}"#
    );
    let c = IntentClassifier::new(MockLLM::new(s));
    let inp = c.input_for_recent_document("next?", &["User: hi".into()], "Q4.md");
    c.triage_with_recent_document(&inp, "Q4.md").await
}

#[tokio::test]
async fn unified_triage_parses_all_actions_and_fallbacks() {
    use TriageDocumentAction as A;
    for (kb, fu, a, exp) in [
        (true, true, Some("use_last_document"), A::UseLastDocument),
        (false, false, Some("none"), A::None),
        (true, false, None, A::NewSearch),
        (false, false, None, A::None),
    ] {
        let out = run_unified_triage(kb, fu, a, "").await;
        assert_eq!(out.document_action, exp);
        assert!(!out.used_fallback);
    }

    // Non-followup new_search drops standaloneQuestion to prevent duplicate search branches.
    let non_followup = run_unified_triage(
        true,
        false,
        Some("new_search"),
        r#""standaloneQuestion":"Budget","rationale":"new topic","#,
    )
    .await;
    assert_eq!(non_followup.document_action, A::NewSearch);
    assert_eq!(non_followup.intent.standalone_question, None);
    assert_eq!(non_followup.rationale.as_deref(), Some("new topic"));
    assert!(!non_followup.used_fallback);

    // Follow-up new_search retains standaloneQuestion.
    let followup = run_unified_triage(
        true,
        true,
        Some("new_search"),
        r#""standaloneQuestion":"What is the Q4 budget?","#,
    )
    .await;
    assert_eq!(followup.document_action, A::NewSearch);
    assert_eq!(
        followup.intent.standalone_question.as_deref(),
        Some("What is the Q4 budget?")
    );
    assert!(!followup.used_fallback);

    let clarify = run_unified_triage(
        true,
        false,
        None,
        r#""document_action":"clarify","clarify_question":" Q4.md? ","#,
    )
    .await;
    assert_eq!(clarify.document_action, A::Clarify);
    assert_eq!(clarify.clarify_question.as_deref(), Some("Q4.md?"));
    assert!(!clarify.used_fallback);

    let bad = IntentClassifier::new(MockLLM::new("not json"))
        .triage_with_recent_document(&input(), "Q4.md")
        .await;
    assert_eq!(bad.intent, TurnIntent::fallback());
    assert_eq!(bad.document_action, A::NewSearch);
    assert_eq!(bad.rationale.as_deref(), Some("fallback"));
    assert!(bad.used_fallback);
}

/// Both prompts define what a turn needs in the same words, so a change to a
/// definition reaches both.
#[test]
fn both_prompts_define_what_a_turn_needs_in_the_same_words() {
    let definitions = |template: &str| -> Vec<String> {
        template
            .lines()
            .filter(|line| {
                [
                    "- needsKnowledgeBase:",
                    "- needsWeb:",
                    "- isFollowup:",
                    "- standaloneQuestion:",
                ]
                .iter()
                .any(|key| line.starts_with(key))
            })
            .map(str::to_string)
            .collect()
    };
    let classify = definitions(DEFAULT_INTENT_PROMPT_TEMPLATE);
    assert_eq!(classify.len(), 4, "{classify:?}");
    assert_eq!(
        classify,
        definitions(DEFAULT_UNIFIED_TRIAGE_PROMPT_TEMPLATE)
    );
}

/// The two answer shapes share the intent fields, in the order the models
/// were measured with: llama.cpp writes the fields in the schema's order.
#[test]
fn the_two_schemas_share_the_intent_fields_in_the_measured_order() {
    let keys = |schema: &serde_json::Value| -> Vec<String> {
        schema["properties"]
            .as_object()
            .expect("properties")
            .keys()
            .cloned()
            .collect()
    };
    let required = |schema: &serde_json::Value| -> Vec<String> {
        schema["required"]
            .as_array()
            .expect("required")
            .iter()
            .map(|key| key.as_str().expect("a key").to_string())
            .collect()
    };
    let intent = intent_schema();
    let triage = unified_triage_schema();

    assert_eq!(
        keys(&intent),
        [
            "needsKnowledgeBase",
            "needsWeb",
            "isFollowup",
            "standaloneQuestion",
            "confidence"
        ]
    );
    assert_eq!(
        keys(&triage),
        [
            "needsKnowledgeBase",
            "needsWeb",
            "isFollowup",
            "standaloneQuestion",
            "documentAction",
            "clarifyQuestion",
            "rationale",
            "confidence"
        ]
    );
    assert_eq!(required(&intent), keys(&intent));
    assert_eq!(required(&triage), keys(&triage));
    for key in keys(&intent) {
        assert_eq!(
            intent["properties"][&key], triage["properties"][&key],
            "{key}"
        );
    }
    assert_eq!(intent["additionalProperties"], false);
    assert_eq!(triage["additionalProperties"], false);
}
