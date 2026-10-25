//! The standing reach (claim sources design §7) over a real course: its
//! sources saved with their provenance, their standing, ownership and the
//! claim's profile already judged, so the only model calls are the claim
//! checks. A claim whose support is below its bar reads the course's
//! strongest sources on its subject before it waits; a claim whose support
//! is the strongest the course has is verified as best available.

use super::batch_scheduler_tests::setup;
use super::evidence_paging_tests::{related_text, PagingModel, CLAIM, MARKER};
use super::*;
use crate::features::learning::dto::{LearningSourceKind, LearningSourcePolicy};
use crate::features::learning::lesson_drafts;
use crate::features::learning::references::trust::SqliteSourceTrustStore;
use crate::features::learning::source_library::{
    CapturedLearningSource, LearningSourceLibraryRepository, NewLearningSource,
};
use lattice_core::application::ports::source_trust::{SourceTrustStore, StoredJudgement};
use lattice_core::application::services::source_trust::{
    claim_profile_key, standing_key, OWNERSHIP_POLICY, STANDING_POLICY,
};
use lattice_core::domain::learning::claim_review::SourceAddedBy;
use lattice_core::domain::provenance::{
    AuthorityAssessment, ClaimKind, ClaimProfile, ClaimSubject, Decision, JudgementKey, Label,
    Origin, ProvenanceSignals, Reason, ReasonCode, SubjectKind, SubjectRelation, TemporalScope,
    TrustLevel, PAGE_SIGNALS_VERSION,
};

const MODEL: &str = "paging-fixture";

fn subject() -> ClaimSubject {
    ClaimSubject::new("Mineral alpha", SubjectKind::Other, vec![], vec![])
}

/// A specification claim: the static bar is Authoritative, so a community
/// source alone, or an identified one, does not meet it.
fn profile() -> ClaimProfile {
    ClaimProfile {
        kind: ClaimKind::Specification,
        subject: Some(subject()),
        temporal: TemporalScope::Timeless,
        measurement_origin: None,
        attributed_to: None,
        reasons: vec![],
        policy: "fixture".into(),
        model: MODEL.into(),
    }
}

/// One saved course source: its version id, its text and how it stands.
struct Saved {
    version_id: String,
    text: String,
}

/// The standing a fixture source was judged to have.
#[derive(Clone, Copy)]
enum Stands {
    /// A community forum: Community.
    Community,
    /// An identified author's site: Established.
    Identified,
    /// A museum's curated record: Authoritative.
    Institutional,
    /// Nobody identified, an unstructured text: Unknown.
    Unknown,
    /// Captured, but no checker has judged its standing yet.
    Unjudged,
}

struct Course {
    pool: sqlx::SqlitePool,
    program_id: String,
    store: SqliteSourceTrustStore,
}

impl Course {
    async fn save(&self, url: &str, text: String, stands: Stands) -> Result<Saved> {
        self.save_by(url, text, stands, SourceAddedBy::Research)
            .await
    }

