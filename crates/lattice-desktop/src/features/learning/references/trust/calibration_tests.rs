//! The user's verdicts move the bars, and a moved bar or an override
//! re-decides the claims it touches with no model call (source provenance
//! §13, §18.7). No test here has a model at all: the re-decision reads only
//! what is stored.

use super::calibration::{bars_of, view};
use super::outcomes::{self, VerdictScope};
use super::overrides;
use super::test_support::{add_page, article, course, id, page_signals, Course};
use crate::features::learning::dto::{
    GetLearningSourceTrustRequestDto, LearningTrustLevel, LearningTrustOverrideInputDto,
};
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::features::learning::lessons::claim_reviews::trust_changed::{
    affected, forget_verdict, override_changed, record_verdict, record_verifier_agreements,
};
use crate::features::learning::lessons::claim_reviews::trust_overrides;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::application::ports::source_trust::{SourceTrustStore, StoredJudgement};
use lattice_core::application::services::source_trust::standing_key;
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation,
    ClaimState, EventProvenance, EvidenceFingerprint, EvidenceSeen, GateVerdict, HumanBinding,
    LessonReview, LessonReviewState, ReopenCause, SupportedBy, TrustAssessment, TrustCause,
    TrustVerdict, UnitKind, UserAction,
};
use lattice_core::domain::provenance::{
    decide, AddedBy, Available, ClaimKind, ClaimProfile, ClusterId, EffectiveBars, LabelSource,
    OriginKey, Outcome, OutcomeRecord, PassageKind, SourceDates, StandingAssessment, StaticBars,
    Support, TemporalScope, TemporalValidity, Trust, TrustLevel, TrustOverride, POLICY,
};
use lattice_core::shared::error::Result;
use serde_json::json;
use sqlx::SqlitePool;

const MODEL: &str = "qwen3.5-27b";
const QUOTE: &str = "A conforming parser rejects a trailing comma.";

fn profile() -> ClaimProfile {
    ClaimProfile {
        kind: ClaimKind::Specification,
        // No subject: the supports are levelled from their standing and the
        // overrides alone, so no ownership judgement is needed.
        subject: None,
        temporal: TemporalScope::Current,
        measurement_origin: None,
        attributed_to: None,
        reasons: vec![],
        policy: "claim-profile-v1".into(),
        model: MODEL.into(),
    }
}

fn support(version: &str, level: TrustLevel) -> Support {
    Support {
        source_version_id: version.into(),
        origin_cluster: ClusterId(version.into()),
        trust: Trust {
            level,
            reasons: vec![],
            override_: None,
        },
        dates: SourceDates::default(),
        passage: PassageKind::Direct,
        temporal: TemporalValidity::Valid,
        added_by: AddedBy::Research,
        cited_urls: vec![],
        owner_hints: vec![],
    }
}

/// An established source alone for a specification, under the static
/// bars: below the bar.
fn below_the_bar(version: &str) -> TrustAssessment {
    let profile = profile();
    let supports = vec![support(version, TrustLevel::Established)];
    TrustAssessment {
        decision: decide(
            &profile,
            &supports,
            &[],
            &EffectiveBars::unmoved(&StaticBars::table(), &POLICY),
            &Available::default(),
        ),
        profile,
        supports,
        contrary: vec![],
        supported: SupportedBy {
            source_version_id: version.into(),
            supporting_quote: QUOTE.into(),
        },
        policy: POLICY.version.into(),
        available: None,
    }
}

/// A held verdict on an established specification source, as stored.
fn held(program_id: &str, at: i64) -> OutcomeRecord {
    OutcomeRecord {
        id: id(),
        program_id: program_id.into(),
        lesson_id: "earlier-lesson".into(),
        claim_id: id(),
        source_version_id: "earlier-version".into(),
        origin_key: OriginKey("web:example.org".into()),
        outcome: Outcome {
            level: TrustLevel::Established,
            kind: ClaimKind::Specification,
            decision_was_sufficient: false,
            right: false,
            at,
        },
        label_source: LabelSource::UserConfirm,
        policy: POLICY.version.into(),
    }
}

/// How many straight held verdicts relax the established specification
/// bar: derived from the policy, not assumed.
fn verdicts_to_relax(program_id: &str) -> usize {
    let mut records = Vec::new();
    loop {
        records.push(held(program_id, records.len() as i64));
        if bars_of(&records).lowest_accepted(ClaimKind::Specification)
            == Some(TrustLevel::Established)
        {
            return records.len();
        }
    }
}

