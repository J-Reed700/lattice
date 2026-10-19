#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use crate::features::learning::content_verification::tests::{fixture_completion, fixture_prompts};
use crate::features::{
    learning::{lessons::course_generation_tests::ScriptedModel, repository::LearningRepository},
    web::mocks::MockWebService,
};
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, WebSearchOutput, WebSearchResult,
};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};

pub(in crate::features::learning) const CAPTURE: &str =
    "Random assignment reduces confounding in comparisons between treatment groups.";
pub(in crate::features::learning) struct ResearchWeb(
    MockWebService,
    Arc<Mutex<Vec<WebSearchInput>>>,
    Arc<Mutex<Vec<String>>>,
);
#[async_trait::async_trait]
impl WebServiceTrait for ResearchWeb {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        self.1.lock().unwrap().push(input.clone());
        self.0.search_web(input).await
    }
    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.2.lock().unwrap().push(url.to_owned());
        self.0.fetch_url_content(url).await
    }
    async fn fetch_url_content(&self, _: &str) -> Result<FetchUrlContentOutput> {
        panic!("Lesson research must fetch full captures, not chat previews")
    }
    fn validate_url(&self, url: &str) -> Result<()> {
        self.0.validate_url(url)
    }
}
impl ResearchWeb {
    pub(in crate::features::learning) fn recorded_inputs(&self) -> Vec<WebSearchInput> {
        self.1.lock().unwrap().clone()
    }
    fn fetched(&self) -> Vec<String> {
        self.2.lock().unwrap().clone()
    }
}
pub(in crate::features::learning) fn web(truncated: bool) -> Arc<ResearchWeb> {
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
            read_via: Default::default(),
        },
    );
    Arc::new(ResearchWeb(
        web,
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
    ))
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
        resolution: None,
        trust: None,
    }
}
fn model() -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(
            [
                json!({"needs":[{"gap":0,"query":"random assignment confounding official guidance","kind":"web"}]}).to_string(),
                selection(&[0]),
            ]
            .into(),
        ),
        prompts: Mutex::new(vec![]),
    }
}

/// `results` as a plain web search's candidates.
fn found(results: Vec<WebSearchResult>) -> Vec<web_research::Candidate> {
    results
        .into_iter()
        .map(|result| web_research::Candidate {
            result,
            kind: lattice_core::domain::sourcing::NeedKind::Web,
            owner: None,
        })
        .collect()
}

pub(in crate::features::learning) fn selection(indices: &[usize]) -> String {
    json!({"decisions":indices.iter().map(|i|json!({"id":format!("result-{i}"),"useForGap":true,"reason":"Directly relevant primary reference for the requested concept."})).collect::<Vec<_>>()}).to_string()
}