    async fn save_by(
        &self,
        url: &str,
        text: String,
        stands: Stands,
        added_by: SourceAddedBy,
    ) -> Result<Saved> {
        let library = LearningSourceLibraryRepository::new(self.pool.clone());
        let id = || uuid::Uuid::new_v4().to_string();
        let (source_id, version_id) = (id(), id());
        let signals: ProvenanceSignals = serde_json::from_value(json!({
            "source": "web",
            "origin": Origin::of_url(url).expect("test URL"),
            "resolved_url": url,
            "dates": { "acquired_at": 1_760_000_000 },
            "extraction_version": PAGE_SIGNALS_VERSION,
        }))?;
        library
            .add_by(
                NewLearningSource {
                    operation_id: &id(),
                    source_id: &source_id,
                    version_id: &version_id,
                    program_id: &self.program_id,
                    kind: LearningSourceKind::Web,
                    operation_kind: "add_test",
                    origin: url,
                    requested_url: Some(url),
                    freshness: LearningSourcePolicy::Manual,
                },
                CapturedLearningSource {
                    title: format!("Page at {url}"),
                    publisher: None,
                    requested_url: Some(url.into()),
                    resolved_url: Some(url.into()),
                    text: text.clone(),
                    truncated: false,
                    extraction_version: "standing_test_v1".into(),
                    signals,
                },
                id(),
                added_by,
            )
            .await?;
        let record = self
            .store
            .provenance(&version_id)
            .await?
            .expect("capture records provenance");
        let content = crate::features::learning::references::trust::judgements::content_evidence(
            &self.pool,
            &self.program_id,
            &record,
        )
        .await?
        .expect("the version is the course's");
        let key = standing_key(&record.signals, &content, MODEL);
        let (standing, publisher, editorial, nature) = match stands {
            Stands::Community => ("community", Value::Null, "community_edited", "discussion"),
            Stands::Identified => (
                "identified",
                json!({"name": "A. Mineralogist"}),
                "self_published",
                "reference",
            ),
            Stands::Institutional => (
                "institutional",
                json!({"name": "Example Museum"}),
                "editorial",
                "reference",
            ),
            Stands::Unknown => ("unknown", Value::Null, "none", "reference"),
            Stands::Unjudged => ("", Value::Null, "", ""),
        };
        if !matches!(stands, Stands::Unjudged) {
            self.store
                .save_standing(&StoredJudgement {
                    value: serde_json::from_value(json!({
                        "standing": standing,
                        "publisher": publisher,
                        "publisher_kind": "publisher",
                        "editorial": editorial,
                        "nature": nature,
                        "flags": [],
                        "reasons": [{ "code": "publisher_metadata", "text": "fixture" }],
                        "policy": STANDING_POLICY,
                        "model": MODEL,
                        "signals_sha256": key.input_sha256,
                    }))?,
                    key,
                    input_tokens: 0,
                    output_tokens: 0,
                    assessed_at: 0,
                })
                .await?;
        }
        self.store
            .save_authority(
                &self.program_id,
                &StoredJudgement {
                    key: JudgementKey {
                        policy: OWNERSHIP_POLICY.into(),
                        model: MODEL.into(),
                        input_sha256: format!("fixture-{version_id}"),
                    },
                    value: AuthorityAssessment {
                        origin_key: record.origin.key.clone(),
                        subject_key: subject().key,
                        relation: SubjectRelation::Independent,
                        basis: vec![Reason::new(ReasonCode::ThirdParty, "fixture")],
                        owner_origin_hints: vec![],
                        policy: OWNERSHIP_POLICY.into(),
                        model: MODEL.into(),
                    },
                    input_tokens: 0,
                    output_tokens: 0,
                    assessed_at: 0,
                },
            )
            .await?;
        Ok(Saved { version_id, text })
    }
}

/// A course of twelve unidentified notes on the subject, `cited` (the
/// source the section cites, which states the claim) and, when given, the
/// museum's record, which states it last; the claim's profile is stored.
async fn course(
    pool: &sqlx::SqlitePool,
    lesson_id: &str,
    cited: Stands,
    museum: bool,
) -> Result<(Course, Vec<LearningSourceDto>, String)> {
    let program_id: String =
        sqlx::query_scalar("SELECT program_id FROM learning_lessons WHERE id=?")
            .bind(lesson_id)
            .fetch_one(pool)
            .await
            .map_err(|error| AppError::Database(error.to_string()))?;
    let course = Course {
        pool: pool.clone(),
        program_id,
        store: SqliteSourceTrustStore::new(pool.clone()),
    };
    let mut saved = Vec::new();
    for index in 0..12 {
        saved.push(
            course
                .save(
                    &format!("https://notes.example.com/alpha/{index}"),
                    related_text(index),
                    Stands::Unknown,
                )
                .await?,
        );
    }
    let cited = course
        .save(
            "https://forum.example.net/alpha",
            format!("Mineral alpha thread: {MARKER}"),
            cited,
        )
        .await?;
    let cited_id = cited.version_id.clone();
    saved.push(cited);
    if museum {
        let lines: String = (0..40)
            .map(|line| format!("Museum catalogue line {line} is filed. "))
            .collect();
        saved.push(
            course
                .save(
                    "https://museum.example.org/minerals/alpha",
                    format!("Mineral alpha record: property was logged. {lines}{MARKER}"),
                    Stands::Institutional,
                )
                .await?,
        );
    }
    course
        .store
        .save_claim_profiles(&[StoredJudgement {
            key: claim_profile_key(CLAIM, &[], MODEL),
            value: profile(),
            input_tokens: 0,
            output_tokens: 0,
            assessed_at: 0,
        }])
        .await?;
    let sources = saved
        .into_iter()
        .map(|saved| LearningSourceDto {
            title: format!("Reference {}", saved.version_id),
            id: saved.version_id,
            url: None,
            excerpt: saved.text,
            acquired_at: 1,
        })
        .collect();
    Ok((course, sources, cited_id))
}

