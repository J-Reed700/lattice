#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use crate::content_verification::tests::{fixture_completion, fixture_prompts};
use crate::lessons::course_generation_tests::ScriptedModel;
use crate::repository::LearningRepository;
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, WebSearchOutput, WebSearchResult,
};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::MockWebService;
use lattice_core::shared::persistence::Database;

pub(crate) const CAPTURE: &str =
    "Random assignment reduces confounding in comparisons between treatment groups. A study that assigns participants to its groups by chance spreads every characteristic it did not measure across the groups in the same way, so the difference it finds afterward is not owed to who was placed where. Researchers give this as the reason a randomised comparison can support a causal claim where an observational one cannot. The method does not remove chance variation, which shrinks as the groups grow, and it does not repair a measurement that was biased or a trial in which many participants left. Those need their own safeguards, such as blinding the people who record the outcome and following up everyone who was assigned. Random assignment is also distinct from random sampling: sampling decides who is studied, assignment decides who receives which treatment, and the two protect different conclusions. Reports of trials state how the sequence of assignments was generated so a reader can judge whether the groups were comparable at the start.";
pub(crate) struct ResearchWeb(MockWebService, Arc<Mutex<Vec<WebSearchInput>>>);
#[async_trait::async_trait]
impl WebServiceTrait for ResearchWeb {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        self.1.lock().unwrap().push(input.clone());
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
impl ResearchWeb {
    pub(crate) fn recorded_inputs(&self) -> Vec<WebSearchInput> {
        self.1.lock().unwrap().clone()
    }
}
pub(crate) fn web(truncated: bool) -> Arc<ResearchWeb> {
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
    Arc::new(ResearchWeb(web, Arc::new(Mutex::new(Vec::new()))))
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
        claim_id: None,
        two_pass: Default::default(),
        label: None,
        class: None,
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

pub(crate) fn selection(indices: &[usize]) -> String {
    json!({"decisions":indices.iter().map(|i|json!({"id":format!("result-{i}"),"useForGap":true,"reason":"Directly relevant primary reference for the requested concept."})).collect::<Vec<_>>()}).to_string()
}

#[test]
fn a_long_reason_for_a_reference_is_read_not_refused() {
    let results = vec![WebSearchResult {
        title: "Relevant reference".into(),
        url: "https://example.org/reference".into(),
        snippet: "Discovery metadata only".into(),
        published_date: None,
        source: None,
    }];
    let reason = "It states the scoped point in its own words. ".repeat(20);
    let raw = json!({"decisions":[{"id":"result-0","useForGap":true,"reason":reason}]}).to_string();
    let selected = super::selection(&raw, &results);
    assert!(selected.is_ok(), "{:?}", selected.as_ref().err());
    assert_eq!(selected.unwrap()[0].1, reason.trim());
}

#[test]
fn the_selection_schema_bounds_no_text_but_a_blank() {
    let ids = [json!("result-0"), json!("result-1")];
    let bounds = crate::lessons::course_generation_tests::length_bounds(&selection_schema(&ids));
    assert!(bounds.is_empty(), "{bounds:?}");
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
async fn reference_selection_reads_the_results_its_window_holds_in_order() -> Result<()> {
    // Far more than a 128k window holds, as a paged catalog search brings.
    let results: Vec<_> = (0..3000)
        .map(|i| WebSearchResult {
            title: format!("Tonal centres in electronic dance music, part {i}"),
            url: format!("https://doi.org/10.1000/paper.{i}"),
            snippet: "A study of key and tonal centre in four-on-the-floor production.".into(),
            published_date: None,
            source: Some("crossref".into()),
        })
        .collect();
    let llm = ScriptedModel {
        outputs: Mutex::new([selection(&[0])].into()),
        prompts: Mutex::new(vec![]),
    };
    let candidates = found(results);
    let room = selection_room(&llm, "Tonality", "tonal centre", &[]);
    let cost = selection_cost(&llm, &[]);
    let selected =
        select_references(&llm, "Tonality", "tonal centre", &[], candidates.clone()).await?;
    assert_eq!(selected[0].url, "https://doi.org/10.1000/paper.0");

    let prompts = llm.prompts.lock().unwrap();
    let sent = prompts[0].matches("\"id\":\"result-").count();
    assert!(sent > 0 && sent < candidates.len(), "sent {sent}");
    assert!(prompts[0].contains("\"id\":\"result-0\""));
    assert!(!prompts[0].contains("paper.2999"));
    // The ones left out are the ones the room could not hold.
    let held: usize = candidates[..sent].iter().map(&cost).sum();
    assert!(held <= room);
    assert!(held + cost(&candidates[sent]) > room - room / 100);
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

/// The planner wrote its scope into the query; research searches the site
/// it named through the source finder's own scope, which keeps the search
/// there (and follows the site where it moved), and reads the page it chose
/// wherever the page's own redirects lead.
#[tokio::test]
async fn research_searches_the_site_the_planner_named_and_reads_a_page_where_it_moved() -> Result<()>
{
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let found = "https://docs.example.org/manual/random-assignment";
    let web = web_capture(false, found, CAPTURE);
    web.0.set_url_content(
        found,
        FetchUrlContentOutput {
            url: "https://manual.example.net/random-assignment".into(),
            title: Some("Random assignment".into()),
            content: CAPTURE.into(),
            content_truncated: false,
            word_count: 12,
            fetch_time_ms: 1.,
            content_type: None,
            from_cache: false,
            read_via: Default::default(),
        },
    );
    let llm = ScriptedModel {
        outputs: Mutex::new(
            [
                json!({"needs":[{"gap":0,"query":"site:docs.example.org/manual random assignment","kind":"web"}]}).to_string(),
                selection(&[0]),
            ]
            .into(),
        ),
        prompts: Mutex::new(vec![]),
    };
    run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web.clone()),
        async {
            let expanded = expand(&llm, &references, &[gap()], &[], &ClaimChecks::default())
                .await?
                .saved()
                .expect("the moved page is read and saved");
            assert_eq!(expanded.sources.len(), sources.len() + 1);
            let searched: Vec<_> = web
                .recorded_inputs()
                .into_iter()
                .map(|input| (input.query, input.site))
                .collect();
            assert_eq!(
                searched,
                [(
                    "random assignment".to_owned(),
                    Some("docs.example.org/manual".to_owned())
                )]
            );
            let saved = repo.verification_sources(&program.summary.id).await?;
            assert!(saved.iter().any(|source| source.excerpt == CAPTURE));
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn research_can_save_evidence_after_one_hundred_sources() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
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
    let library = crate::source_library::LearningSourceLibraryRepository::new(db.clone());
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
        db.clone(),
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
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let original = ReferenceCollection::lexical(&sources)?;
    let llm = model();
    let findings = vec![gap()];
    run(
        db.clone(),
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

fn saved_page(url: &str, text: &str) -> crate::source_library::CapturedLearningSource {
    crate::source_library::CapturedLearningSource {
        title: "Research methods".into(),
        publisher: None,
        requested_url: Some(url.into()),
        resolved_url: Some(url.into()),
        text: text.into(),
        truncated: false,
        extraction_version: "test_v1".into(),
        signals: crate::references::trust::capture::not_extracted("test fixture"),
    }
}

/// Saves two pages at once, each as a lesson that read the course's sources
/// before the other saved anything: how many saves returned a new source, and
/// how many sources the course gained.
async fn save_together(pages: [(&str, &str); 2]) -> Result<(usize, usize)> {
    let db = Database::new(crate::tests::pool().await?);
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let before = repo.verification_sources(&program.summary.id).await?.len();
    let context = Context {
        database: db.clone(),
        program_id: program.summary.id.clone(),
        topic: program.summary.goal.clone(),
        web: web(false),
        searched: Default::default(),
    };
    let library = crate::source_library::LearningSourceLibraryRepository::new(db.clone());
    let saving = Saving {
        context: &context,
        program_repo: &repo,
        library: &library,
    };
    let (mut first_sources, mut first_urls) = (Vec::new(), HashSet::new());
    let (mut second_sources, mut second_urls) = (Vec::new(), HashSet::new());
    let (first, second) = tokio::join!(
        saving.save(
            &mut first_sources,
            &mut first_urls,
            pages[0].0,
            saved_page(pages[0].0, pages[0].1),
            None
        ),
        saving.save(
            &mut second_sources,
            &mut second_urls,
            pages[1].0,
            saved_page(pages[1].0, pages[1].1),
            None
        ),
    );
    let returned = [first?, second?].iter().flatten().count();
    let held = repo.verification_sources(&program.summary.id).await?.len();
    Ok((returned, held - before))
}

#[tokio::test]
async fn two_lessons_saving_the_same_page_leave_one_source() -> Result<()> {
    let (returned, added) = save_together([
        ("https://example.org/same", CAPTURE),
        ("https://example.org/same#section", CAPTURE),
    ])
    .await?;
    assert_eq!((returned, added), (1, 1));
    Ok(())
}

#[tokio::test]
async fn two_lessons_saving_the_same_address_with_different_text_leave_one_source() -> Result<()> {
    let (returned, added) = save_together([
        ("https://example.org/same", CAPTURE),
        (
            "https://example.org/same",
            "The page was edited between the two reads.",
        ),
    ])
    .await?;
    assert_eq!((returned, added), (1, 1));
    Ok(())
}

#[tokio::test]
async fn two_lessons_saving_different_pages_leave_two_sources() -> Result<()> {
    let other = format!("{CAPTURE} A second page says more.");
    let (returned, added) = save_together([
        ("https://example.org/one", CAPTURE),
        ("https://example.org/two", other.as_str()),
    ])
    .await?;
    assert_eq!((returned, added), (2, 2));
    Ok(())
}

#[tokio::test]
async fn research_rejects_truncated_pages_and_does_not_search_for_judge_failures() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    run(
        db.clone(),
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
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web(false)),
        async {
            let llm = ScriptedModel {
                outputs: Mutex::new([json!({"needs":[]}).to_string()].into()),
                prompts: Mutex::new(vec![]),
            };
            let outcome = expand(&llm, &references, &[gap()], &[], &ClaimChecks::default()).await?;
            let Expansion::Exhausted(exhausted, _) = outcome else {
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
                "No further public search can help 1 unresolved statement (0 searches already tried)"
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
async fn a_hint_with_no_terms_besides_its_sites_is_not_searched_and_the_log_says_so() -> Result<()>
{
    let logs = lattice_core::shared::testing::logs::capture_logs(tracing::Level::TRACE);

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web(false)),
        async {
            let llm = ScriptedModel {
                outputs: Mutex::new([json!({"needs":[]}).to_string()].into()),
                prompts: Mutex::new(vec![]),
            };
            let hints = [(0, "site:docs.python.org".to_owned())];
            let outcome =
                expand(&llm, &references, &[gap()], &hints, &ClaimChecks::default()).await?;
            assert!(
                matches!(outcome, Expansion::Exhausted(..)),
                "a hint that is not a search adds none"
            );
            assert_eq!(llm.prompts.lock().unwrap().len(), 1, "only the planner ran");
            let logs = logs.text();
            assert!(
                logs.contains(
                    "A research hint is not a search: no terms are left once any site: operators are lifted out"
                ),
                "{logs}"
            );
            assert!(!logs.contains("3 to 240"), "{logs}");
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn expanded_collection_checks_new_evidence_and_reuses_unchanged_comparisons() -> Result<()> {
    const SECOND_CAPTURE: &str =
        "Allocation concealment prevents foreknowledge of treatment assignments. The people enrolling participants in a trial must not be able to see or guess which group the next participant will join, because a recruiter who can foresee the assignment may steer sicker or healthier people toward one group, deliberately or without noticing. Concealment is kept until the moment a participant is irrevocably enrolled, for example by sealed opaque envelopes, a central telephone service or a pharmacy that prepares identical containers. It differs from blinding, which hides the assignment from participants and assessors after enrolment and protects the measurement of outcomes rather than the make-up of the groups. A trial can conceal its allocation even when blinding is impossible, as in a surgical comparison, and reports of trials describe the method of concealment separately from the method that generated the sequence. Where concealment fails, studies tend to exaggerate the benefit of the treatment under test.";
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
                crate::lessons::content_verification::tests::fixture_response(prompt)
            {
                return Ok(reply);
            }
            if prompt.starts_with("Plan focused reference searches") {
                let mut plans = self.plans.lock().unwrap();
                *plans += 1;
                assert!(
                    *plans <= 1,
                    "A claim is researched once: one round serves every gap"
                );
                return Ok(
                    json!({"needs":[
                        {"gap":0,"kind":"web","query":"random assignment confounding official guidance"},
                        {"gap":1,"kind":"web","query":"allocation concealment official guidance"}
                    ]})
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

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let original = sources[0].excerpt.clone();
    let llm = Model {
        claims: Mutex::new(vec![]),
        plans: Mutex::new(0),
    };
    let candidate = json!({"blocks":[{"kind":"explanation","body":original},{"kind":"explanation","body":CAPTURE},{"kind":"explanation","body":SECOND_CAPTURE}],"questions":[]});
    let references = ReferenceCollection::lexical(&sources)?;
    let runtimes = crate::embedded_runtime::BuiltinRuntimes::for_tests();
    let (_, report) = run(
        db.clone(),
        program.summary.id,
        program.summary.goal,
        Some(Arc::new(ResearchRounds {
            first: web(false),
            second: web_capture(false, "https://example.org/concealment", SECOND_CAPTURE),
        })),
        async {
            let shape = crate::lessons::shape::FixedSchema(json!({"type":"object"}));
            let mut numbered = sources.clone();
            let started = super::super::restart(
                &llm,
                "{}",
                &shape,
                &candidate.to_string(),
                &references,
                &mut numbered,
            )
            .await?;
            super::super::verify_and_repair_with_references(
                &llm,
                &runtimes,
                "{}",
                &shape,
                candidate.to_string(),
                &references,
                (3000, &mut numbered),
                started,
            )
            .await
        },
    )
    .await?;
    assert!(report.issues.is_empty());
    assert_eq!(report.sources.len(), 3);
    assert_eq!(*llm.plans.lock().unwrap(), 1);
    let checks = llm.claims.lock().unwrap();
    assert_eq!(
        checks
            .iter()
            .filter(|(_, claim)| claim == &original)
            .count(),
        1
    );
    // The two missing claims had no retrieval hits on the first pass, so
    // their first model judgments happen only after research supplies a
    // complete capture of each, and the expanded collection judges each once.
    for claim in [CAPTURE, SECOND_CAPTURE] {
        assert_eq!(
            checks
                .iter()
                .filter(|(round, checked)| *round == 1 && checked == claim)
                .count(),
            1,
            "{checks:?}"
        );
    }
    for finding in &report.findings {
        assert_eq!(finding.verdict, ClaimVerdict::Supported);
        assert!(finding
            .evidence
            .iter()
            .any(|passage| passage.text.contains(&finding.statement)));
    }
    Ok(())
}

#[tokio::test]
async fn a_planned_official_docs_need_is_searched_with_its_subject_and_site() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;

    let target_url = "https://learn.microsoft.com/en-us/dotnet/api/system.threading.asynclocal-1";
    let second_url = "https://learn.microsoft.com/en-us/dotnet/fundamentals/runtime-libraries/system-threading-asynclocal";
    let capture = "AsyncLocal<T> represents ambient data local to an asynchronous flow. A value stored in an AsyncLocal belongs to the logical flow of control that set it, not to any one thread, so code running after an await sees the value its caller set even when a different thread resumes it. The runtime copies the value into each flow that the setting code starts, which is why a change made in a child task is not seen by its parent, while a change made before the child starts is seen by the child. Libraries use this to carry request identifiers, culture settings and logging scopes through layers of asynchronous calls without passing them as arguments. Because the copy is made when work is queued, a long-lived background loop started early keeps the values it was given and does not see later changes. A notification callback can be supplied to observe each change, whether the value was set directly or replaced when a flow moved to another thread, which helps to diagnose a value that appears to leak.";
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
    let finding = Finding {
        quote: "AsyncLocal in .NET".into(),
        statement: "AsyncLocal in .NET".into(),
        ..gap()
    };

    let llm = ScriptedModel {
        outputs: Mutex::new(
            [
                json!({"needs":[{"gap":0,"query":"AsyncLocal value flow across await","kind":"official_docs","subject":".NET","site":"learn.microsoft.com"}]}).to_string(),
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
        db.clone(),
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
            assert!(matches!(exhausted, Expansion::Exhausted(..)));
            Ok(())
        },
    )
    .await
}

/// Counts as a tokenizer does on notation and formulas: plain text at four
/// characters a token, as the estimate assumes, but every sign outside ASCII
/// (♯, ♭, subscript digits, arrows) a token of its own. Refuses, as the
/// gates do, a prompt its exact count leaves no room beside.
struct Notation {
    window: usize,
    counts: std::sync::atomic::AtomicUsize,
    refusals: std::sync::atomic::AtomicUsize,
    prompts: Mutex<Vec<String>>,
}

fn notation(window: usize) -> Notation {
    Notation {
        window,
        counts: Default::default(),
        refusals: Default::default(),
        prompts: Mutex::new(Vec::new()),
    }
}

fn exact_tokens(text: &str) -> usize {
    let wide = text.chars().filter(|c| !c.is_ascii()).count();
    (text.chars().count() - wide) / 4 + wide
}

#[async_trait::async_trait]
impl LLMPort for Notation {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        use lattice_core::application::ports::llm_port::CompletionInput;
        let rendered = request
            .input
            .iter()
            .filter_map(|item| match item {
                CompletionInput::Message { role, content } => Some(format!("{role}\n{content}")),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let prompt_tokens = exact_tokens(&rendered);
        if prompt_tokens >= self.window {
            self.refusals
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(AppError::PromptExceedsWindow {
                prompt_tokens,
                window: self.window,
                counted_exactly: true,
                message: "It was not sent.".into(),
            });
        }
        self.prompts.lock().unwrap().push(rendered);
        Ok(CompletionResponse::from_text(selection(&[0])))
    }
    fn model_name(&self) -> &str {
        "notation"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        self.counts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(exact_tokens(text))
    }
    fn counts_tokens_exactly(&self) -> bool {
        true
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn reference_selection_reads_fewer_dense_results_instead_of_refusing() -> Result<()> {
    // Music notation: the estimate reads it as prose, the tokenizer at about
    // twice that.
    let candidates = found(
        (0..40)
            .map(|i| WebSearchResult {
                title: format!("Spelling altered chords, part {i}"),
                url: format!("https://music.example.edu/harmony/{i}"),
                snippet: "C♯ E♭ G♯ B♭ ".repeat(30),
                published_date: None,
                source: None,
            })
            .collect(),
    );
    let (topic, query) = ("Chromatic harmony", "altered chord spelling");
    // The smallest window whose estimate holds every candidate.
    let total: usize = candidates
        .iter()
        .map(selection_cost(&notation(0), &[]))
        .sum();
    let mut window = 4096;
    while selection_room(&notation(window), topic, query, &[]) < total {
        window += 1024;
    }
    let llm = notation(window);
    let all: Vec<Value> = candidates
        .iter()
        .enumerate()
        .map(|(i, candidate)| shown(i, candidate, &[]))
        .collect();
    assert!(
        !model_call::fits(&llm, &selection_request(topic, query, &[], &all)).await,
        "counted exactly, every candidate is over the window"
    );

    let selected = select_references(&llm, topic, query, &[], candidates).await?;

    assert_eq!(selected[0].url, "https://music.example.edu/harmony/0");
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1, "one selection, sent once");
    let read = prompts[0].matches("\"id\":\"result-").count();
    assert!(read > 0 && read < 40, "read {read}");
    assert!(exact_tokens(&prompts[0]) < window);
    assert_eq!(
        llm.refusals.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "measured before it was sent, never refused"
    );
    Ok(())
}

#[tokio::test]
async fn reference_selection_leaves_the_gap_open_when_no_dense_result_fits() -> Result<()> {
    // Chemistry formulas: every subscript and arrow is a token of its own.
    let llm = notation(8192);
    let (topic, query) = ("Fermentation", "glucose to ethanol");
    let room = selection_room(&llm, topic, query, &[]);
    let formula = "C₆H₁₂O₆→2C₂H₅OH+2CO₂ ";
    let candidates = found(vec![WebSearchResult {
        title: "Alcoholic fermentation".into(),
        url: "https://chemistry.example.edu/fermentation".into(),
        snippet: formula.repeat((room * 4 * 9 / 10 - 400) / formula.chars().count()),
        published_date: None,
        source: None,
    }]);
    assert!(
        selection_cost(&llm, &[])(&candidates[0]) <= room,
        "the estimate holds the one result"
    );

    let selected = select_noted(&llm, topic, query, &[], candidates).await?;

    assert!(selected.is_empty(), "the gap stays open");
    assert!(llm.prompts.lock().unwrap().is_empty(), "nothing was sent");
    assert!(llm.counts.load(std::sync::atomic::Ordering::SeqCst) > 0);
    assert_eq!(llm.refusals.load(std::sync::atomic::Ordering::SeqCst), 0);
    Ok(())
}

/// Fails every call as a research round can: refused for size, as the
/// reference selection of a music theory lesson once was, or with an answer
/// the round cannot use.
struct Failing {
    refused: bool,
}

#[async_trait::async_trait]
impl LLMPort for Failing {
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        if self.refused {
            return Err(AppError::PromptExceedsWindow {
                prompt_tokens: 35_569,
                window: 32_768,
                counted_exactly: true,
                message: "This request's prompt is 35569 tokens (counted exactly), which leaves \
                          no room for an answer; the configured context window is 32768. It \
                          was not sent."
                    .into(),
            });
        }
        Ok(CompletionResponse::from_text(
            r#"{"decisions":[{"id":"result-99","useForGap":true,"reason":"A page."}]}"#,
        ))
    }
    fn model_name(&self) -> &str {
        "failing"
    }
    fn max_context_tokens(&self) -> usize {
        32_768
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_research_round_that_fails_leaves_the_claims_held_and_the_lesson_going() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web(false)),
        async {
            for refused in [true, false] {
                let outcome = expand(
                    &Failing { refused },
                    &references,
                    &[gap()],
                    &[],
                    &ClaimChecks::default(),
                )
                .await?;
                let Expansion::Unchanged(searched) = outcome else {
                    panic!("research adds evidence; it never stops the lesson");
                };
                assert!(searched.per_claim.is_some(), "the round is still timed");
                assert!(searched.sources_added.is_empty());
            }
            assert_eq!(
                repo.verification_sources(&program.summary.id).await?.len(),
                sources.len()
            );
            Ok(())
        },
    )
    .await
}

/// Searches the engines answered with nothing found reached an engine: the
/// round is not "unreached", so it counts as the claim's research round.
#[tokio::test]
async fn a_round_whose_searches_found_nothing_did_reach_a_search_engine() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let empty = web(false);
    empty.0.set_search_results(Vec::new());
    run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(empty),
        async {
            let outcome = expand(
                &model(),
                &references,
                &[gap()],
                &[],
                &ClaimChecks::default(),
            )
            .await?;
            let Expansion::Unchanged(searched) = outcome else {
                panic!("a search that found nothing adds nothing");
            };
            assert!(!searched.queries.is_empty(), "the search ran");
            assert!(
                searched.unresearched.is_none(),
                "an answer of nothing was an answer"
            );
            Ok(())
        },
    )
    .await
}

/// The pages `serving` offers, each a long page of its own.
fn serving(pages: &[(&str, &str)]) -> Arc<ResearchWeb> {
    let web = web_capture(false, pages[0].0, CAPTURE);
    web.0.set_search_results(
        pages
            .iter()
            .map(|(url, title)| WebSearchResult {
                title: (*title).into(),
                url: (*url).into(),
                snippet: "SEARCH_SNIPPET_MUST_NOT_BE_EVIDENCE".into(),
                published_date: None,
                source: None,
            })
            .collect(),
    );
    for (index, (url, title)) in pages.iter().enumerate() {
        web.0.set_url_content(
            *url,
            FetchUrlContentOutput {
                url: (*url).into(),
                title: Some((*title).into()),
                content: format!("{CAPTURE} Page {index}."),
                content_truncated: false,
                word_count: 170,
                fetch_time_ms: 1.,
                content_type: None,
                from_cache: false,
                read_via: Default::default(),
            },
        );
    }
    web
}

#[test]
fn a_round_fetches_at_most_two_pages_for_a_statement_and_twice_the_statements_in_all() {
    assert_eq!(
        Pages::of(Gaps::Claims, 7).announced(7),
        "Looking up 7 statements on the web, at most 14 pages"
    );
    assert_eq!(
        Pages::of(Gaps::Claims, 1).announced(1),
        "Looking up 1 statement on the web, at most 2 pages"
    );
    let mut pages = Pages::of(Gaps::Claims, 3);
    // The selector's first choice for a statement, then one page on its
    // owner's site, then no more.
    assert!(pages.allows(Some(0), false));
    pages.take(Some(0), false);
    assert!(!pages.allows(Some(0), false), "one general page");
    assert!(pages.allows(Some(0), true));
    pages.take(Some(0), true);
    assert!(!pages.allows(Some(0), true) && !pages.allows(Some(0), false));
    // Another statement's pages are its own.
    assert!(pages.allows(Some(1), false));
    // A hint's search stands behind no statement: only the total holds it.
    pages.take(None, false);
    assert!(pages.allows(None, false));
    pages.take(Some(1), false);
    pages.take(Some(1), true);
    pages.take(Some(2), false);
    assert!(!pages.allows(None, false), "six pages in all");
    assert!(!pages.allows(Some(2), true));
    // A lesson's topics have no statement and no bound.
    let topics = Pages::of(Gaps::Topics, 9);
    assert!(topics.allows(Some(4), false) && topics.allows(None, true));
}

#[tokio::test]
async fn a_round_fetches_the_selectors_first_choice_and_one_page_on_the_owners_site_and_no_more(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let web = serving(&[
        ("https://docs.example.org/guide", "Random assignment guide"),
        (
            "https://docs.example.org/reference",
            "Random assignment reference",
        ),
        ("https://docs.example.org/notes", "Random assignment notes"),
        (
            "https://other.example.net/post",
            "A post about random assignment",
        ),
        ("https://example.org/login", "Sign in"),
    ]);
    let llm = ScriptedModel {
        outputs: Mutex::new(
            [
                json!({"needs":[
                    {"gap":0,"query":"random assignment confounding","kind":"web"},
                    {"gap":0,"query":"random assignment guidance","kind":"official_docs","subject":"Example","site":"docs.example.org"}
                ]})
                .to_string(),
                selection(&[0, 1, 2]),
                selection(&[0, 1]),
            ]
            .into(),
        ),
        prompts: Mutex::new(vec![]),
    };
    run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web),
        async {
            let outcome = expand(&llm, &references, &[gap()], &[], &ClaimChecks::default()).await?;
            let Expansion::Saved(expanded, searched) = outcome else {
                panic!("the pages are sources");
            };
            // The selector chose three pages for the statement's general
            // search and two for its owner-site search; one of each is read.
            assert_eq!(searched.fetched, 2);
            assert_eq!(searched.sources_added.len(), 2);
            assert_eq!(expanded.sources.len(), sources.len() + 2);
            // A login page is never offered to the selector.
            let prompts = llm.prompts.lock().unwrap();
            assert!(prompts.iter().all(|prompt| !prompt.contains("Sign in")));
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn a_fetched_page_that_is_not_a_source_counts_against_the_pages_and_is_not_saved(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let web = web_capture(
        false,
        "https://example.org/research",
        "Please sign in to continue to the library.",
    );
    run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web),
        async {
            let outcome = expand(
                &model(),
                &references,
                &[gap()],
                &[],
                &ClaimChecks::default(),
            )
            .await?;
            let Expansion::Unchanged(searched) = outcome else {
                panic!("a page that is not a source adds nothing");
            };
            assert_eq!(searched.fetched, 1);
            assert!(searched.sources_added.is_empty());
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
async fn a_claim_research_round_records_what_it_cost_a_claim_on_each_researched_event() -> Result<()>
{
    use lattice_core::domain::learning::claim_review::{
        ClaimEvent, ClaimId, ClaimLocation, UnitKind,
    };
    const CLAIM: &str = "Mineral alpha shows property beta under pressure.";
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let statements = [CLAIM.to_owned(), format!("{CLAIM} It holds in the lab.")];
    let findings: Vec<Finding> = statements
        .iter()
        .map(|statement| Finding {
            unit: 0,
            quote: statement.clone(),
            statement: statement.clone(),
            verdict: ClaimVerdict::Unsupported,
            reason: "No passage states this.".into(),
            evidence: vec![],
            supporting_quote: None,
            resolution: None,
            trust: None,
            claim_id: None,
            two_pass: Default::default(),
            label: None,
            class: None,
        })
        .collect();
    // A planner that finds nothing public to search: the round ends
    // exhausted, and is timed all the same.
    let llm = ScriptedModel {
        outputs: Mutex::new([json!({"needs":[]}).to_string()].into()),
        prompts: Mutex::new(vec![]),
    };
    let expansion = run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(web(false)),
        expand(&llm, &references, &findings, &[], &ClaimChecks::default()),
    )
    .await?;
    let Expansion::Exhausted(_, searched) = expansion else {
        panic!("an empty plan exhausts the round");
    };
    let per_claim = searched.per_claim.expect("a round that ran is timed");
    let ms = u64::try_from(per_claim.as_millis()).unwrap();

    let mut ledger = claim_ledger::ClaimLedger::open(&llm, "{}").await?;
    let ids: Vec<ClaimId> = (0..statements.len())
        .map(|index| ClaimId(format!("claim-{index}")))
        .collect();
    for (id, statement) in ids.iter().zip(&statements) {
        ledger
            .append(
                id,
                ClaimEvent::Extracted {
                    location: ClaimLocation::locate(
                        0,
                        UnitKind::Block,
                        "body",
                        statement,
                        statement,
                    ),
                    statement: statement.clone(),
                },
            )
            .await?;
    }
    let round: Vec<(ClaimId, usize)> = ids.iter().map(|id| (id.clone(), 0)).collect();
    ledger
        .record_research(
            &lattice_core::domain::learning::acceptance::StandardPolicy::new(),
            &round,
            &searched,
            false,
        )
        .await?;
    // Every claim the round researched carries its share of the round.
    for id in &ids {
        let costs: Vec<Option<u64>> = ledger
            .entry(id)
            .unwrap()
            .events()
            .iter()
            .filter_map(|record| match &record.event {
                ClaimEvent::Researched {
                    round_ms_per_claim, ..
                } => Some(*round_ms_per_claim),
                _ => None,
            })
            .collect();
        assert_eq!(costs, vec![Some(ms)]);
    }
    Ok(())
}

#[test]
fn a_selection_round_trips_through_its_contract() {
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        super::ResearchQuery,
    >(&super::Selection {
        decisions: vec![super::Selected {
            id: "result-0".into(),
            use_for_gap: true,
            reason: "States the rule.".into(),
        }],
    });
}