fn provenance(actor: Actor) -> EventProvenance {
    EventProvenance {
        actor,
        at: 1_760_000_000_000,
        job_id: None,
        app_version: "1.0.0".into(),
        checker_model: Some(MODEL.into()),
    }
}

/// A lesson of the course with a review waiting for the user.
async fn waiting_lesson(course: &Course) -> Result<String> {
    let lesson_id: String = sqlx::query_scalar(
        "SELECT id FROM learning_lessons WHERE program_id = ? ORDER BY ordinal LIMIT 1",
    )
    .bind(&course.program_id)
    .fetch_one(&course.pool)
    .await
    .unwrap();
    let job_id = id();
    for (statement, binds) in [
        (
            "INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_total, progress_message, created_at) VALUES (?, 'lesson_preparation', ?, ?, 'hash', 'pending', '{}', 1, 'Queued', 1000)",
            vec![job_id.clone(), lesson_id.clone(), id()],
        ),
        (
            "INSERT INTO learning_jobs (job_id, program_id) VALUES (?, ?)",
            vec![job_id.clone(), course.program_id.clone()],
        ),
    ] {
        let mut query = sqlx::query(statement);
        for bind in binds {
            query = query.bind(bind);
        }
        query.execute(&course.pool).await.unwrap();
    }
    ClaimReviewRepository::new(course.pool.clone())
        .set_lesson_review(
            &lesson_id,
            LessonReview {
                program_id: course.program_id.clone(),
                job_id,
                state: LessonReviewState::NeedsReview,
                draft_sha256: "draft".into(),
                checker_model: MODEL.into(),
                stop_reason: None,
            },
        )
        .await?;
    Ok(lesson_id)
}

async fn append(
    pool: &SqlitePool,
    lesson_id: &str,
    claim: &ClaimId,
    actor: Actor,
    event: ClaimEvent,
) {
    let store = ClaimReviewRepository::new(pool.clone());
    let revision = store.load(lesson_id).await.unwrap().revision;
    store
        .append(lesson_id, claim, provenance(actor), event, revision)
        .await
        .unwrap();
}

/// A claim the gate held below its bar, handed to the user, as the loop and
/// the review card record it.
async fn queued_below_the_bar(
    pool: &SqlitePool,
    lesson_id: &str,
    trust: TrustAssessment,
) -> ClaimId {
    let claim = ClaimId(id());
    let GateVerdict::Fails(kind) = trust.verdict() else {
        panic!("expected a support below its bar");
    };
    let version = trust.supported.source_version_id.clone();
    let evidence = vec![EvidenceSeen {
        source_version_id: version,
        start_byte: 0,
        end_byte: QUOTE.len(),
        text_sha256: lattice_core::domain::learning::claim_review::text_sha256(QUOTE),
    }];
    let events = [
        (
            Actor::Machine,
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(0, UnitKind::Block, "body", QUOTE, QUOTE),
                statement: QUOTE.into(),
            },
        ),
        (
            Actor::Machine,
            ClaimEvent::Checked(Check {
                checker: Checker::Local {
                    model: MODEL.into(),
                },
                result: CheckResult::Failed { kind },
                reason: "Its support comes from 1 independent origin.".into(),
                fingerprint: EvidenceFingerprint::of("lesson-evidence-v21", MODEL, &evidence),
                evidence,
                window_tokens: 32_768,
            }),
        ),
        (
            Actor::Machine,
            ClaimEvent::StandingAssessed {
                reason: "Its support comes from 1 independent origin.".into(),
                trust: Box::new(trust),
                cause: None,
            },
        ),
        (
            Actor::User,
            ClaimEvent::UserAction {
                action: UserAction::ResearchWithHint {
                    hint: "the specification".into(),
                },
                note: None,
            },
        ),
    ];
    for (actor, event) in events {
        append(pool, lesson_id, &claim, actor, event).await;
    }
    claim
}

async fn claim_entry(pool: &SqlitePool, lesson_id: &str, claim: &ClaimId) -> ClaimLedgerEntry {
    ClaimReviewRepository::new(pool.clone())
        .load(lesson_id)
        .await
        .unwrap()
        .claims
        .into_iter()
        .find(|entry| entry.claim_id() == claim)
        .unwrap()
}

