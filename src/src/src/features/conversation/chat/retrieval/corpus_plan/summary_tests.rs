//! Summary-tier behaviour inside the retrieval planner: what the planner is
//! shown, and who gets to choose the opening documents.

use super::*;

/// Replays one canned plan and records what it was asked.
struct SummaryPlannerStub {
    response: String,
    prompts: std::sync::Mutex<Vec<String>>,
}

impl SummaryPlannerStub {
    fn new(response: &str) -> Self {
        Self {
            response: response.to_string(),
            prompts: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn last_prompt(&self) -> String {
        self.prompts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .last()
            .cloned()
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl LLMPort for SummaryPlannerStub {
    async fn generate(
        &self,
        prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<String> {
        self.prompts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(prompt.to_string());
        Ok(self.response.clone())
    }

    async fn generate_streaming(
        &self,
        _prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Ok(Box::new(Box::pin(futures::stream::empty())))
    }

    fn model_name(&self) -> &str {
        "summary-planner-stub"
    }

    fn max_context_tokens(&self) -> usize {
        32_000
    }

    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn document(id: &str, name: &str) -> CorpusDocument {
    CorpusDocument {
        id: id.into(),
        name: name.into(),
        opening: "Opening line".into(),
        chapter_number: None,
        source_context: None,
        sections: Vec::new(),
    }
}

fn catalog() -> Vec<CorpusDocument> {
    vec![document("doc-1", "Manual"), document("doc-2", "Primer")]
}

fn summaries(entries: &[(&str, &str)]) -> HashMap<String, String> {
    entries
        .iter()
        .map(|(id, text)| ((*id).to_string(), (*text).to_string()))
        .collect()
}

const PLAN: &str =
    r#"{"queries":["engine start"],"opening_document_ids":[],"start_at_beginning":false}"#;

#[tokio::test]
async fn the_catalog_carries_summaries_when_the_tier_has_them() {
    let stub = SummaryPlannerStub::new(PLAN);
    let catalog = catalog();
    plan(
        &stub,
        "how do I start the engine",
        None,
        &catalog,
        &summaries(&[("doc-1", "A manual about preflight and engine start.")]),
    )
    .await
    .unwrap();
    let prompt = stub.last_prompt();
    assert!(prompt.contains("A manual about preflight and engine start."));
    // A document with no summary is still offered, just without one.
    assert!(prompt.contains("Primer"));
    assert_eq!(prompt.matches("\"summary\"").count(), 1);
}

#[tokio::test]
async fn an_empty_summary_map_reproduces_the_plain_catalog() {
    let with_tier = SummaryPlannerStub::new(PLAN);
    let catalog = catalog();
    plan(&with_tier, "engine start", None, &catalog, &HashMap::new())
        .await
        .unwrap();
    let without_tier = SummaryPlannerStub::new(PLAN);
    plan(
        &without_tier,
        "engine start",
        None,
        &catalog,
        &HashMap::new(),
    )
    .await
    .unwrap();
    assert_eq!(with_tier.last_prompt(), without_tier.last_prompt());
    assert!(!with_tier.last_prompt().contains("\"summary\""));
}

#[tokio::test]
async fn catalog_summaries_are_truncated_per_document() {
    let stub = SummaryPlannerStub::new(PLAN);
    let catalog = catalog();
    let long = "word ".repeat(4_000);
    plan(
        &stub,
        "engine start",
        None,
        &catalog,
        &summaries(&[("doc-1", &long)]),
    )
    .await
    .unwrap();
    let prompt = stub.last_prompt();
    assert!(prompt.len() < long.len());
}

#[test]
fn whole_document_intent_covers_both_phrase_families() {
    assert!(whole_document_intent("Where do I start with this?"));
    assert!(whole_document_intent("What is this document about"));
    assert!(whole_document_intent("Give me an overview"));
    // The ordered-learning phrases count too — they are the same question.
    assert!(whole_document_intent("help me learn this material"));
    assert!(!whole_document_intent(
        "What is the stall speed at 5000 feet?"
    ));
}

#[test]
fn summary_openings_replace_the_planner_choice_only_for_whole_document_requests() {
    let catalog = catalog();
    let mut focused = CorpusSearchPlan {
        queries: vec!["stall speed".into()],
        opening_document_ids: vec!["doc-1".into()],
        start_at_beginning: false,
    };
    assert!(!apply_summary_openings(
        &mut focused,
        "What is the stall speed?",
        &["doc-2".to_string()],
        &catalog
    ));
    assert_eq!(focused.opening_document_ids, ["doc-1"]);

    let mut broad = CorpusSearchPlan {
        queries: vec!["overview".into()],
        opening_document_ids: vec!["doc-1".into()],
        start_at_beginning: false,
    };
    assert!(apply_summary_openings(
        &mut broad,
        "Where do I start?",
        &["doc-2".to_string(), "doc-1".to_string()],
        &catalog
    ));
    assert_eq!(broad.opening_document_ids, ["doc-2", "doc-1"]);
}

#[test]
fn a_start_at_beginning_plan_uses_summaries_even_without_a_matching_phrase() {
    let catalog = catalog();
    let mut plan = CorpusSearchPlan {
        queries: vec!["introduction".into()],
        opening_document_ids: Vec::new(),
        start_at_beginning: true,
    };
    assert!(apply_summary_openings(
        &mut plan,
        "teach me",
        &["doc-2".to_string()],
        &catalog
    ));
    assert_eq!(plan.opening_document_ids, ["doc-2"]);
}

#[test]
fn unknown_or_absent_summary_documents_leave_the_plan_alone() {
    let catalog = catalog();
    let mut plan = CorpusSearchPlan {
        queries: vec!["overview".into()],
        opening_document_ids: vec!["doc-1".into()],
        start_at_beginning: true,
    };
    assert!(!apply_summary_openings(
        &mut plan,
        "where do I start",
        &[],
        &catalog
    ));
    assert!(!apply_summary_openings(
        &mut plan,
        "where do I start",
        &["not-in-scope".to_string()],
        &catalog
    ));
    assert_eq!(plan.opening_document_ids, ["doc-1"]);
}

#[test]
fn explicit_reading_order_outranks_summary_similarity() {
    use crate::domain::value_objects::source_context::{SourceContext, SourceGroup};

    let ordered = |id: &str, position: u32| CorpusDocument {
        source_context: Some(SourceContext {
            group: SourceGroup {
                id: "book".into(),
                title: "Book".into(),
                edition: None,
                description: None,
                ordered: true,
                structure: Default::default(),
            },
            position,
        }),
        ..document(id, id)
    };
    let catalog = vec![ordered("first", 0), ordered("second", 1)];
    let mut plan = CorpusSearchPlan {
        queries: vec!["overview".into()],
        opening_document_ids: Vec::new(),
        start_at_beginning: true,
    };
    // Similarity ranked the later chapter first; reading order puts it back.
    assert!(apply_summary_openings(
        &mut plan,
        "start at the beginning",
        &["second".to_string(), "first".to_string()],
        &catalog
    ));
    assert_eq!(plan.opening_document_ids, ["first", "second"]);
}