/// Pass A cites `cited`'s passage stating the claim; pass B reads its first
/// page only, so nothing past it is read but by the standing reach.
async fn checked(
    model: &PagingModel,
    course: &Course,
    references: &ReferenceCollection<'_>,
    cited: &str,
) -> Result<Vec<CheckedClaim>> {
    let cites: Vec<EvidencePassage> = references
        .cited_passages(&[MARKER])?
        .into_iter()
        .filter(|passage| passage.source_id == cited)
        .collect();
    let citations = Citations::new(HashMap::from([(0, cites)]), HashMap::new());
    provenance_gate::with_trust(
        course.pool.clone(),
        course.program_id.clone(),
        "Minerals".into(),
        evidence_checks::check_with_paging(
            model,
            references,
            &evidence_paging_tests::inventory(&[CLAIM.into()]),
            &[],
            &ClaimChecks::default(),
            (
                &citations,
                Paging {
                    reaches: REACHES,
                    depth: Depth::FirstPage,
                },
            ),
        ),
    )
    .await
}

#[tokio::test]
async fn a_claim_left_insufficient_reads_the_courses_strongest_sources_before_waiting() -> Result<()>
{
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let (course, sources, cited) = course(&pool, &lesson, Stands::Community, true).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let museum = sources.last().unwrap().id.clone();
    let model = PagingModel::new(6_000);
    lesson_drafts::run(&repo, &job, &lesson, async {
        let results = checked(&model, &course, &references, &cited).await?;
        let finding = &results[0].2;
        assert_eq!(finding.verdict, ClaimVerdict::Supported);
        assert_eq!(finding.two_pass.reach, Some(Reach::Standing));
        assert_eq!(finding.two_pass.page, Some(1), "one standing page read");
        assert_eq!(
            finding.two_pass.supported_by.as_ref().unwrap().source_id,
            museum
        );
        assert!(finding
            .two_pass
            .also_supported
            .iter()
            .any(|other| other.passage.source_id == cited));
        let trust = finding.trust.as_ref().expect("weighed again");
        assert!(
            matches!(trust.decision, Decision::Sufficient { .. }),
            "{}",
            trust.explain()
        );
        assert_eq!(
            trust.available.and_then(|available| available.ceiling),
            Some(TrustLevel::Authoritative)
        );
        // The standing page's check has its receipt: a second pass restores
        // every check and asks the model nothing.
        let asked = model.checks.lock().unwrap().len();
        let again = checked(&model, &course, &references, &cited).await?;
        assert_eq!(model.checks.lock().unwrap().len(), asked);
        assert_eq!(again[0].2.two_pass.reach, Some(Reach::Standing));
        Ok(())
    })
    .await
}

