//! The judgements the provenance gate reads for one verification pass:
//! overrides, relations, calibrated bars, each claim's profile, and each
//! supporting version's provenance, standing and ownership. A missing
//! judgement is made once with the lesson's model (§12) and saved, so later
//! re-decisions need no model (§18.3).
//! Design: docs/design/2026-10-18-source-provenance-and-trust.md §5, §12, §16;
//! docs/design/2026-10-21-claim-official-sources.md §4.3, §7.3.

use super::{SavedText, VersionFacts};
use crate::features::learning::references::trust::{calibration, SqliteSourceTrustStore};
use lattice_core::application::ports::{LLMPort, SourceTrustStore, StoredJudgement};
use lattice_core::application::services::source_trust::{
    claim_profile_key, judge_ownership, judge_standing, ownership_key, profile_claims,
    standing_key, ClaimToProfile, OwnershipRequest, ProfileRequest, OWNERSHIP_POLICY,
};
use lattice_core::domain::provenance::{
    containment, effective_date, known_classes, markdown_links, shingles, trust_level, AddedBy,
    AuthorityAssessment, Chosen, ClaimProfile, ClaimSubject, ContentEvidence, EffectiveBars,
    InheritedTrust, Judged, LevelInputs, Origin, Overrides, ProvenanceRecord, Reason, ReasonCode,
    RelationEvidence, RelationKind, RelationTarget, SourceDates, SourceRelation,
    StandingAssessment, TrustLevel, POLICY,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::Arc;

mod scope;

/// Milliseconds in a second: the library stores times in milliseconds, the
/// provenance domain in seconds.
const MILLIS_PER_SECOND: i64 = 1000;

/// The relations through which a version may inherit a level (§5 step 4).
const COPY_RELATIONS: [RelationKind; 4] = [
    RelationKind::CopyOf,
    RelationKind::ContainsCopyOf,
    RelationKind::ArchiveOf,
    RelationKind::CanonicalOf,
];

/// The course a lesson preparation job weighs its claims' support in.
#[derive(Clone)]
struct Course {
    pool: SqlitePool,
    program_id: String,
    /// The course and lesson the judgements are asked about (§12).
    topic: String,
}

tokio::task_local! {
    static COURSE: Course;
}

/// Runs `future` with the provenance gate on for a course: inside it every
/// supported claim's support is weighed, and passages carry their source's
/// label. The lesson preparation job installs it beside research
/// (`research::run`); outside it, nothing is weighed and the claim ledger's
/// gate applies the quotation rule alone.
pub(in crate::features::learning) async fn with_trust<T>(
    pool: SqlitePool,
    program_id: String,
    topic: String,
    future: impl std::future::Future<Output = T>,
) -> T {
    COURSE
        .scope(
            Course {
                pool,
                program_id,
                topic,
            },
            future,
        )
        .await
}

tokio::task_local! {
    static OUTLINE_POOL: SqlitePool;
}

/// Turns the provenance gate on for the outline that `future` drafts or
/// repairs. The outline commands install it, as the lesson preparation job
/// installs [`with_trust`]; the course may not exist yet, so the outline's
/// review and repair loop opens the course's scope itself
/// ([`outline_trust`]). Outside it (unit tests) no outline is weighed.
pub(in crate::features::learning) async fn with_outline_trust<T>(
    pool: SqlitePool,
    future: impl std::future::Future<Output = T>,
) -> T {
    OUTLINE_POOL.scope(pool, future).await
}

/// Runs `future` in `program_id`'s trust scope when an outline command
/// turned the gate on; otherwise runs it as it is.
pub(in crate::features::learning) async fn outline_trust<T>(
    program_id: String,
    topic: String,
    future: impl std::future::Future<Output = T>,
) -> T {
    match OUTLINE_POOL.try_with(Clone::clone) {
        Ok(pool) => with_trust(pool, program_id, topic, future).await,
        Err(_) => future.await,
    }
}

fn course() -> Option<Course> {
    COURSE.try_with(Clone::clone).ok()
}

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// A judgement a cache-only read needed and the store does not hold.
fn not_judged(what: &str, origin: &str) -> AppError {
    AppError::NotFound(format!(
        "No saved {what} judgement for {origin}; re-deciding without a model cannot level it."
    ))
}

fn stored<T>(
    key: lattice_core::domain::provenance::JudgementKey,
    judged: Judged<T>,
) -> StoredJudgement<T> {
    StoredJudgement {
        key,
        value: judged.value,
        input_tokens: judged.input_tokens,
        output_tokens: judged.output_tokens,
        assessed_at: now(),
    }
}

/// One supporting version as the gate reads it, whatever the subject.
struct VersionRecord {
    source_id: String,
    added_by: AddedBy,
    acquired_at: i64,
    saved: Arc<SavedText>,
    /// `None` when capture recorded no provenance for the version.
    judged: Option<(ProvenanceRecord, StandingAssessment)>,
}

/// The judgements of one verification pass.
pub(in crate::features::learning) struct Judgements {
    store: SqliteSourceTrustStore,
    pool: SqlitePool,
    program_id: String,
    topic: String,
    model: String,
    overrides: Overrides,
    relations: Vec<SourceRelation>,
    bars: EffectiveBars,
    /// Subject classes the user's site overrides name, so profiles reuse
    /// their wording and class matching stays exact.
    known_classes: Vec<String>,
    versions: HashMap<String, Arc<VersionRecord>>,
    /// The course's versions as the standing scope reads them, loaded once
    /// per pass (`scope`).
    course: Option<Arc<Vec<scope::CourseVersion>>>,
}

impl Judgements {
    /// The pass's judgements inside a preparation job; `None` elsewhere,
    /// where no provenance is stored.
    pub(in crate::features::learning::lessons::content_verification) async fn open(
        llm: &dyn LLMPort,
    ) -> Result<Option<Self>> {
        let Some(course) = course() else {
            return Ok(None);
        };
        Self::load(course, llm.model_name()).await.map(Some)
    }

    /// The judgements already made for a course under `model`, with the
    /// course's overrides, relations and bars as they are now: what a trust
    /// change re-decides from. Nothing here calls a model; a judgement the
    /// store lacks is an error (`facts` with no model).
    pub(in crate::features::learning) async fn cached(
        pool: SqlitePool,
        program_id: &str,
        model: &str,
    ) -> Result<Self> {
        let course = Course {
            pool,
            program_id: program_id.to_owned(),
            topic: String::new(),
        };
        Self::load(course, model).await
    }

    async fn load(course: Course, model: &str) -> Result<Self> {
        let store = SqliteSourceTrustStore::new(course.pool.clone());
        let overrides = store.overrides(&course.program_id).await?;
        let relations = store.relations(&course.program_id).await?;
        let bars = calibration::bars_of(&store.outcomes().await?);
        let known_classes = known_classes(&overrides.entries);
        Ok(Self {
            store,
            pool: course.pool,
            program_id: course.program_id,
            topic: course.topic,
            model: model.to_owned(),
            overrides,
            relations,
            bars,
            known_classes,
            versions: HashMap::new(),
            course: None,
        })
    }

    /// The profile stored for `statement` under this pass's model, if any;
    /// never a model call (the page-recall eval's standing order).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::features::learning) async fn profile_if_stored(
        &self,
        statement: &str,
    ) -> Result<Option<ClaimProfile>> {
        let key = claim_profile_key(statement, &self.known_classes, &self.model);
        Ok(self
            .store
            .claim_profile(&key)
            .await?
            .map(|saved| saved.value))
    }

    pub(in crate::features::learning::lessons::content_verification) fn relations(
        &self,
    ) -> &[SourceRelation] {
        &self.relations
    }

    pub(in crate::features::learning::lessons::content_verification) fn bars(
        &self,
    ) -> &EffectiveBars {
        &self.bars
    }

    /// The profile of each `(unit, statement)`, in order: from the store, or
    /// judged one call per lesson unit for the claims it lacks (§12.3).
    pub(in crate::features::learning::lessons::content_verification) async fn profiles(
        &mut self,
        llm: &dyn LLMPort,
        claims: &[(usize, &str)],
    ) -> Result<Vec<ClaimProfile>> {
        let mut known: HashMap<String, ClaimProfile> = HashMap::new();
        let mut missing: HashMap<usize, Vec<String>> = HashMap::new();
        for (unit, statement) in claims {
            if known.contains_key(*statement) {
                continue;
            }
            let key = claim_profile_key(statement, &self.known_classes, &self.model);
            match self.store.claim_profile(&key).await? {
                Some(saved) => {
                    known.insert((*statement).to_owned(), saved.value);
                }
                None => {
                    let unit_claims = missing.entry(*unit).or_default();
                    if !unit_claims.iter().any(|claim| claim.as_str() == *statement) {
                        unit_claims.push((*statement).to_owned());
                    }
                }
            }
        }
        let mut units: Vec<_> = missing.into_iter().collect();
        units.sort_by_key(|(unit, _)| *unit);
        for (unit, statements) in units {
            let to_profile: Vec<ClaimToProfile> = statements
                .iter()
                .enumerate()
                .map(|(index, statement)| ClaimToProfile {
                    id: format!("claim-{index}"),
                    statement: statement.clone(),
                })
                .collect();
            let unit_title = format!("Section {}", unit + 1);
            let judged = profile_claims(
                llm,
                &ProfileRequest {
                    unit_title: &unit_title,
                    course_topic: &self.topic,
                    known_classes: &self.known_classes,
                    claims: &to_profile,
                },
            )
            .await?;
            if judged.len() != statements.len() {
                return Err(AppError::InternalError(
                    "The claim profile call answered for a different number of claims.".into(),
                ));
            }
            let rows: Vec<_> = statements
                .iter()
                .zip(judged)
                .map(|(statement, judged)| {
                    known.insert(statement.clone(), judged.value.clone());
                    stored(
                        claim_profile_key(statement, &self.known_classes, &self.model),
                        judged,
                    )
                })
                .collect();
            self.store.save_claim_profiles(&rows).await?;
        }
        claims
            .iter()
            .map(|(_, statement)| {
                known.get(*statement).cloned().ok_or_else(|| {
                    AppError::InternalError("A claim was left without a profile.".into())
                })
            })
            .collect()
    }

    /// The facts of each of `versions` for `profile`'s subject: the version's
    /// level from its standing, its ownership of the subject, its copy
    /// relations and the overrides (§5). With no `llm`, only judgements
    /// already stored are read, and a missing one is an error.
    pub(in crate::features::learning::lessons::content_verification) async fn facts(
        &mut self,
        llm: Option<&dyn LLMPort>,
        versions: &[String],
        profile: &ClaimProfile,
    ) -> Result<HashMap<String, VersionFacts>> {
        let subject = profile.authority_subject();
        let mut facts = HashMap::new();
        for version in versions {
            let record = self.version(llm, version).await?;
            let inherited = self
                .inherited(llm, version, &record, subject.as_ref())
                .await?;
            let own = self
                .facts_of(llm, version, &record, subject.as_ref(), &inherited)
                .await?;
            facts.insert(version.clone(), own);
        }
        Ok(facts)
    }

    /// Levels a version may inherit through a copy relation to another
    /// version of the course, each that version's own level for the subject.
    async fn inherited(
        &mut self,
        llm: Option<&dyn LLMPort>,
        version: &str,
        record: &VersionRecord,
        subject: Option<&ClaimSubject>,
    ) -> Result<Vec<InheritedTrust>> {
        let copies: Vec<(String, RelationKind, Option<f64>)> = self
            .relations
            .iter()
            .filter(|relation| relation.from == version && COPY_RELATIONS.contains(&relation.kind))
            .filter_map(|relation| match &relation.to {
                RelationTarget::Version(to) => Some((
                    to.clone(),
                    relation.kind,
                    match relation.evidence {
                        RelationEvidence::Text { containment, .. } => Some(containment),
                        _ => None,
                    },
                )),
                RelationTarget::Url(_) => None,
            })
            .collect();
        let mut inherited = Vec::new();
        for (original, kind, measured) in copies {
            let source = self.version(llm, &original).await?;
            let contained = measured.unwrap_or_else(|| {
                containment(
                    &shingles(&source.saved.text, &POLICY),
                    &shingles(&record.saved.text, &POLICY),
                )
            });
            let level = self.facts_of(llm, &original, &source, subject, &[]).await?;
            inherited.push(InheritedTrust {
                of: original,
                of_origin: level.origin,
                relation: kind,
                level: level.trust.level,
                containment: contained,
            });
        }
        Ok(inherited)
    }

    async fn facts_of(
        &self,
        llm: Option<&dyn LLMPort>,
        version: &str,
        record: &VersionRecord,
        subject: Option<&ClaimSubject>,
        inherited: &[InheritedTrust],
    ) -> Result<VersionFacts> {
        let Some((provenance, standing)) = &record.judged else {
            return Ok(VersionFacts {
                origin: lattice_core::domain::provenance::OriginKey(format!("version:{version}")),
                trust: lattice_core::domain::provenance::Trust {
                    level: TrustLevel::Unknown,
                    reasons: vec![Reason::new(
                        ReasonCode::NoSignals,
                        "No provenance was recorded when this source was saved",
                    )],
                    override_: None,
                },
                dates: SourceDates {
                    acquired_at: record.acquired_at,
                    ..SourceDates::default()
                },
                added_by: record.added_by,
                saved: record.saved.clone(),
                owner_hints: Vec::new(),
            });
        };
        let relation = match subject {
            Some(subject) if provenance.origin.host.is_some() => {
                Some(self.authority(llm, provenance, subject).await?)
            }
            _ => None,
        };
        let trust = trust_level(&LevelInputs {
            program_id: &self.program_id,
            source_id: &record.source_id,
            origin: &provenance.origin,
            signals: &provenance.signals,
            standing,
            subject,
            relation: relation.as_ref(),
            inherited,
            overrides: &self.overrides,
            added_by: record.added_by,
            policy: &POLICY,
        });
        Ok(VersionFacts {
            origin: provenance.origin.key.clone(),
            trust,
            dates: provenance.signals.dates().cloned().unwrap_or(SourceDates {
                acquired_at: record.acquired_at,
                ..SourceDates::default()
            }),
            added_by: record.added_by,
            saved: record.saved.clone(),
            owner_hints: relation
                .map(|relation| relation.owner_origin_hints)
                .unwrap_or_default(),
        })
    }

    /// Whether the version's origin owns `subject` (§12.2), judged once per
    /// course, origin and subject.
    async fn authority(
        &self,
        llm: Option<&dyn LLMPort>,
        provenance: &ProvenanceRecord,
        subject: &ClaimSubject,
    ) -> Result<AuthorityAssessment> {
        if let Some(saved) = self
            .store
            .authority(
                &self.program_id,
                &provenance.origin.key,
                &subject.key,
                OWNERSHIP_POLICY,
                &self.model,
            )
            .await?
        {
            return Ok(saved.value);
        }
        let llm = llm.ok_or_else(|| not_judged("ownership", &provenance.origin.key.0))?;
        let request = OwnershipRequest {
            subject,
            origin: &provenance.origin,
            signals: &provenance.signals,
            course_topic: &self.topic,
        };
        let key = ownership_key(&request, &self.model);
        let judged = judge_ownership(llm, &request).await?;
        let value = judged.value.clone();
        self.store
            .save_authority(&self.program_id, &stored(key, judged))
            .await?;
        Ok(value)
    }

    /// A version's text, library facts, provenance and standing: read once
    /// per pass, the standing judged once per distinct signals (§12.1).
    async fn version(
        &mut self,
        llm: Option<&dyn LLMPort>,
        version: &str,
    ) -> Result<Arc<VersionRecord>> {
        if let Some(record) = self.versions.get(version) {
            return Ok(record.clone());
        }
        let row = sqlx::query(
            "SELECT v.source_id, v.full_text, v.acquired_at, l.added_by, l.selection_note FROM learning_source_versions v JOIN learning_source_library l ON l.id = v.source_id AND l.program_id = v.program_id WHERE v.program_id = ? AND v.id = ?",
        )
        .bind(&self.program_id)
        .bind(version)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        let added_by: String = row.try_get("added_by").map_err(db)?;
        let added_by = AddedBy::from_column(&added_by).ok_or_else(|| {
            AppError::InternalError(format!("Unknown source origin \"{added_by}\"."))
        })?;
        let selection_note: Option<String> = row.try_get("selection_note").map_err(db)?;
        let text: String = row.try_get("full_text").map_err(db)?;
        let acquired_at: i64 = row.try_get("acquired_at").map_err(db)?;
        let provenance = self.store.provenance(version).await?;
        let links = provenance
            .as_ref()
            .map_or_else(|| markdown_links(&text), |record| record.links.clone());
        let saved = SavedText { text, links };
        let chosen = Chosen {
            by: added_by,
            note: selection_note,
        };
        let judged = match provenance {
            Some(provenance) => {
                let standing = self.standing(llm, &provenance, &saved, chosen).await?;
                Some((provenance, standing))
            }
            None => None,
        };
        let record = Arc::new(VersionRecord {
            source_id: row.try_get("source_id").map_err(db)?,
            added_by,
            acquired_at: acquired_at / MILLIS_PER_SECOND,
            saved: Arc::new(saved),
            judged,
        });
        self.versions.insert(version.to_owned(), record.clone());
        Ok(record)
    }

    /// A version's standing (`source-standing-v2`): judged from its signals
    /// and the content evidence of its saved text and links, keyed by both.
    async fn standing(
        &self,
        llm: Option<&dyn LLMPort>,
        provenance: &ProvenanceRecord,
        saved: &SavedText,
        chosen: Chosen,
    ) -> Result<StandingAssessment> {
        let content = ContentEvidence::of(&saved.text, &saved.links, &provenance.origin, chosen);
        let key = standing_key(&provenance.signals, &content, &self.model);
        if let Some(saved) = self.store.standing(&key).await? {
            return Ok(saved.value);
        }
        let llm = llm.ok_or_else(|| not_judged("standing", &provenance.origin.key.0))?;
        let track_record = self.store.track_record(&provenance.origin.key).await?;
        let judged = judge_standing(llm, &provenance.signals, &content, track_record).await?;
        let value = judged.value.clone();
        self.store.save_standing(&stored(key, judged)).await?;
        Ok(value)
    }
}

