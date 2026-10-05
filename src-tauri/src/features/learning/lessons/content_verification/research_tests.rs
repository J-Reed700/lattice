#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use crate::features::{
    function_calling::dto::{FetchUrlContentOutput, WebSearchOutput, WebSearchResult},
    learning::{lessons::course_generation_tests::ScriptedModel, repository::LearningRepository},
    web::mocks::MockWebService,
};

const CAPTURE: &str =
    "Random assignment reduces confounding in comparisons between treatment groups.";
struct ResearchWeb(MockWebService);
#[async_trait::async_trait]
impl WebServiceTrait for ResearchWeb {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        self.0.search_web(input).await
    }
    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.0.fetch_url_content(url).await
    }
    async fn fetch_url_content(&self, _: &str) -> Result<FetchUrlContentOutput> {
        panic!("Lesson research must fetch full captures, not chat previews")
    }
    fn validate_url(&self, url: &str) -> Result<()> {
        self.0.validate_url(url)
    }
}
fn web(truncated: bool) -> Arc<ResearchWeb> {
    web_capture(truncated, "https://example.org/research", CAPTURE)
}
fn web_capture(truncated: bool, url: &str, content: &str) -> Arc<ResearchWeb> {
    let web = MockWebService::new();
    web.set_search_results(vec![WebSearchResult {
        title: "Research methods".into(),
        url: url.into(),
        snippet: "SEARCH_SNIPPET_MUST_NOT_BE_EVIDENCE".into(),
        published_date: None,
        source: None,
    }]);
    web.set_url_content(
        url,
        FetchUrlContentOutput {
            url: url.into(),
            title: Some("Research methods".into()),
            content: format!("\r\n{content}\r\n"),
            content_truncated: truncated,
            word_count: 12,
            fetch_time_ms: 1.,
            content_type: None,
            from_cache: false,
        },
    );
    Arc::new(ResearchWeb(web))
}
fn gap() -> Finding {
    Finding {
        unit: 0,
        quote: CAPTURE.into(),
        statement: CAPTURE.into(),
        verdict: ClaimVerdict::Unsupported,
        reason: "The saved introduction does not discuss random assignment.".into(),
        evidence: vec![],
        supporting_quote: None,
    }
}
fn model() -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(
            [
                json!({"queries":["random assignment confounding official guidance"]}).to_string(),
                selection(&[0]),
            ]
            .into(),
        ),
        prompts: Mutex::new(vec![]),
    }
}

fn selection(indices: &[usize]) -> String {
    json!({"selectedResults":indices.iter().map(|i|json!({"id":format!("result-{i}"),"reason":"Directly relevant primary reference for the requested concept."})).collect::<Vec<_>>()}).to_string()
}