/// The lesson that stalled: a section cites a community page, the course
/// holds the vendor's own documentation, and no checker ever judged that
/// documentation (an upgraded standing policy leaves every old judgement
/// unread). The reach must read it, judge it, and settle the claim.
#[tokio::test]
async fn an_official_document_nobody_has_judged_is_read_judged_and_settles_the_claim() -> Result<()>
{
    use std::sync::atomic::Ordering;
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let (course, mut sources, cited) = course(&pool, &lesson, Stands::Community, false).await?;
    let lines: String = (0..40)
        .map(|line| format!("Reference section {line} is listed. "))
        .collect();
    let docs = course
        .save(
            "https://docs.example-vendor.com/minerals/alpha",
            format!("Mineral alpha documentation: property was logged. {lines}{MARKER}"),
            Stands::Unjudged,
        )
        .await?;
    let docs_id = docs.version_id.clone();
    sources.push(LearningSourceDto {
        id: docs.version_id,
        title: "Vendor documentation".into(),
        url: None,
        excerpt: docs.text,
        acquired_at: 1,
    });
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000).judging_institutional("docs.example-vendor.com");
    lesson_drafts::run(&repo, &job, &lesson, async {
        let results = checked(&model, &course, &references, &cited).await?;
        let finding = &results[0].2;
        assert_eq!(finding.verdict, ClaimVerdict::Supported);
        assert_eq!(finding.two_pass.reach, Some(Reach::Standing));
        assert_eq!(
            finding.two_pass.supported_by.as_ref().unwrap().source_id,
            docs_id
        );
        let trust = finding.trust.as_ref().expect("weighed again");
        let Decision::Sufficient { met_by, .. } = &trust.decision else {
            panic!("the vendor's documentation states it: {}", trust.explain());
        };
        assert!(met_by.contains(&docs_id), "{}", trust.explain());
        assert!(
            model.standing_requests.load(Ordering::SeqCst) >= 1,
            "the documentation is judged once it is read"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_claim_with_no_stronger_source_is_verified_as_best_available_without_a_standing_page(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let (course, sources, cited) = course(&pool, &lesson, Stands::Identified, false).await?;
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    lesson_drafts::run(&repo, &job, &lesson, async {
        let results = checked(&model, &course, &references, &cited).await?;
        let finding = &results[0].2;
        assert_eq!(finding.verdict, ClaimVerdict::Supported);
        assert_ne!(finding.two_pass.reach, Some(Reach::Standing));
        let trust = finding.trust.as_ref().expect("weighed");
        assert!(
            matches!(
                trust.decision,
                Decision::Sufficient {
                    label: Some(Label::BestAvailable {
                        level: TrustLevel::Established
                    }),
                    ..
                }
            ),
            "{}",
            trust.explain()
        );
        // Pass A's check and pass B's first page: no standing page.
        assert_eq!(model.checks.lock().unwrap().len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn reading_one_silent_passage_does_not_hide_the_rest_of_an_authoritative_document(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = setup(&pool).await?;
    let (course, mut sources, cited) = course(&pool, &lesson, Stands::Community, false).await?;
    let introduction = "Mineral alpha record: property was logged; this introduction does not state the finding.\n";
    let official = course
        .save(
            "https://museum.example.org/full-reference",
            format!(
                "{introduction}{}\nMineral alpha: {MARKER}",
                "Catalogue filing instructions and archive opening hours.\n".repeat(160)
            ),
            Stands::Institutional,
        )
        .await?;
    let official_id = official.version_id.clone();
    sources.push(LearningSourceDto {
        id: official.version_id,
        title: "Museum reference".into(),
        url: None,
        excerpt: official.text,
        acquired_at: 1,
    });
    let references = ReferenceCollection::lexical(&sources)?;
    let mut cites: Vec<_> = references
        .cited_passages(&[MARKER])?
        .into_iter()
        .filter(|passage| passage.source_id == cited)
        .collect();
    cites.push(EvidencePassage {
        source_id: official_id.clone(),
        text: introduction.into(),
        start_byte: 0,
        end_byte: introduction.len(),
        retrieval_kind: "citation".into(),
        score: 1.0,
    });
    let citations = Citations::new(HashMap::from([(0, cites)]), HashMap::new());
    let model = PagingModel::new(6_000);
    lesson_drafts::run(
        &repo,
        &job,
        &lesson,
        provenance_gate::with_trust(
            course.pool.clone(),
            course.program_id.clone(),
            "Minerals".into(),
            async {
                let results = evidence_checks::check_with_paging(
                    &model,
                    &references,
                    &evidence_paging_tests::inventory(&[CLAIM.into()]),
                    &[],
                    &ClaimChecks::default(),
                    (
                        &citations,
                        Paging {
                            reaches: REACHES,
                            depth: Depth::FirstPage,
                        },
                    ),
                )
                .await?;
                let finding = &results[0].2;
                let Decision::Sufficient { met_by, .. } = &finding.trust.as_ref().unwrap().decision
                else {
                    panic!("the full authoritative document contains support: {finding:?}");
                };
                // The section cites the document (its silent introduction),
                // so the support is the cited document's, not "elsewhere".
                assert!(met_by.contains(&official_id), "{met_by:?}");
                assert_eq!(finding.two_pass.reach, Some(Reach::Standing));
                Ok(())
            },
        ),
    )
    .await
}

#[tokio::test]
async fn the_standing_scope_is_built_from_stored_facts_without_a_model() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (_, _, lesson) = setup(&pool).await?;
    let (course, sources, cited) = course(&pool, &lesson, Stands::Community, false).await?;
    let text = |name: &str| format!("Mineral alpha {name} page: property beta under pressure.");
    let owner = course
        .save(
            "https://museum.example.org/minerals/alpha",
            text("owner"),
            Stands::Institutional,
        )
        .await?;
    let restricted = course
        .save(
            "https://survey.example.gov/alpha",
            text("survey"),
            Stands::Unknown,
        )
        .await?;
    let user = course
        .save_by(
            "https://notes.example.net/mine",
            text("user"),
            Stands::Unknown,
            SourceAddedBy::User,
        )
        .await?;
    let hinted = course
        .save(
            "https://collections.example-hinted.org/alpha",
            text("hinted"),
            Stands::Unknown,
        )
        .await?;
    let unjudged = course
        .save(
            "https://docs.example-vendor.com/alpha",
            text("vendor docs"),
            Stands::Unjudged,
        )
        .await?;
    let shown = course
        .save(
            "https://press.example.org/alpha",
            text("press"),
            Stands::Institutional,
        )
        .await?;
    // The museum owns the subject, and believes its records live on the
    // hinted host too.
    course
        .store
        .save_authority(
            &course.program_id,
            &StoredJudgement {
                key: JudgementKey {
                    policy: OWNERSHIP_POLICY.into(),
                    model: MODEL.into(),
                    input_sha256: "owner".into(),
                },
                value: AuthorityAssessment {
                    origin_key: Origin::of_url("https://museum.example.org/").unwrap().key,
                    subject_key: subject().key,
                    relation: SubjectRelation::Owner,
                    basis: vec![Reason::new(ReasonCode::PublisherIsOwner, "fixture")],
                    owner_origin_hints: vec!["collections.example-hinted.org".into()],
                    policy: OWNERSHIP_POLICY.into(),
                    model: MODEL.into(),
                },
                input_tokens: 0,
                output_tokens: 0,
                assessed_at: 0,
            },
        )
        .await?;
    // No model: every fact comes from the store and the library.
    let mut judgements =
        provenance_gate::Judgements::cached(pool.clone(), &course.program_id, MODEL).await?;
    let scope = judgements
        .standing_scope(&profile(), std::slice::from_ref(&shown.version_id))
        .await?;
    let expected: HashSet<String> = [&owner, &restricted, &user, &hinted, &unjudged]
        .iter()
        .map(|saved| saved.version_id.clone())
        .collect();
    assert!(scope.candidates.is_superset(&expected), "{scope:?}");
    // A current judgement below the ladder keeps a version out: the
    // community forum and the unknown notes; the shown press is support.
    for kept_out in sources
        .iter()
        .map(|source| &source.id)
        .chain([&shown.version_id, &cited])
    {
        assert!(!scope.candidates.contains(kept_out), "{kept_out} read");
    }
    // Anything else read is a version nobody judged (the lesson's own file).
    for other in scope.candidates.difference(&expected) {
        assert_eq!(scope.proxies.get(other), None);
        assert_eq!(scope.tier(other), 1);
    }
    assert_eq!(
        scope.proxies.get(&owner.version_id),
        Some(&TrustLevel::Primary)
    );
    assert_eq!(
        scope.proxies.get(&restricted.version_id),
        Some(&TrustLevel::Established)
    );
    assert_eq!(
        scope.proxies.get(&user.version_id),
        Some(&TrustLevel::Established)
    );
    assert_eq!(
        scope.proxies.get(&hinted.version_id),
        None,
        "a hinted host is read, never a level"
    );
    assert_eq!(
        scope.proxies.get(&unjudged.version_id),
        None,
        "a source nobody has judged is read, and levelled only once read"
    );
    assert_eq!(scope.owner_hosts, ["collections.example-hinted.org"]);
    // The reading order: the strongest and the led first, then unjudged.
    for first in [&owner, &restricted, &user, &hinted] {
        assert_eq!(scope.tier(&first.version_id), 0);
    }
    assert_eq!(scope.tier(&unjudged.version_id), 1);

    // A new checker inherits leads, never levels: it makes its own
    // ownership call, and stale judgements cannot raise its ceiling.
    let mut switched =
        provenance_gate::Judgements::cached(pool.clone(), &course.program_id, "new-checker")
            .await?;
    assert!(!switched.owner_judged(&profile()).await?);
    let switched_scope = switched
        .standing_scope(&profile(), std::slice::from_ref(&shown.version_id))
        .await?;
    assert!(switched_scope.candidates.is_superset(&expected));
    for unjudged_now in sources.iter().map(|source| &source.id) {
        assert!(
            switched_scope.candidates.contains(unjudged_now),
            "an old checker's judgement does not keep a version out"
        );
        assert_eq!(switched_scope.tier(unjudged_now), 1);
    }
    assert_eq!(switched_scope.proxies.get(&owner.version_id), None);
    assert_eq!(
        switched_scope.proxies.get(&restricted.version_id),
        Some(&TrustLevel::Established),
        "facts that need no judgement still level"
    );
    assert_eq!(
        switched_scope.tier(&owner.version_id),
        0,
        "the old owner leads"
    );
    assert_eq!(switched_scope.tier(&hinted.version_id), 0);
    assert_eq!(switched_scope.owner_hosts, scope.owner_hosts);

    // Once the new checker has judged the subject, its call is the lead: the
    // stale owner and the host it hinted fall back to the unjudged, and the
    // host the new checker names is read first.
    course
        .store
        .save_authority(
            &course.program_id,
            &StoredJudgement {
                key: JudgementKey {
                    policy: OWNERSHIP_POLICY.into(),
                    model: "new-checker".into(),
                    input_sha256: "new-checker-owner".into(),
                },
                value: AuthorityAssessment {
                    origin_key: Origin::of_url("https://press.example.org/").unwrap().key,
                    subject_key: subject().key,
                    relation: SubjectRelation::Independent,
                    basis: vec![Reason::new(ReasonCode::PublisherIsOwner, "fixture")],
                    owner_origin_hints: vec!["docs.example-vendor.com".into()],
                    policy: OWNERSHIP_POLICY.into(),
                    model: "new-checker".into(),
                },
                input_tokens: 0,
                output_tokens: 0,
                assessed_at: 1,
            },
        )
        .await?;
    let mut judged =
        provenance_gate::Judgements::cached(pool.clone(), &course.program_id, "new-checker")
            .await?;
    assert!(judged.owner_judged(&profile()).await?);
    let judged_scope = judged
        .standing_scope(&profile(), std::slice::from_ref(&shown.version_id))
        .await?;
    assert!(judged_scope.candidates.is_superset(&expected));
    assert_eq!(judged_scope.owner_hosts, ["docs.example-vendor.com"]);
    assert_eq!(judged_scope.tier(&unjudged.version_id), 0);
    assert_eq!(
        judged_scope.tier(&owner.version_id),
        1,
        "a superseded owner call no longer leads"
    );
    assert_eq!(judged_scope.tier(&hinted.version_id), 1);
    Ok(())
}