/// The label each passage carries to the checker (§18.6) inside a trust
/// scope: the source's title, its registrable domain and its effective
/// date, so a passage is never read as another source's and its date is in
/// view. The level is not in it: it is relative to the claim's subject,
/// which the gate judges after the check, and it is context for the
/// checker, never its verdict. Outside a trust scope passages are unlabelled.
pub(in crate::features::learning::lessons::content_verification) async fn passage_labels(
    references: &crate::features::learning::reference_collection::ReferenceCollection<'_>,
) -> Result<HashMap<String, String>> {
    let Some(course) = course() else {
        return Ok(HashMap::new());
    };
    let records: HashMap<String, ProvenanceRecord> = SqliteSourceTrustStore::new(course.pool)
        .program_provenance(&course.program_id)
        .await?
        .into_iter()
        .map(|record| (record.source_version_id.clone(), record))
        .collect();
    Ok(references
        .sources
        .iter()
        .map(|source| {
            let record = records.get(&source.id);
            let domain = record
                .and_then(|record| record.origin.registrable_domain.clone())
                .or_else(|| {
                    source
                        .url
                        .as_deref()
                        .and_then(|url| Origin::of_url(url).ok())
                        .and_then(|origin| origin.registrable_domain)
                });
            let date = record
                .and_then(|record| record.signals.dates())
                .and_then(|dates| effective_date(dates).at)
                .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
                .map(|at| at.format("%Y-%m-%d").to_string());
            let label = std::iter::once(format!("From \"{}\"", source.title.trim()))
                .chain(domain)
                .chain(date.map(|date| format!("dated {date}")))
                .collect::<Vec<_>>()
                .join(" · ");
            (source.id.clone(), label)
        })
        .collect())
}