#[tokio::test]
async fn reference_selection_accepts_all_valid_selected_results_past_four() -> Result<()> {
    let results: Vec<_> = (0..8)
        .map(|i| WebSearchResult {
            title: format!("Relevant reference {i}"),
            url: format!("https://example.org/reference-{i}"),
            snippet: "Discovery metadata only".into(),
            published_date: None,
            source: None,
        })
        .collect();
    let llm = ScriptedModel {
        outputs: Mutex::new([selection(&(0..8).collect::<Vec<_>>())].into()),
        prompts: Mutex::new(vec![]),
    };
    let selected = select_references(
        &llm,
        "Research methods",
        "random assignment",
        &[],
        found(results),
    )
    .await?;
    assert_eq!(selected.len(), 8);
    assert_eq!(selected[7].url, "https://example.org/reference-7");
    Ok(())
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
                json!({"decisions":[
                    {"id":"result-0","useForGap":false,"reason":"A dictionary meaning does not address the software behavior."},
                    {"id":"result-1","useForGap":true,"reason":"The manual addresses the command's behavior."}
                ]}).to_string(),
                selection(&[]),
                selection(&[2]),
                selection(&[1, 1]),
            ]
            .into(),
        ),
        prompts: Mutex::new(vec![]),
    };
    let chosen = select_references(
        &llm,
        "Rust programming",
        "cargo build",
        &[],
        found(results.clone()),
    )
    .await?;
    assert_eq!(chosen.len(), 1);
    assert_eq!(chosen[0].url, results[1].url);
    assert!(select_references(
        &llm,
        "Rust programming",
        "cargo build",
        &[],
        found(results.clone())
    )
    .await?
    .is_empty());
    assert!(select_references(
        &llm,
        "Rust programming",
        "cargo build",
        &[],
        found(results.clone())
    )
    .await
    .is_err());
    assert!(
        select_references(&llm, "Rust programming", "cargo build", &[], found(results))
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
                read_via: Default::default(),
            },
        );
    }
    let llm = ScriptedModel {
        outputs: Mutex::new(
            [
                json!({"needs":[{"gap":0,"query":"site:docs.example.org/manual random assignment","kind":"web"}]}).to_string(),
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
            let expanded = expand(&llm, &references, &[gap()], &[], &ClaimChecks::default())
                .await?
                .saved()
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
async fn research_can_save_evidence_after_one_hundred_sources() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    program.summary.status = crate::features::learning::dto::LearningProgramStatus::Active;
    program.sources = (0..100)
        .map(|index| LearningSourceDto {
            id: uuid::Uuid::new_v4().to_string(),
            title: format!("Existing reference {index}"),
            url: Some(format!("https://example.org/existing-{index}")),
            excerpt: format!("Existing reference {index} discusses the study design."),
            acquired_at: 0,
        })
        .collect();
    repo.create(&program).await?;
    let library = crate::features::learning::source_library::LearningSourceLibraryRepository::new(
        pool.clone(),
    );
    library
        .preflight_new_source(
            &program.summary.id,
            &uuid::Uuid::new_v4().to_string(),
            &uuid::Uuid::new_v4().to_string(),
        )
        .await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let llm = model();
    run(
        pool,
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web(false)),
        async {
            let expanded = expand(&llm, &references, &[gap()], &[], &ClaimChecks::default())
                .await?
                .saved()
                .expect("new evidence saved");
            assert_eq!(expanded.sources.len(), 101);
            let saved = repo.verification_sources(&program.summary.id).await?;
            assert_eq!(saved.len(), 101);
            assert!(saved.iter().any(|source| source.excerpt == CAPTURE));
            for old in &sources {
                assert!(saved
                    .iter()
                    .any(|source| source.id == old.id && source.excerpt == old.excerpt));
            }
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
            let expanded = expand(&llm, &original, &findings, &[], &ClaimChecks::default())
                .await?
                .saved()
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
            assert!(
                expand(&model(), &expanded, &findings, &[], &ClaimChecks::default())
                    .await?
                    .saved()
                    .is_none()
            );
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
            assert!(
                expand(&llm, &references, &[failed], &[], &ClaimChecks::default())
                    .await?
                    .saved()
                    .is_none()
            );
            assert!(llm.prompts.lock().unwrap().is_empty());
            assert!(
                expand(&llm, &references, &[gap()], &[], &ClaimChecks::default())
                    .await?
                    .saved()
                    .is_none()
            );
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
async fn an_empty_claim_plan_ends_research_honestly_instead_of_failing() -> Result<()> {
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
        Some(web(false)),
        async {
            let llm = ScriptedModel {
                outputs: Mutex::new([json!({"needs":[]}).to_string()].into()),
                prompts: Mutex::new(vec![]),
            };
            let outcome = expand(&llm, &references, &[gap()], &[], &ClaimChecks::default()).await?;
            let Expansion::Exhausted(exhausted) = outcome else {
                panic!("an empty plan means public search cannot help, not a failure");
            };
            assert_eq!(
                exhausted,
                Exhausted {
                    claims: 1,
                    searched: 0
                }
            );
            assert_eq!(
                exhausted.to_string(),
                "No further public search can help 1 unresolved claim (0 searches already tried)"
            );
            assert_eq!(llm.prompts.lock().unwrap().len(), 1, "only the planner ran");
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
        claims: Mutex<Vec<(usize, String)>>,
        plans: Mutex<usize>,
    }
    #[async_trait::async_trait]
    impl LLMPort for Model {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            let mut replies = Vec::new();
            for prompt in fixture_prompts(request) {
                replies.push(self.respond(&prompt).await?);
            }
            Ok(fixture_completion(request, replies))
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
    impl Model {
        async fn respond(&self, prompt: &str) -> Result<String> {
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
                    json!({"needs":[{"gap":0,"kind":"web","query":if *plans == 1 {"random assignment confounding official guidance"} else {"allocation concealment official guidance"}}]})
                        .to_string(),
                );
            }
            if prompt.starts_with("Select trustworthy references") {
                return Ok(selection(&[0]));
            }
            if prompt.starts_with("Source passages:") {
                let (passages, tail) = prompt.split_once("\n\nClaim: ").expect("claim request");
                let claim = tail.split("\n\n").next().unwrap();
                let research_round = *self.plans.lock().unwrap();
                self.claims
                    .lock()
                    .unwrap()
                    .push((research_round, claim.into()));
                let quote = claim;
                return Ok(if passages.contains(quote) {
                    format!("supported\nReason: The complete capture directly states the claim.\nSource quote: {quote}\nSource passage: passage-0")
                } else {
                    "unsupported\nReason: The requested relationship is missing from the capture.\nSource quote: none".into()
                });
            }
            panic!("Unexpected authoring or repair request")
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
    let runtimes = crate::features::learning::embedded_runtime::BuiltinRuntimes::for_tests();
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
            &runtimes,
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
    assert_eq!(
        checks
            .iter()
            .filter(|(_, claim)| claim == &original)
            .count(),
        1
    );
    // The missing claim had no retrieval hits on the first pass, so its first
    // model judgment happens only after research supplies a complete capture;
    // Later automatic research is scoped to the remaining failed claim. Even
    // when its page shares terminology with an already supported claim, that
    // completed comparison stays reusable.
    assert!(
        !checks
            .iter()
            .any(|(round, claim)| *round == 2 && claim == CAPTURE),
        "The second research round must not reopen a claim resolved by the first round: {checks:?}"
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

fn spec_finding(statement: &str, subject: &str, owner_hint: &str) -> Finding {
    use crate::features::learning::lessons::content_verification::provenance_gate::FindingTrust;
    use lattice_core::domain::provenance::*;
    Finding {
        unit: 0,
        quote: statement.into(),
        statement: statement.into(),
        verdict: ClaimVerdict::Supported,
        reason: "Supported".into(),
        evidence: vec![],
        supporting_quote: None,
        resolution: None,
        trust: Some(Box::new(FindingTrust {
            profile: ClaimProfile {
                kind: ClaimKind::Specification,
                subject: Some(ClaimSubject {
                    name: subject.into(),
                    kind: SubjectKind::Product,
                    aliases: vec![],
                    classes: vec![],
                    key: SubjectKey::of(subject),
                }),
                temporal: TemporalScope::Current,
                measurement_origin: None,
                attributed_to: None,
                reasons: vec![],
                policy: "test".into(),
                model: "test".into(),
            },
            supports: vec![],
            contrary: vec![],
            decision: Decision::Insufficient {
                bar: Bar::Primary,
                have: vec![],
                missing: Missing::NoPrimaryForSubject,
                next: vec![NextStep::SearchOwner {
                    origin_hints: vec![owner_hint.into()],
                }],
                reasons: vec![],
            },
        })),
    }
}

#[tokio::test]
async fn specification_gap_with_known_owner_issues_official_docs_search_need() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    program.summary.status = crate::features::learning::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;

    let target_url = "https://learn.microsoft.com/en-us/dotnet/api/system.threading.asynclocal-1";
    let second_url = "https://learn.microsoft.com/en-us/dotnet/fundamentals/runtime-libraries/system-threading-asynclocal";
    let capture = "AsyncLocal<T> represents ambient data local to an asynchronous flow.";
    let web = web_capture(false, target_url, capture);
    web.0.set_search_results(vec![
        WebSearchResult {
            title: "AsyncLocal<T> Class".into(),
            url: target_url.into(),
            snippet: "SEARCH_SNIPPET_MUST_NOT_BE_EVIDENCE".into(),
            published_date: None,
            source: Some("site_discovery".into()),
        },
        WebSearchResult {
            title: "AsyncLocal<T> Fundamentals".into(),
            url: second_url.into(),
            snippet: "SEARCH_SNIPPET_MUST_NOT_BE_EVIDENCE".into(),
            published_date: None,
            source: Some("site_discovery".into()),
        },
    ]);
    web.0.set_url_content(
        second_url,
        FetchUrlContentOutput {
            url: second_url.into(),
            title: Some("AsyncLocal<T> Fundamentals".into()),
            content: format!("\r\n{capture}\r\n"),
            content_truncated: false,
            word_count: 12,
            fetch_time_ms: 1.,
            content_type: None,
            from_cache: false,
            read_via: Default::default(),
        },
    );
    let finding = spec_finding("AsyncLocal in .NET", ".NET", "learn.microsoft.com");

    let llm = ScriptedModel {
        outputs: Mutex::new(
            [
                // The subject comes from the gap the need serves; the host
                // only because the planner set it.
                json!({"needs":[{"gap":0,"query":"AsyncLocal value flow across await","kind":"official_docs","site":"learn.microsoft.com"}]}).to_string(),
                selection(&[0]),
                json!({"needs":[{"gap":0,"query":"AsyncLocal flow","kind":"official_docs","subject":".NET","site":"learn.microsoft.com"}]}).to_string(),
                selection(&[0]),
                json!({"needs":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: Mutex::new(vec![]),
    };

    run(
        pool,
        program.summary.id,
        program.summary.goal,
        Some(web.clone()),
        async {
            let expanded = expand(&llm, &references, &[finding], &[], &ClaimChecks::default())
                .await?
                .saved()
                .expect("saved capture");
            assert_eq!(expanded.sources.len(), sources.len() + 1);

            let recorded = web.recorded_inputs();
            assert!(!recorded.is_empty(), "At least one search should be issued");
            let input = &recorded[0];
            assert_eq!(input.query, "AsyncLocal value flow across await");
            assert_eq!(input.kind.as_deref(), Some("official_docs"));
            assert_eq!(input.subject.as_deref(), Some(".NET"));
            assert_eq!(input.site.as_deref(), Some("learn.microsoft.com"));

            // Structured needs and empty needs both execute cleanly
            let exp2 = expand(&llm, &references, &[gap()], &[], &ClaimChecks::default()).await?;
            assert!(exp2.saved().is_some());
            let exhausted =
                expand(&llm, &references, &[gap()], &[], &ClaimChecks::default()).await?;
            assert!(matches!(exhausted, Expansion::Exhausted(_)));
            Ok(())
        },
    )
    .await
}

/// A supported finding whose gate asks only to follow `urls`.
fn citing_finding(urls: &[&str]) -> Finding {
    use lattice_core::domain::provenance::{Decision, NextStep};
    let mut finding = spec_finding(CAPTURE, "Random assignment", "example.org");
    if let Some(Decision::Insufficient { next, .. }) =
        finding.trust.as_mut().map(|trust| &mut trust.decision)
    {
        *next = urls
            .iter()
            .map(|url| NextStep::FollowCitation {
                url: (*url).into(),
                from: "citing-version".into(),
            })
            .collect();
    }
    finding
}

#[tokio::test]
async fn a_link_to_a_place_in_a_saved_page_never_fetches_it_again() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    program.summary.status = crate::features::learning::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let web = web(false);
    let empty_plan = || json!({"needs":[]}).to_string();
    let llm = ScriptedModel {
        outputs: Mutex::new([empty_plan(), empty_plan()].into()),
        prompts: Mutex::new(vec![]),
    };
    run(
        pool,
        program.summary.id,
        program.summary.goal,
        Some(web.clone()),
        async {
            let page = "https://example.org/research";
            let first = [citing_finding(&[page])];
            let expanded = expand(&llm, &references, &first, &[], &ClaimChecks::default())
                .await?
                .saved()
                .expect("the cited page is saved");
            assert_eq!(web.fetched(), vec![page.to_owned()]);
            let places = [citing_finding(&[
                "https://example.org/research#why-models-need-tools",
                "https://example.org/research#exercises",
            ])];
            let again = expand(&llm, &expanded, &places, &[], &ClaimChecks::default()).await?;
            assert!(again.saved().is_none(), "nothing new to save");
            assert_eq!(
                web.fetched(),
                vec![page.to_owned()],
                "links to places in the saved page are that page"
            );
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn a_search_for_an_independent_source_never_offers_the_supports_own_origin() -> Result<()> {
    use lattice_core::domain::provenance::{
        AddedBy, ClusterId, Decision, NextStep, PassageKind, SourceDates, Support,
        TemporalValidity, Trust, TrustLevel,
    };
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    program.summary.status = crate::features::learning::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    // The claim's one support is a blog post on example.com.
    let blog = crate::features::learning::dto::LearningSourceDto {
        id: "blog-version".into(),
        title: "Strict mode explained".into(),
        url: Some("https://blog.example.com/strict-mode".into()),
        excerpt: "Strict mode makes the model follow the schema.".into(),
        acquired_at: 0,
    };
    let references = ReferenceCollection::lexical(std::slice::from_ref(&blog))?;
    let mut finding = spec_finding(
        "Strict mode follows the schema.",
        "OpenAI API",
        "openai.com",
    );
    if let Some(trust) = finding.trust.as_mut() {
        trust.supports = vec![Support {
            source_version_id: blog.id.clone(),
            origin_cluster: ClusterId(blog.id.clone()),
            trust: Trust {
                level: TrustLevel::Established,
                reasons: vec![],
                override_: None,
            },
            dates: SourceDates::default(),
            passage: PassageKind::Direct,
            temporal: TemporalValidity::Valid,
            added_by: AddedBy::Research,
            cited_urls: vec![],
            owner_hints: vec![],
        }];
        if let Decision::Insufficient { next, .. } = &mut trust.decision {
            *next = vec![NextStep::SearchIndependent];
        }
    }
    let web = web(false);
    let result = |url: &str| WebSearchResult {
        title: "Strict mode".into(),
        url: url.into(),
        snippet: String::new(),
        published_date: None,
        source: Some("duckduckgo".into()),
    };
    web.0.set_search_results(vec![
        result("https://docs.example.com/strict-mode"),
        result("https://independent.org/structured-outputs-review"),
    ]);
    let llm = ScriptedModel {
        outputs: Mutex::new(
            [
                json!({"needs":[{"gap":0,"query":"structured outputs strict mode review","kind":"web"}]}).to_string(),
                selection(&[]),
            ]
            .into(),
        ),
        prompts: Mutex::new(vec![]),
    };
    run(
        pool,
        program.summary.id,
        program.summary.goal,
        Some(web.clone()),
        async {
            expand(&llm, &references, &[finding], &[], &ClaimChecks::default()).await?;
            Ok::<_, AppError>(())
        },
    )
    .await?;
    let prompts = llm.prompts.lock().unwrap();
    assert!(
        prompts[0].contains("\"independentOf\":[\"example.com\"]"),
        "the planner is told which origins to avoid"
    );
    let offered = &prompts[1];
    assert!(offered.contains("independent.org"));
    assert!(
        !offered.contains("docs.example.com"),
        "the supports' own origin is dropped before selection"
    );
    assert!(offered.contains("\"needKind\":\"web\""));
    Ok(())
}
