use super::readings_tests::Overlap;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn sections(count: usize) -> Vec<Value> {
    (0..count)
        .map(|index| json!({"kind":"teaching","title":format!("Section {index}"),"body":GOOD}))
        .collect()
}

fn inventory_of(count: usize) -> Inventory {
    Inventory {
        units: (0..count)
            .map(|index| UnitClaims {
                index,
                claims: vec![Claim {
                    quote: GOOD.into(),
                    statement: GOOD.into(),
                }],
                non_factual_reason: String::new(),
            })
            .collect(),
    }
}

fn sorted(batches: &Mutex<Vec<Vec<usize>>>) -> Vec<Vec<usize>> {
    let mut batches = batches.lock().unwrap().clone();
    batches.sort();
    batches
}

/// Answers the first `bad` extractions of section 4 alone with an empty
/// answer, and everything else as the fixture model does.
struct Flaky {
    inner: Model,
    bad: usize,
    asked: AtomicUsize,
}

impl Flaky {
    fn new(bad: usize) -> Self {
        Self {
            inner: Model::new(),
            bad,
            asked: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl LLMPort for Flaky {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            let only_last = prompt.starts_with("Extract lesson claims.")
                && context(&prompt)["requestedIndices"] == json!([4]);
            let reply = self.inner.respond(&prompt).await?;
            if only_last && self.asked.fetch_add(1, Ordering::SeqCst) < self.bad {
                replies.push(json!({"units":{}}).to_string());
            } else {
                replies.push(reply);
            }
        }
        Ok(fixture_completion(request, replies))
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
        Ok(true)
    }
}

#[tokio::test]
async fn inventory_asks_its_batches_of_sections_at_once() -> Result<()> {
    let inner = Model::new();
    let model = Overlap::new(&inner, 2);

    let inventory = inventory::extract_remaining(&model, &sections(5), vec![]).await?;

    assert_eq!(model.most(), 2);
    assert_eq!(
        sorted(&inner.extracted_sections),
        vec![vec![0, 1, 2, 3], vec![4]]
    );
    let indices: Vec<usize> = inventory.units.iter().map(|unit| unit.index).collect();
    assert_eq!(indices, vec![0, 1, 2, 3, 4]);
    Ok(())
}

#[tokio::test]
async fn inventory_is_the_same_whether_its_batches_are_asked_at_once_or_not() -> Result<()> {
    let inner = Model::new();
    let model = Overlap::new(&inner, 2);
    let once = inventory::extract_remaining(&model, &sections(9), vec![]).await?;
    let alone = inventory::extract_remaining(&Model::new(), &sections(9), vec![]).await?;

    assert_eq!(serde_json::to_value(&once)?, serde_json::to_value(&alone)?);
    Ok(())
}

#[tokio::test]
async fn a_batch_with_a_malformed_first_answer_is_asked_once_more() -> Result<()> {
    let model = Flaky::new(1);

    let inventory = inventory::extract_remaining(&model, &sections(5), vec![]).await?;

    assert_eq!(inventory.units.len(), 5);
    assert_eq!(
        sorted(&model.inner.extracted_sections),
        vec![vec![0, 1, 2, 3], vec![4], vec![4]]
    );
    Ok(())
}

#[tokio::test]
async fn a_batch_failing_twice_names_its_sections_and_the_others_are_kept() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let model = Flaky::new(2);
    let content = sections(5);
    crate::lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson, async {
        let error = inventory::extract_remaining(&model, &content, vec![])
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("sections 5"), "{error}");
        assert_eq!(
            sorted(&model.inner.extracted_sections),
            vec![vec![0, 1, 2, 3], vec![4], vec![4]]
        );
        let resumed = Model::new();
        let inventory = inventory::extract_remaining(&resumed, &content, vec![]).await?;
        assert_eq!(inventory.units.len(), 5);
        assert_eq!(*resumed.extracted_sections.lock().unwrap(), vec![vec![4]]);
        Ok(())
    })
    .await
}

/// Holds each coverage mapping request until another is in flight, at most
/// 10 s, and records the most that were in flight together.
struct Mappings<'m> {
    inner: &'m Model,
    peak: tokio::sync::watch::Sender<usize>,
    now: AtomicUsize,
    most: AtomicUsize,
}

#[async_trait::async_trait]
impl LLMPort for Mappings<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mapping = fixture_prompts(request)
            .iter()
            .any(|prompt| prompt.starts_with("Audit claim coverage independently."));
        if mapping {
            let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(now, Ordering::SeqCst);
            self.peak.send_modify(|peak| *peak = (*peak).max(now));
            let _ = tokio::time::timeout(
                Duration::from_secs(10),
                self.peak.subscribe().wait_for(|peak| *peak >= 2),
            )
            .await;
        }
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.inner.respond(&prompt).await?);
        }
        if mapping {
            self.now.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(fixture_completion(request, replies))
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
        Ok(true)
    }
}

#[tokio::test]
async fn coverage_audits_its_batches_of_sections_at_once() -> Result<()> {
    let inner = Model::new();
    let model = Mappings {
        inner: &inner,
        peak: tokio::sync::watch::channel(0).0,
        now: AtomicUsize::new(0),
        most: AtomicUsize::new(0),
    };
    let content = sections(5);
    let inventory = inventory_of(5);

    let audited = coverage::audit(&model, &content, &inventory, None).await?;
    let alone = coverage::audit(&Model::new(), &content, &inventory, None).await?;

    assert_eq!(model.most.load(Ordering::SeqCst), 2);
    assert_eq!(
        sorted(&inner.audited_sections),
        vec![vec![0, 1, 2, 3], vec![4]]
    );
    let indices: Vec<usize> = audited.units.iter().map(|unit| unit.index).collect();
    assert_eq!(indices, vec![0, 1, 2, 3, 4]);
    assert_eq!(
        serde_json::to_value(&audited)?,
        serde_json::to_value(&alone)?
    );
    Ok(())
}

struct Represented;

#[async_trait::async_trait]
impl LLMPort for Represented {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let replies = fixture_prompts(request)
            .into_iter()
            .map(|_| {
                json!({"missingAssertions":[],"reason":"Every assertion is represented."})
                    .to_string()
            })
            .collect();
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "represented-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        32000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn fidelity_checks_every_passage_of_a_section_at_once() -> Result<()> {
    let passages = 4;
    let model = Overlap::new(&Represented, passages);
    let fixture = json!({
        "units": [{"index":0,
            "passages":(0..passages).map(|i|json!({"id":format!("p-{i}"),"text":"Equal inputs guarantee identical results."})).collect::<Vec<_>>(),
            "claims":[{"id":"claim-a","statement":"Equal inputs guarantee identical results."}]}],
        "mapping": (0..passages)
            .map(|i| (format!("p-{i}"), json!({"claimIds":["claim-a"],"nonFactualReason":"","missingClaims":[]})))
            .collect::<std::collections::BTreeMap<_, _>>(),
    });

    coverage::live_mapping_fixture(&model, &fixture).await?;

    assert_eq!(model.most(), passages);
    Ok(())
}
