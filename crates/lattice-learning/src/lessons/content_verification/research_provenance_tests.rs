#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! A research round researches several claims at once. The page it chose for
//! a claim leads that claim's first page, whatever the round saved for the
//! others.
use super::readings_tests::claim;
use super::*;
use crate::repository::LearningRepository;
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, WebSearchInput, WebSearchOutput, WebSearchResult,
};
use lattice_core::application::ports::{MockWebService, WebServiceTrait};
use serde_json::json;
use std::sync::Arc;

const X: &str = "The status strip shows the operator and the station name.";
const Y: &str = "The rota panel lists the shift lead and the depot code.";
/// In the page chosen for X, which states it in other words. Of X's words it
/// alone holds "strip", so it is on X's subject.
const P_MARK: &str = "who is on duty";
/// In the page chosen for Y, which repeats X's other words in other
/// statements, so it outranks P by words.
const Q_MARK: &str = "mute the chime";
/// In the page a user's hint found.
const R_MARK: &str = "ledger shelf";

fn page_p() -> String {
    format!(
        "Console headers. Every console in the hall carries a narrow strip across its \
         top edge, known as the band. The band tells a passer-by {P_MARK} and where \
         the console stands in the building. \
         The band names the operator at the desk and the station of the console. \
         The text in the band is set in a plain face so it can be read from the doorway. \
         When the person at the desk changes, the band is rewritten at the handover. \
         Visitors are asked not to touch the band, and a cleaner wipes it once a week. \
         A band that has gone dark is reported to the maintenance office the same day. \
         A spare band is kept in the cabinet beside the first aid box. \
         The cabinet is checked on the first of every month by the building warden. \
         Replacement bands are ordered from the same printer as the door signs."
    )
}