#[tokio::test]
async fn reference_selection_can_reject_unrelated_hits_without_inventing_pages() -> Result<()> {
    let results = [
        ("Cargo definition", "https://dictionary.example/cargo"),
        ("Cargo build manual", "https://docs.example/cargo/build"),
    ]
    .into_iter()
    .map(|(title, url)| WebSearchResult {
        title: title.into(),
        url: url.into(),
        snippet: "Search discovery only".into(),
        published_date: None,
        source: None,
    })
    .collect::<Vec<_>>();
    let llm = ScriptedModel {
        outputs: Mutex::new(
            [
                selection(&[1]),
                selection(&[]),
                selection(&[2]),
                selection(&[1, 1]),
            ]
            .into(),
        ),
        prompts: Mutex::new(vec![]),
    };
    let chosen =
        select_references(&llm, "Rust programming", "cargo build", results.clone()).await?;
    assert_eq!(chosen.len(), 1);
    assert_eq!(chosen[0].url, results[1].url);
    assert!(
        select_references(&llm, "Rust programming", "cargo build", results.clone())
            .await?
            .is_empty()
    );
    assert!(
        select_references(&llm, "Rust programming", "cargo build", results.clone())
            .await
            .is_err()
    );
    assert!(
        select_references(&llm, "Rust programming", "cargo build", results)
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn research_site_constraints_require_the_requested_host_and_path() {
    let query = "site:docs.example.org/manual random assignment";
    assert!(within_search_scope(
        query,
        "https://docs.example.org/manual"
    ));
    assert!(within_search_scope(
        query,
        "https://docs.example.org/manual/chapter.html"
    ));
    assert!(within_search_scope(
        query,
        "https://archive.docs.example.org/manual/chapter"
    ));
    for address in [
        "https://en.wikipedia.org/wiki/Manual",
        "https://docs.example.org.attacker.test/manual",
        "https://fakedocs.example.org/manual",
        "https://docs.example.org/manuals",
        "https://docs.example.org/other",
        "not a URL",
    ] {
        assert!(!within_search_scope(query, address), "{address}");
    }
    assert!(within_search_scope(
        "site:example.org OR site:example.edu topic",
        "https://example.edu/page"
    ));
    assert!(within_search_scope(
        "site:https://example.org/reference.html topic",
        "https://example.org/reference.html"
    ));
    assert!(within_search_scope(
        "unrestricted topic search",
        "https://example.net/page"
    ));
}

#[tokio::test]
async fn research_ignores_out_of_scope_results_and_redirects() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    program.summary.status = crate::features::learning::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let web = web_capture(false, "https://docs.example.org/manual/good", CAPTURE);
    let addresses = [
        "https://unrelated.example/dictionary/assignment",
        "https://docs.example.org/manual/redirect",
        "https://docs.example.org/manual/good",
    ];
    web.0.set_search_results(
        addresses
            .iter()
            .map(|url| WebSearchResult {
                title: "Search result".into(),
                url: (*url).into(),
                snippet: "Snippet".into(),
                published_date: None,
                source: None,
            })
            .collect(),
    );
    for address in &addresses[..2] {
        web.0.set_url_content(
            *address,
            FetchUrlContentOutput {
                url: "https://unrelated.example/redirected".into(),
                title: None,
                content: "Irrelevant material must not be saved as a course reference.".into(),
                content_truncated: false,
                word_count: 12,
                fetch_time_ms: 1.,
                content_type: None,
                from_cache: false,
            },
        );
    }
    let llm = ScriptedModel {
        outputs: Mutex::new(
            [
                json!({"queries":["site:docs.example.org/manual random assignment"]}).to_string(),
                selection(&[0, 1]),
            ]
            .into(),
        ),
        prompts: Mutex::new(vec![]),
    };
    run(
        pool,
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web),
        async {
            let expanded = expand(&llm, &references, &[gap()])
                .await?
                .expect("relevant capture");
            assert_eq!(expanded.sources.len(), sources.len() + 1);
            let saved = repo.verification_sources(&program.summary.id).await?;
            assert!(saved.iter().any(|source| source.excerpt == CAPTURE));
            assert!(saved
                .iter()
                .all(|source| !source.excerpt.contains("Irrelevant material")));
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn research_persists_full_capture_and_deduplicates_without_approving_claims() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    program.summary.status = crate::features::learning::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let original = ReferenceCollection::lexical(&sources)?;
    let llm = model();
    let findings = vec![gap()];
    run(
        pool,
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web(false)),
        async {
            let expanded = expand(&llm, &original, &findings)
                .await?
                .expect("full capture added");
            assert_eq!(expanded.sources.len(), sources.len() + 1);
            let added = expanded
                .sources
                .iter()
                .find(|s| s.excerpt == CAPTURE)
                .expect("exact saved capture");
            let persisted = repo.verification_sources(&program.summary.id).await?;
            assert!(persisted
                .iter()
                .any(|s| s.id == added.id && s.excerpt == CAPTURE));
            assert!(persisted
                .iter()
                .all(|s| !s.excerpt.contains("SEARCH_SNIPPET_MUST_NOT_BE_EVIDENCE")));
            assert_eq!(findings[0].verdict, ClaimVerdict::Unsupported);
            assert!(expand(&model(), &expanded, &findings).await?.is_none());
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn research_rejects_truncated_pages_and_does_not_search_for_judge_failures() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    program.summary.status = crate::features::learning::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    run(
        pool,
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web(true)),
        async {
            let llm = model();
            let mut failed = gap();
            failed.verdict = ClaimVerdict::Unverified;
            assert!(expand(&llm, &references, &[failed]).await?.is_none());
            assert!(llm.prompts.lock().unwrap().is_empty());
            assert!(expand(&llm, &references, &[gap()]).await?.is_none());
            assert_eq!(
                repo.verification_sources(&program.summary.id).await?.len(),
                sources.len()
            );
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn expanded_collection_checks_new_evidence_and_reuses_unchanged_comparisons() -> Result<()> {
    const SECOND_CAPTURE: &str =
        "Allocation concealment prevents foreknowledge of treatment assignments.";
    struct ResearchRounds {
        first: Arc<ResearchWeb>,
        second: Arc<ResearchWeb>,
    }
    #[async_trait::async_trait]
    impl WebServiceTrait for ResearchRounds {
        async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
            if input.query.contains("concealment") {
                self.second.search_web(input).await
            } else {
                self.first.search_web(input).await
            }
        }
        async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
            if url.ends_with("concealment") {
                self.second.fetch_reference_content(url).await
            } else {
                self.first.fetch_reference_content(url).await
            }
        }
        async fn fetch_url_content(&self, _: &str) -> Result<FetchUrlContentOutput> {
            panic!("Use full captures")
        }
        fn validate_url(&self, url: &str) -> Result<()> {
            self.first.validate_url(url)
        }
    }
    struct Model {
        claims: Mutex<Vec<String>>,
        plans: Mutex<usize>,
    }
    #[async_trait::async_trait]
    impl LLMPort for Model {
        async fn generate(
            &self,
            prompt: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<String> {
            if let Some(reply) =
                crate::features::learning::lessons::content_verification::tests::fixture_response(
                    prompt,
                )
            {
                return Ok(reply);
            }
            if prompt.starts_with("Plan focused reference searches") {
                let mut plans = self.plans.lock().unwrap();
                *plans += 1;
                assert!(
                    *plans <= 2,
                    "Research should finish once both gaps are resolved"
                );
                return Ok(
                    json!({"queries":[if *plans == 1 {"random assignment confounding official guidance"} else {"allocation concealment official guidance"}]})
                        .to_string(),
                );
            }
            if prompt.starts_with("Select trustworthy references") {
                return Ok(selection(&[0]));
            }
            if prompt.starts_with("Source passages:") {
                let (passages, tail) = prompt.split_once("\n\nClaim: ").expect("claim request");
                let claim = tail.split("\n\n").next().unwrap();
                self.claims.lock().unwrap().push(claim.into());
                let quote = claim;
                return Ok(if passages.contains(quote) {
                    format!("supported\nReason: The complete capture directly states the claim.\nSource quote: {quote}\nSource passage: passage-0")
                } else {
                    "unsupported\nReason: The requested relationship is missing from the capture.\nSource quote: none".into()
                });
            }
            panic!("Unexpected authoring or repair request")
        }
        async fn generate_streaming(
            &self,
            _: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unreachable!()
        }
        fn model_name(&self) -> &str {
            "reference-expansion-fixture"
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }
        fn max_context_tokens(&self) -> usize {
            128000
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    program.summary.status = crate::features::learning::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let original = sources[0].excerpt.clone();
    let llm = Model {
        claims: Mutex::new(vec![]),
        plans: Mutex::new(0),
    };
    let candidate = json!({"blocks":[{"kind":"explanation","body":original},{"kind":"explanation","body":CAPTURE},{"kind":"explanation","body":SECOND_CAPTURE}],"questions":[]});
    let references = ReferenceCollection::lexical(&sources)?;
    let (_, report) = run(
        pool,
        program.summary.id,
        program.summary.goal,
        Some(Arc::new(ResearchRounds {
            first: web(false),
            second: web_capture(false, "https://example.org/concealment", SECOND_CAPTURE),
        })),
        super::super::verify_and_repair_with_references(
            &llm,
            "{}",
            &json!({"type":"object"}),
            candidate.to_string(),
            &references,
            3000,
        ),
    )
    .await?;
    assert!(report.issues.is_empty());
    assert_eq!(report.sources.len(), 3);
    assert_eq!(*llm.plans.lock().unwrap(), 2);
    let checks = llm.claims.lock().unwrap();
    assert_eq!(checks.iter().filter(|claim| *claim == &original).count(), 1);
    // The missing claim had no retrieval hits on the first pass, so its first
    // model judgment happens only after research supplies a complete capture;
    // the second capture also matches treatment-assignment terms, so this
    // previously supported claim must receive a fresh comparison.
    assert!(
        checks
            .iter()
            .filter(|claim| claim.as_str() == CAPTURE)
            .count()
            >= 2
    );
    for finding in &report.findings {
        assert_eq!(finding.verdict, ClaimVerdict::Supported);
        assert!(finding
            .evidence
            .iter()
            .any(|passage| passage.text.contains(&finding.statement)));
    }
    Ok(())
}