#[tokio::test]
async fn a_verdict_is_an_outcome_per_cluster_and_undo_forgets_it() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let claim = queued_below_the_bar(&course.pool, &lesson_id, below_the_bar("v-blog")).await;
    let before = claim_entry(&course.pool, &lesson_id, &claim).await;
    let scope = VerdictScope {
        program_id: &course.program_id,
        lesson_id: &lesson_id,
    };
    let moved = outcomes::record(&course.pool, scope, &before, TrustVerdict::Confirm).await?;
    assert!(moved.is_empty(), "one verdict does not move a bar");
    let stored = course.store.outcomes().await?;
    assert_eq!(stored.len(), 1);
    let row = &stored[0];
    assert_eq!(row.claim_id, claim.0);
    assert_eq!(row.source_version_id, "v-blog");
    assert_eq!(
        row.origin_key,
        OriginKey("version:v-blog".into()),
        "no provenance row"
    );
    assert_eq!(row.label_source, LabelSource::UserConfirm);
    assert_eq!(row.outcome.level, TrustLevel::Established);
    assert!(!row.outcome.decision_was_sufficient && !row.outcome.right);

    outcomes::forget(&course.pool, &claim.0, TrustVerdict::Confirm).await?;
    assert!(course.store.outcomes().await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_verdict_that_relaxes_a_bar_verifies_the_claims_it_held_back() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let waiting = queued_below_the_bar(&course.pool, &lesson_id, below_the_bar("v-blog")).await;
    let confirmed = queued_below_the_bar(&course.pool, &lesson_id, below_the_bar("v-other")).await;
    // Every verdict but the last that relaxes the bar, from earlier lessons.
    for at in 0..verdicts_to_relax(&course.program_id) - 1 {
        course
            .store
            .record_outcome(&held(&course.program_id, at as i64))
            .await?;
    }
    assert_eq!(
        affected(
            &course.pool,
            &TrustCause::Bar {
                kind: ClaimKind::Specification
            }
        )
        .await?
        .len(),
        2
    );

    // The user confirms one claim: its outcome is the one that relaxes the
    // bar, and the other claim is verified without a model.
    let before = claim_entry(&course.pool, &lesson_id, &confirmed).await;
    append(
        &course.pool,
        &lesson_id,
        &confirmed,
        Actor::User,
        ClaimEvent::UserAction {
            action: UserAction::Confirm {
                bound: HumanBinding::new(QUOTE, vec![]),
            },
            note: None,
        },
    )
    .await;
    let scope = VerdictScope {
        program_id: &course.program_id,
        lesson_id: &lesson_id,
    };
    let redecided = record_verdict(&course.pool, scope, &before, TrustVerdict::Confirm).await?;
    assert_eq!(redecided.verified, 1, "{redecided:?}");
    assert_eq!(redecided.lessons, vec![lesson_id.clone()]);
    let verified = claim_entry(&course.pool, &lesson_id, &waiting).await;
    assert_eq!(
        verified.state(),
        &ClaimState::ResolvedByEvidence {
            source_version_id: "v-blog".into(),
            supporting_quote: QUOTE.into(),
        }
    );
    let last = &verified.events().last().unwrap().event;
    assert!(matches!(
        last,
        ClaimEvent::StandingAssessed {
            cause: Some(TrustCause::Bar {
                kind: ClaimKind::Specification
            }),
            ..
        }
    ));
    assert!(matches!(
        claim_entry(&course.pool, &lesson_id, &confirmed)
            .await
            .state(),
        ClaimState::Confirmed { .. }
    ));
    let calibration = view(&course.pool).await?;
    let change = calibration.changes.last().unwrap();
    assert!(change.moves_bar() && change.accepted_alone);
    assert_eq!(change.claim_id, confirmed.0);

    // Undo puts the bar back, and the claim it verified waits again.
    let undone = forget_verdict(&course.pool, &confirmed.0, TrustVerdict::Confirm).await?;
    assert_eq!(undone.reopened, 1, "{undone:?}");
    let reopened = claim_entry(&course.pool, &lesson_id, &waiting).await;
    assert!(matches!(
        reopened.state(),
        ClaimState::Reopened {
            cause: ReopenCause::TrustChanged(TrustCause::Bar { .. }),
            ..
        }
    ));
    Ok(())
}