fn page_q() -> String {
    let lines = [
        "The status lamp shows a red light when the line halts.",
        "The operator may {Q} for one minute after a halt.",
        "The station name is printed on every ticket roll.",
        "The status lamp shows a green light when the line runs.",
        "The operator logs each halt in the book by the door.",
        "The station name is not editable by the operator.",
        "The status lamp shows a blue light when a ticket roll runs out.",
        "The operator fits a new ticket roll before the next train leaves.",
        "The station name is read aloud by the announcer at every stop.",
        "The status lamp shows a white light while the doors are open.",
        "The operator closes the doors once the platform is clear.",
        "The status lamp shows an amber light during the weekly test.",
        "The operator signs the test sheet when the lamp is amber.",
        "The station name appears in the footer of each printed receipt.",
        "The status lamp shows no light when the power is cut.",
    ];
    format!(
        "Line notes. {}",
        lines
            .iter()
            .map(|line| line.replace("{Q}", Q_MARK))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn page_r() -> String {
    format!(
        "Archive notes. Old record books are kept on the {R_MARK} at the back of the hall. \
         Each book is labelled with the year it was opened and the year it was closed. \
         A book is never lent out, and a reader signs the list before opening one. \
         The warden counts the books at the turn of each year and files the count. \
         Books that have gone damp are dried flat and returned to their place. \
         A new book is opened when the last page of the old one has been written. \
         The oldest books are wrapped in cloth and kept in the lowest drawer. \
         A reader who finds a loose page hands it to the warden rather than putting it back. \
         The shelf is dusted every Friday, and each book is turned so its spine faces out. \
         A list on the wall gives the place of each book on the shelf by its year. \
         Pencil is the only thing a reader may write with at the reading table. \
         The reading table is cleared and wiped when the last reader of the day leaves."
    )
}

/// The page at `url`, served for the searches that mention `keyword`.
struct Served {
    keyword: &'static str,
    url: String,
    web: MockWebService,
}

struct Web(Vec<Served>);

impl Web {
    fn serving(pages: &[(&'static str, &str, String)]) -> Self {
        Self(
            pages
                .iter()
                .map(|(keyword, url, text)| {
                    let web = MockWebService::new();
                    web.set_search_results(vec![WebSearchResult {
                        title: "Reference page".into(),
                        url: (*url).into(),
                        snippet: "SEARCH_SNIPPET_MUST_NOT_BE_EVIDENCE".into(),
                        published_date: None,
                        source: None,
                    }]);
                    web.set_url_content(
                        *url,
                        FetchUrlContentOutput {
                            url: (*url).into(),
                            title: Some("Reference page".into()),
                            content: text.clone(),
                            content_truncated: false,
                            word_count: 170,
                            fetch_time_ms: 1.,
                            content_type: None,
                            from_cache: false,
                            read_via: Default::default(),
                        },
                    );
                    Served {
                        keyword,
                        url: (*url).into(),
                        web,
                    }
                })
                .collect(),
        )
    }

    fn of_url(&self, url: &str) -> Result<&MockWebService> {
        self.0
            .iter()
            .find(|served| served.url == url)
            .map(|served| &served.web)
            .ok_or_else(|| AppError::NotFound(format!("no page for {url}")))
    }
}

#[async_trait::async_trait]
impl WebServiceTrait for Web {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        let served = self
            .0
            .iter()
            .find(|served| input.query.contains(served.keyword))
            .ok_or_else(|| AppError::NotFound(format!("no page for {}", input.query)))?;
        served.web.search_web(input).await
    }
    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.of_url(url)?.fetch_url_content(url).await
    }
    async fn fetch_url_content(&self, _: &str) -> Result<FetchUrlContentOutput> {
        panic!("Lesson research must fetch full captures, not chat previews")
    }
    fn validate_url(&self, url: &str) -> Result<()> {
        self.of_url(url)?.validate_url(url)
    }
}

/// Plans one search for each of X and Y, and selects each search's first
/// result.
struct Planner;

impl Planner {
    fn respond(prompt: &str) -> String {
        if prompt.starts_with("Plan focused reference searches") {
            return json!({"needs":[
                {"gap":0,"kind":"web","query":"operator station status strip console"},
                {"gap":1,"kind":"web","query":"rota shift depot code panel"}
            ]})
            .to_string();
        }
        if prompt.starts_with("Select trustworthy references") {
            return research::tests::selection(&[0]);
        }
        panic!("Unexpected request: {prompt}")
    }
}

#[async_trait::async_trait]
impl LLMPort for Planner {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let replies = fixture_prompts(request)
            .iter()
            .map(|prompt| Self::respond(prompt))
            .collect();
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "research-provenance-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        32_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn gap(statement: &str) -> Finding {
    Finding {
        unit: 0,
        quote: statement.into(),
        statement: statement.into(),
        verdict: ClaimVerdict::Unsupported,
        reason: "No saved passage states this.".into(),
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

/// A page that holds one passage: the first one is read even over the budget.
fn one_passage() -> evidence_checks::PageBudget {
    evidence_checks::PageBudget {
        tokens: Some(1),
        unavailable: None,
    }
}

#[tokio::test]
async fn a_claims_first_page_leads_with_the_page_research_chose_for_it() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let llm = Planner;
    let retained = ClaimChecks::default();
    let (x, budget) = (claim(X), one_passage());
    let current = evidence_selection::sources(&references);
    evidence_selection::select(
        &llm,
        (&x, X, 0),
        &references,
        &current,
        &retained,
        (&[], &budget),
    )
    .await?;

    let web = Web::serving(&[
        ("operator", "https://example.org/headers", page_p()),
        ("rota", "https://example.org/line-notes", page_q()),
    ]);
    let findings = [gap(X), gap(Y)];
    let expansion = research::run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(Arc::new(web)),
        research::expand(&llm, &references, &findings, &[], &retained),
    )
    .await?;
    let research::Expansion::Saved(expanded, searched) = expansion else {
        panic!("both searches save a page");
    };
    assert_eq!(searched.sources_added.len(), 2);

    let current = evidence_selection::sources(&expanded);
    let selected = evidence_selection::select(
        &llm,
        (&x, X, 0),
        &expanded,
        &current,
        &retained,
        (&[], &budget),
    )
    .await?;
    let page: Vec<&str> = selected
        .evidence
        .iter()
        .map(|passage| passage.text.as_str())
        .collect();
    assert!(
        page.iter().any(|text| text.contains(P_MARK)),
        "the page research chose for the claim leads its page: {page:?}"
    );
    assert!(
        !page.iter().any(|text| text.contains(Q_MARK)),
        "the page chosen for the other claim does not take its place"
    );
    Ok(())
}

/// The id of the saved source whose text holds `mark`.
fn source_with(references: &ReferenceCollection<'_>, mark: &str) -> String {
    references
        .sources
        .iter()
        .find(|source| source.excerpt.contains(mark))
        .map(|source| source.id.clone())
        .unwrap_or_else(|| panic!("no saved source holds {mark:?}"))
}

#[tokio::test]
async fn a_saved_page_is_chosen_for_the_gap_whose_search_chose_it_and_a_hints_page_for_none(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let mut program = crate::tests::fixture();
    program.summary.status = crate::dto::LearningProgramStatus::Active;
    repo.create(&program).await?;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let web = Web::serving(&[
        ("operator", "https://example.org/headers", page_p()),
        ("rota", "https://example.org/line-notes", page_q()),
        ("ledger", "https://example.org/archive", page_r()),
    ]);
    let expansion = research::run(
        db.clone(),
        program.summary.id.clone(),
        program.summary.goal.clone(),
        Some(Arc::new(web)),
        research::expand(
            &Planner,
            &references,
            &[gap(X), gap(Y)],
            &[(0, "ledger shelf archive".into())],
            &ClaimChecks::default(),
        ),
    )
    .await?;
    let research::Expansion::Saved(expanded, searched) = expansion else {
        panic!("the searches save pages");
    };
    let (p, q, r) = (
        source_with(&expanded, P_MARK),
        source_with(&expanded, Q_MARK),
        source_with(&expanded, R_MARK),
    );
    let mut chosen = searched.chosen.clone();
    chosen.sort();
    let mut expected = vec![(0, p), (1, q)];
    expected.sort();
    assert_eq!(chosen, expected);
    assert!(
        searched.sources_added.contains(&r),
        "the hint's page is saved"
    );
    Ok(())
}

#[tokio::test]
async fn the_versions_research_chose_for_a_claim_are_restored_from_the_checkpoint() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = super::batch_scheduler_tests::setup(&db).await?;
    let source = |id: &str, text: String| LearningSourceDto {
        id: id.into(),
        title: format!("Reference {id}"),
        url: None,
        excerpt: text,
        acquired_at: 1,
    };
    let mut sources = vec![source("base", "Floor plan. The hall has two doors.".into())];
    crate::lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let (x, budget, retained) = (claim(X), one_passage(), ClaimChecks::default());
        let references = ReferenceCollection::lexical(&sources)?;
        let current = evidence_selection::sources(&references);
        let llm = Planner;
        evidence_selection::select(
            &llm,
            (&x, X, 0),
            &references,
            &current,
            &retained,
            (&[], &budget),
        )
        .await?;
        sources.push(source("chosen-for-x", page_p()));
        sources.push(source("saved-for-y", page_q()));
        let expanded = ReferenceCollection::lexical(&sources)?;
        evidence_selection::record_research_scope(
            &expanded,
            &[gap(X), gap(Y)],
            &[(0, "chosen-for-x".into()), (1, "saved-for-y".into())],
            &ClaimChecks::default(),
        )
        .await?;
        // The scope above is gone from memory: the checkpoint restores it.
        let current = evidence_selection::sources(&expanded);
        let selected = evidence_selection::select(
            &llm,
            (&x, X, 0),
            &expanded,
            &current,
            &retained,
            (&[], &budget),
        )
        .await?;
        assert!(selected
            .evidence
            .iter()
            .any(|passage| passage.text.contains(P_MARK)));
        assert!(!selected
            .evidence
            .iter()
            .any(|passage| passage.text.contains(Q_MARK)));
        Ok(())
    })
    .await
}