fn institutional(signals_sha256: &str) -> StandingAssessment {
    serde_json::from_value(json!({
        "standing": "institutional",
        "publisher": { "name": "Example Standards Body" },
        "publisher_kind": "government",
        "editorial": "editorial",
        "nature": "documentation",
        "flags": [],
        "reasons": [{ "code": "publisher_metadata", "text": "JSON-LD publisher" }],
        "policy": "test-policy-v1",
        "model": MODEL,
        "signals_sha256": signals_sha256,
    }))
    .expect("standing assessment")
}

#[tokio::test]
async fn an_override_re_levels_from_stored_judgements_and_never_guesses() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let judged_url = "https://spec.example.org/json";
    let judged = add_page(
        &course,
        judged_url,
        &article("judged", 8),
        page_signals(judged_url, json!({})),
    )
    .await?;
    let record = course.store.provenance(&judged.version_id).await?.unwrap();
    let content =
        super::judgements::content_evidence(course.store.pool(), &course.program_id, &record)
            .await?
            .unwrap();
    let key = standing_key(&record.signals, &content, MODEL);
    course
        .store
        .save_standing(&StoredJudgement {
            value: institutional(&key.input_sha256),
            key,
            input_tokens: 0,
            output_tokens: 0,
            assessed_at: 0,
        })
        .await?;
    let unjudged_url = "https://blog.example.net/json";
    let unjudged = add_page(
        &course,
        unjudged_url,
        &article("unjudged", 8),
        page_signals(unjudged_url, json!({})),
    )
    .await?;
    let lifted =
        queued_below_the_bar(&course.pool, &lesson_id, below_the_bar(&judged.version_id)).await;
    let left = queued_below_the_bar(
        &course.pool,
        &lesson_id,
        below_the_bar(&unjudged.version_id),
    )
    .await;

    for source_id in [&judged.source_id, &unjudged.source_id] {
        let change = overrides::set(
            &course.store,
            &TrustOverride::Source {
                program_id: course.program_id.clone(),
                source_id: source_id.clone(),
                level: TrustLevel::Primary,
                note: None,
                set_at: 0,
            },
        )
        .await?;
        let redecided = override_changed(&course.pool, &change).await?;
        if source_id == &judged.source_id {
            assert_eq!(redecided.verified, 1, "{redecided:?}");
        } else {
            assert_eq!(redecided.not_levelled, 1, "{redecided:?}");
            assert_eq!(redecided.verified, 0);
        }
    }
    let verified = claim_entry(&course.pool, &lesson_id, &lifted).await;
    assert!(matches!(
        verified.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    let support = &verified.trust().unwrap().supports[0];
    assert_eq!(support.trust.level, TrustLevel::Primary);
    assert!(support.trust.override_.is_some());
    // No standing was ever judged for the other page: only a model could
    // level it, so it is left as it was.
    assert!(matches!(
        claim_entry(&course.pool, &lesson_id, &left).await.state(),
        ClaimState::Queued { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn the_menu_counts_the_claims_an_override_re_decides_and_applying_it_re_decides_them(
) -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let url = "https://spec.example.org/json";
    let chosen = add_page(
        &course,
        url,
        &article("chosen", 8),
        page_signals(url, json!({})),
    )
    .await?;
    let record = course.store.provenance(&chosen.version_id).await?.unwrap();
    let content =
        super::judgements::content_evidence(course.store.pool(), &course.program_id, &record)
            .await?
            .unwrap();
    let key = standing_key(&record.signals, &content, MODEL);
    course
        .store
        .save_standing(&StoredJudgement {
            value: institutional(&key.input_sha256),
            key,
            input_tokens: 0,
            output_tokens: 0,
            assessed_at: 0,
        })
        .await?;
    let other_url = "https://elsewhere.example.net/json";
    let other = add_page(
        &course,
        other_url,
        &article("other", 8),
        page_signals(other_url, json!({})),
    )
    .await?;
    for version in [&chosen.version_id, &chosen.version_id, &other.version_id] {
        queued_below_the_bar(&course.pool, &lesson_id, below_the_bar(version)).await;
    }

    // Before the user commits: the claims the ledger would re-decide.
    let request = GetLearningSourceTrustRequestDto {
        program_id: course.program_id.clone(),
        source_ids: Vec::new(),
        claim: None,
        include_impact: true,
    };
    let view = trust_overrides::view(&course.store, &request).await?;
    let entry = view
        .sources
        .iter()
        .find(|entry| entry.source_id == chosen.source_id)
        .expect("listed");
    let impact = entry.impact.expect("asked for");
    assert_eq!(
        (
            impact.source.versions,
            impact.source.claims,
            impact.source.lessons
        ),
        (1, 2, 1)
    );
    assert_eq!(impact.site.expect("a web page has a site").claims, 2);

    // Applying it re-decides those claims, with no model.
    let applied = trust_overrides::set(
        &course.store,
        &LearningTrustOverrideInputDto::Source {
            program_id: course.program_id.clone(),
            source_id: chosen.source_id.clone(),
            level: LearningTrustLevel::Primary,
            note: None,
        },
    )
    .await?;
    assert_eq!(applied.impact.claims, 2);
    assert_eq!(
        (applied.redecided.verified, applied.redecided.lessons),
        (2, 1)
    );
    Ok(())
}

/// The evidence a check of `version` compared: the quotation itself.
fn quoted(version: &str) -> Vec<EvidenceSeen> {
    vec![EvidenceSeen {
        source_version_id: version.into(),
        start_byte: 0,
        end_byte: QUOTE.len(),
        text_sha256: lattice_core::domain::learning::claim_review::text_sha256(QUOTE),
    }]
}

fn supported_by(checker: Checker, version: &str) -> ClaimEvent {
    let evidence = quoted(version);
    ClaimEvent::Checked(Check {
        checker,
        result: CheckResult::Supported {
            source_version_id: version.into(),
            supporting_quote: QUOTE.into(),
        },
        reason: "The passage states it.".into(),
        fingerprint: EvidenceFingerprint::of("lesson-evidence-v21", MODEL, &evidence),
        evidence,
        window_tokens: 32_768,
    })
}

#[tokio::test]
async fn the_verifier_agreeing_with_an_accepted_claim_is_a_held_outcome() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let url = "https://spec.example.org/json";
    let saved = add_page(
        &course,
        url,
        &article("spec", 8),
        page_signals(url, json!({})),
    )
    .await?;
    // A primary source alone meets a specification's bar.
    let mut trust = below_the_bar(&saved.version_id);
    trust.supports = vec![support(&saved.version_id, TrustLevel::Primary)];
    trust.decision = decide(
        &trust.profile,
        &trust.supports,
        &[],
        &EffectiveBars::unmoved(&StaticBars::table(), &POLICY),
        &Available::default(),
    );
    let claim = ClaimId(id());
    let local = Checker::Local {
        model: MODEL.into(),
    };
    for event in [
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(0, UnitKind::Block, "body", QUOTE, QUOTE),
            statement: QUOTE.into(),
        },
        supported_by(local, &saved.version_id),
        ClaimEvent::StandingAssessed {
            reason: "Its support meets the bar for its kind.".into(),
            trust: Box::new(trust),
            cause: None,
        },
    ] {
        append(&course.pool, &lesson_id, &claim, Actor::Machine, event).await;
    }
    let before = ClaimReviewRepository::new(course.pool.clone())
        .load(&lesson_id)
        .await?;
    let verifier = Checker::Verifier {
        model: MODEL.into(),
    };
    append(
        &course.pool,
        &lesson_id,
        &claim,
        Actor::Machine,
        supported_by(verifier, &saved.version_id),
    )
    .await;

    let scope = VerdictScope {
        program_id: &course.program_id,
        lesson_id: &lesson_id,
    };
    record_verifier_agreements(&course.pool, scope, &before).await?;
    let outcomes = course.store.outcomes().await?;
    assert_eq!(outcomes.len(), 1, "{outcomes:?}");
    assert_eq!(outcomes[0].claim_id, claim.0);
    assert_eq!(outcomes[0].label_source, LabelSource::VerifierAgree);
    assert!(outcomes[0].outcome.decision_was_sufficient && outcomes[0].outcome.right);

    // Read again with nothing new appended, it records nothing more.
    let now = ClaimReviewRepository::new(course.pool.clone())
        .load(&lesson_id)
        .await?;
    record_verifier_agreements(&course.pool, scope, &now).await?;
    assert_eq!(course.store.outcomes().await?.len(), 1);
    Ok(())
}
