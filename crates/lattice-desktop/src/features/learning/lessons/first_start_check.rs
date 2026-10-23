//! ONE-OFF, NOT FOR COMMIT. What the owner's install goes through the first
//! time this branch starts on his data, measured on a COPY of `lattice.db`
//! named by `LATTICE_FIRST_START_DB` (copy it with
//! `scratchpad/first-start/copy_data.sh`). The copy is migrated and then only
//! read, the way the app opens it; nothing here makes a model call.
//!
//! a. the migrations, timed, and whether `20261022120000_source_selection_note`
//!    applied;
//! b. every lesson with a claim ledger loaded and replayed through
//!    `ClaimReviewRepository::load` (what `get_learning_lesson_claim_review`
//!    runs), its new tally beside the old code's stored job tally;
//! c. the review DTO the UI fetches (`ClaimReviewService::review`), serialized
//!    as the IPC layer does;
//! d. sources whose `source-standing-v1` judgement exists but whose
//!    `source-standing-v2` one does not: the standing calls the model owes;
//! e. one summary table.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout
)]

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use futures::FutureExt;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::application::services::source_trust::{standing_key, STANDING_POLICY};
use lattice_core::domain::learning::claim_review::{
    ClaimCounts, ClaimLedgerEntry, ClaimState, VerdictKind,
};
use lattice_core::domain::provenance::ProvenanceRecord;
use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use super::claim_reviews::dto::LessonClaimReviewDto;
use super::claim_reviews::repository::ClaimReviewRepository;
use super::claim_reviews::service::ClaimReviewService;
use crate::features::learning::curriculum::{LearningClaimTally, LearningGenerationActivity};
use crate::features::learning::references::trust::judgements::content_evidence;
use crate::features::learning::references::trust::repository::SqliteSourceTrustStore;
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};

const NEW_MIGRATION: i64 = 20261022120000;
const V1_POLICY: &str = "source-standing-v1";

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
        })
        .unwrap_or_else(|| "non-text panic payload".into())
}

fn short(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

async fn applied_migrations(pool: &SqlitePool) -> Result<BTreeSet<i64>> {
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await
    .map_err(db)?;
    if table == 0 {
        return Ok(BTreeSet::new());
    }
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success=1")
            .fetch_all(pool)
            .await
            .map_err(db)?;
    Ok(versions.into_iter().collect())
}

async fn count(pool: &SqlitePool, sql: &str, binds: &[&str]) -> Result<i64> {
    let mut query = sqlx::query_scalar::<_, i64>(sql);
    for bind in binds {
        query = query.bind(*bind);
    }
    query.fetch_one(pool).await.map_err(db)
}

/// The tally the next verification pass writes into the job's activity: a
/// copy of `content_verification::claim_ledger::claim_tally` (private to that
/// module), over the new code's `ClaimCounts` and claim states.
fn job_tally(entries: &[ClaimLedgerEntry]) -> LearningClaimTally {
    let counts = ClaimCounts::tally(entries.iter());
    let count = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
    let (mut unchecked, mut searching, mut sent_back) = (0, 0, 0);
    for entry in entries {
        match entry.state() {
            ClaimState::Unchecked => unchecked += 1,
            ClaimState::Open {
                kind: VerdictKind::InsufficientStanding { .. },
                ..
            } if entry.may_research() => sent_back += 1,
            ClaimState::Open { .. } if entry.may_research() => searching += 1,
            _ => {}
        }
    }
    LearningClaimTally {
        unchecked: count(unchecked),
        searching: count(searching),
        sent_back: count(sent_back),
        repairing: count(
            counts
                .open
                .saturating_sub(unchecked + searching + sent_back),
        ),
        waiting: count(counts.queued),
        verified: count(counts.verified),
        resolved_by_you: count(counts.resolved_by_you()),
        rewritten: count(counts.rewritten),
    }
}

struct LessonLedger {
    lesson_id: String,
    program_id: Option<String>,
    job_id: Option<String>,
    stored_claims: i64,
}

/// One row of the summary table.
#[derive(Default)]
struct LessonResult {
    lesson_id: String,
    claims: usize,
    load: String,
    load_time: Duration,
    counts: Option<ClaimCounts>,
    old: Option<Value>,
    review: String,
    items: usize,
    with_sentence: usize,
    in_paragraph: usize,
    panicked: bool,
    failed: bool,
}

/// What the review DTOs name as supports, for (d).
#[derive(Default)]
struct Supports {
    all: BTreeSet<(String, String)>,
    waiting: BTreeSet<(String, String)>,
}

fn state_name(state: &impl serde::Serialize) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|value| {
            value
                .get("state")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "?".into())
}

fn report_review(dto: &LessonClaimReviewDto, result: &mut LessonResult, supports: &mut Supports) {
    result.items = dto.claims.len();
    let mut states: BTreeMap<String, usize> = BTreeMap::new();
    // source title · kind → (all items, waiting items)
    let mut groups: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for item in &dto.claims {
        let state = state_name(&item.state);
        let waiting = matches!(state.as_str(), "queued" | "reopened");
        *states.entry(state).or_default() += 1;
        if !item.presentation.sentence.trim().is_empty() {
            result.with_sentence += 1;
        }
        if !item.presentation.context_before.trim().is_empty()
            || !item.presentation.context_after.trim().is_empty()
        {
            result.in_paragraph += 1;
        }
        let group = match &item.support {
            Some(support) => format!("{} · {:?}", support.source.title, support.kind),
            None => "(no support)".into(),
        };
        let slot = groups.entry(group).or_default();
        slot.0 += 1;
        if waiting {
            slot.1 += 1;
        }
        for support in item.support.iter().chain(item.also_supported.iter()) {
            let key = (
                dto.program_id.clone(),
                support.source.source_version_id.clone(),
            );
            if waiting {
                supports.waiting.insert(key.clone());
            }
            supports.all.insert(key);
        }
    }
    println!(
        "  review DTO: state {:?} · revision {} · pass {} · changed since stop {} · stop reason {}",
        dto.state,
        dto.revision,
        dto.pass,
        dto.changed_since_stop,
        if dto.stop_reason.is_some() {
            "set"
        } else {
            "none"
        },
    );
    println!("  review DTO counts: {:?}", dto.counts);
    println!(
        "  items {} · presentation with a sentence {} · placed in its paragraph {} · teaching findings {}",
        result.items,
        result.with_sentence,
        result.in_paragraph,
        dto.teaching_findings.len()
    );
    println!("  items by state: {states:?}");
    let mut ordered: Vec<_> = groups.into_iter().collect();
    ordered.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    println!("  items grouped by source (all / waiting for you):");
    for (group, (all, waiting)) in ordered {
        println!("    {all:>4} / {waiting:>4}  {group}");
    }
}

#[tokio::test]
#[ignore = "requires LATTICE_FIRST_START_DB, a COPY of the app's lattice.db; migrates and reads the copy, makes no model call"]
async fn first_start_on_a_copy_of_the_owners_data() -> Result<()> {
    let path = PathBuf::from(
        std::env::var("LATTICE_FIRST_START_DB")
            .expect("LATTICE_FIRST_START_DB names a copy of lattice.db"),
    );
    let shown = path.display().to_string();
    assert!(
        !shown.contains("Application Support/tech.lattice.app"),
        "LATTICE_FIRST_START_DB must be a copy, never the live data folder: {shown}"
    );
    assert!(path.is_file(), "no database file at {shown}");

    // a. Open and migrate, as `setup_database` does at app start.
    println!("=== a. migrations on {shown}");
    let opened = Instant::now();
    let connection = DatabaseConnection::new(path.clone()).await?;
    let open_time = opened.elapsed();
    if let Some(moved) = connection.recovered_from() {
        panic!(
            "the copy failed the integrity check and was moved aside to {}; nothing below would describe the owner's data. Copy it again with the app closed.",
            moved.display()
        );
    }
    let pool = connection.pool().clone();
    let before = applied_migrations(&pool).await?;
    let migrating = Instant::now();
    let migrated = initialize_database(&pool).await;
    let migrate_time = migrating.elapsed();
    if let Err(error) = &migrated {
        println!("MIGRATIONS FAILED after {migrate_time:.1?}: {error}");
    }
    migrated?;
    let after = applied_migrations(&pool).await?;
    let newly: Vec<i64> = after.difference(&before).copied().collect();
    let new_applied = newly.contains(&NEW_MIGRATION);
    let column = count(
        &pool,
        "SELECT COUNT(*) FROM pragma_table_info('learning_source_library') WHERE name='selection_note'",
        &[],
    )
    .await?;
    let library_rows = count(&pool, "SELECT COUNT(*) FROM learning_source_library", &[]).await?;
    let null_notes = count(
        &pool,
        "SELECT COUNT(*) FROM learning_source_library WHERE selection_note IS NULL",
        &[],
    )
    .await?;
    println!(
        "  open {open_time:.1?} · migrations {migrate_time:.1?} · applied before {} · newly applied {newly:?}",
        before.len()
    );
    println!(
        "  {NEW_MIGRATION}_source_selection_note: {} · selection_note column present: {} · library rows {library_rows} (NULL note {null_notes})",
        if new_applied {
            "APPLIED NOW"
        } else if after.contains(&NEW_MIGRATION) {
            "already applied before"
        } else {
            "NOT APPLIED"
        },
        column == 1
    );

    // b. + c. Every lesson with a claim ledger.
    let rows = sqlx::query(
        "SELECT r.lesson_id AS lesson_id, r.program_id AS program_id, r.job_id AS job_id, \
                (SELECT COUNT(*) FROM learning_lesson_claims c WHERE c.lesson_id=r.lesson_id) AS claims \
         FROM learning_lesson_reviews r \
         UNION ALL \
         SELECT c.lesson_id, NULL, NULL, COUNT(*) FROM learning_lesson_claims c \
         WHERE c.lesson_id NOT IN (SELECT lesson_id FROM learning_lesson_reviews) \
         GROUP BY c.lesson_id \
         ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .map_err(db)?;
    let lessons: Vec<LessonLedger> = rows
        .iter()
        .map(|row| LessonLedger {
            lesson_id: row.get("lesson_id"),
            program_id: row.get("program_id"),
            job_id: row.get("job_id"),
            stored_claims: row.get("claims"),
        })
        .collect();
    println!("\n=== b./c. {} lessons with a claim ledger", lessons.len());

    let store = ClaimReviewRepository::new(pool.clone());
    let service = ClaimReviewService::new(pool.clone());
    let mut results: Vec<LessonResult> = Vec::new();
    let mut supports = Supports::default();
    for lesson in &lessons {
        let mut result = LessonResult {
            lesson_id: lesson.lesson_id.clone(),
            ..LessonResult::default()
        };
        println!(
            "\nlesson {} · program {} · job {} · {} claim rows",
            lesson.lesson_id,
            lesson.program_id.as_deref().unwrap_or("(no review row)"),
            lesson.job_id.as_deref().unwrap_or("-"),
            lesson.stored_claims
        );

        // b. Load and replay, as the review command does.
        let loading = Instant::now();
        let loaded = AssertUnwindSafe(store.load(&lesson.lesson_id))
            .catch_unwind()
            .await;
        result.load_time = loading.elapsed();
        let ledger = match loaded {
            Ok(Ok(ledger)) => {
                result.load = "loaded".into();
                Some(ledger)
            }
            Ok(Err(error)) => {
                result.load = "FAILED".into();
                result.failed = true;
                println!("  load FAILED: {error}");
                None
            }
            Err(payload) => {
                result.load = "PANIC".into();
                result.panicked = true;
                println!("  load PANICKED: {}", panic_text(payload.as_ref()));
                None
            }
        };
        if let Some(ledger) = &ledger {
            result.claims = ledger.claims.len();
            let counts = ClaimCounts::tally(ledger.claims.iter());
            println!(
                "  loaded {} claims in {:.1?} · review {} · revision {}",
                ledger.claims.len(),
                result.load_time,
                ledger
                    .review
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |review| format!("{:?}", review.state)),
                ledger.revision.0
            );
            println!(
                "  new tally: verified {} · waiting for you {} · removed_by_you {} · edited_by_you {} · rewritten {} · open/queued-for-machine {} · confirmed {} · general knowledge {} (resolved by you {})",
                counts.verified,
                counts.queued,
                counts.removed_by_you,
                counts.edited_by_you,
                counts.rewritten,
                counts.open,
                counts.confirmed,
                counts.general_knowledge,
                counts.resolved_by_you()
            );
            println!(
                "  new job tally (what the next pass writes): {:?}",
                job_tally(&ledger.claims)
            );
            result.counts = Some(counts);
        }

        // The old code's tally, as the job's activity stored it.
        let job_id = lesson.job_id.clone().or_else(|| {
            ledger
                .as_ref()
                .and_then(|ledger| ledger.review.as_ref())
                .map(|review| review.job_id.clone())
        });
        if let Some(job_id) = job_id {
            let activity: Option<Option<String>> =
                sqlx::query_scalar("SELECT activity_json FROM jobs WHERE id=?")
                    .bind(&job_id)
                    .fetch_optional(&pool)
                    .await
                    .map_err(db)?;
            match activity.flatten() {
                None => println!("  stored job tally (old code): no job row or no activity"),
                Some(raw) => {
                    let value: Value = serde_json::from_str(&raw)
                        .map_err(|error| AppError::Serialization(error.to_string()))?;
                    let claims = value.get("claims").cloned().unwrap_or(Value::Null);
                    println!("  stored job tally (old code): {claims}");
                    match serde_json::from_value::<LearningGenerationActivity>(value) {
                        Ok(_) => println!("  stored activity decodes under the new code: yes"),
                        Err(error) => {
                            result.failed = true;
                            println!("  stored activity decodes under the new code: NO ({error})");
                        }
                    }
                    if !claims.is_null() {
                        result.old = Some(claims);
                    }
                }
            }
        }

        // c. The review the UI fetches, then serialized for IPC.
        let program_id = lesson.program_id.clone().or_else(|| {
            ledger
                .as_ref()
                .and_then(|ledger| ledger.review.as_ref())
                .map(|review| review.program_id.clone())
        });
        match program_id {
            None => result.review = "no review".into(),
            Some(program_id) => {
                let fetched = AssertUnwindSafe(service.review(&program_id, &lesson.lesson_id))
                    .catch_unwind()
                    .await;
                match fetched {
                    Ok(Ok(Some(dto))) => {
                        let encoded =
                            std::panic::catch_unwind(AssertUnwindSafe(|| serde_json::to_vec(&dto)));
                        match encoded {
                            Ok(Ok(bytes)) => {
                                result.review = "ok".into();
                                println!("  review DTO serialized: {} bytes", bytes.len());
                            }
                            Ok(Err(error)) => {
                                result.review = "SERIALIZE FAILED".into();
                                result.failed = true;
                                println!("  review DTO serialize FAILED: {error}");
                            }
                            Err(payload) => {
                                result.review = "SERIALIZE PANIC".into();
                                result.panicked = true;
                                println!(
                                    "  review DTO serialize PANICKED: {}",
                                    panic_text(payload.as_ref())
                                );
                            }
                        }
                        report_review(&dto, &mut result, &mut supports);
                    }
                    Ok(Ok(None)) => {
                        result.review = "none".into();
                        println!("  review: none (no pass yet, or another program)");
                    }
                    Ok(Err(error)) => {
                        result.review = "FAILED".into();
                        result.failed = true;
                        println!("  review FAILED: {error}");
                    }
                    Err(payload) => {
                        result.review = "PANIC".into();
                        result.panicked = true;
                        println!("  review PANICKED: {}", panic_text(payload.as_ref()));
                    }
                }
            }
        }
        results.push(result);
    }

    // d. v1 standing without v2 standing. No model call: keys only.
    println!("\n=== d. source standing, v1 vs v2");
    let by_policy = sqlx::query(
        "SELECT policy, model, COUNT(*) AS n FROM source_standing_assessments GROUP BY policy, model ORDER BY policy, model",
    )
    .fetch_all(&pool)
    .await
    .map_err(db)?;
    for row in &by_policy {
        let (policy, model, n): (String, String, i64) =
            (row.get("policy"), row.get("model"), row.get("n"));
        println!("  stored judgements: {policy} · {model} · {n}");
    }
    let trust = SqliteSourceTrustStore::new(pool.clone());
    let programs: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT program_id FROM source_provenance ORDER BY 1")
            .fetch_all(&pool)
            .await
            .map_err(db)?;
    let mut records: Vec<(ProvenanceRecord, bool)> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for program in &programs {
        for record in trust.program_provenance(program).await? {
            seen.insert((program.clone(), record.source_version_id.clone()));
            records.push((record, true));
        }
    }
    // Supports the held lessons name that are not a library entry's active
    // version still get weighed.
    for (program, version) in &supports.all {
        if seen.contains(&(program.clone(), version.clone())) {
            continue;
        }
        if let Some(mut record) = trust.provenance(version).await? {
            record.program_id = program.clone();
            records.push((record, false));
        }
    }
    let (mut rows_total, mut with_v1, mut with_v2, mut owed, mut neither) = (0, 0, 0, 0, 0);
    let (mut not_keyable, mut key_errors, mut stored_hash_differs) = (0, 0, 0);
    let mut owed_keys: BTreeSet<String> = BTreeSet::new();
    let mut owed_all: BTreeSet<String> = BTreeSet::new();
    let mut owed_waiting: BTreeSet<String> = BTreeSet::new();
    for (record, active) in &records {
        rows_total += 1;
        let v1_hash = record.signals.signals_sha256();
        if v1_hash != record.signals_sha256 {
            stored_hash_differs += 1;
        }
        let v1 = count(
            &pool,
            "SELECT COUNT(*) FROM source_standing_assessments WHERE signals_sha256 IN (?, ?) AND policy=?",
            &[&v1_hash, &record.signals_sha256, V1_POLICY],
        )
        .await?
            > 0;
        let content = match content_evidence(&pool, &record.program_id, record).await {
            Ok(content) => content,
            Err(error) => {
                key_errors += 1;
                println!(
                    "  {} in {}: v2 key FAILED: {error}",
                    short(&record.source_version_id),
                    short(&record.program_id)
                );
                continue;
            }
        };
        let Some(content) = content else {
            not_keyable += 1;
            if v1 {
                println!(
                    "  {} in {} ({}): v1 judged, no chosen version to key v2 by",
                    short(&record.source_version_id),
                    short(&record.program_id),
                    if *active { "active" } else { "older version" }
                );
            }
            continue;
        };
        let key = standing_key(&record.signals, &content, "");
        let v2 = count(
            &pool,
            "SELECT COUNT(*) FROM source_standing_assessments WHERE signals_sha256=? AND policy=?",
            &[&key.input_sha256, STANDING_POLICY],
        )
        .await?
            > 0;
        with_v1 += usize::from(v1);
        with_v2 += usize::from(v2);
        neither += usize::from(!v1 && !v2);
        if v1 && !v2 {
            owed += 1;
            owed_keys.insert(key.input_sha256.clone());
            let id = (record.program_id.clone(), record.source_version_id.clone());
            if supports.all.contains(&id) {
                owed_all.insert(key.input_sha256.clone());
            }
            if supports.waiting.contains(&id) {
                owed_waiting.insert(key.input_sha256.clone());
            }
        }
    }
    println!(
        "  provenance rows {rows_total} ({} active versions in {} programs) · v1 {with_v1} · v2 {with_v2} · neither {neither} · not keyable {not_keyable} · key errors {key_errors} · stored signals hash differs from recomputed {stored_hash_differs}",
        records.iter().filter(|(_, active)| *active).count(),
        programs.len()
    );
    println!(
        "  v1 without v2: {owed} rows = {} distinct v2 judgements owed; {} of them on versions the held lessons' cards cite, {} on waiting claims' supports",
        owed_keys.len(),
        owed_all.len(),
        owed_waiting.len()
    );

    // e. Summary.
    println!("\n=== e. first start summary");
    println!(
        "open {open_time:.1?} · migrations {migrate_time:.1?} · newly applied {} · selection_note migration {} · library rows {library_rows} (NULL note {null_notes})",
        newly.len(),
        if new_applied { "applied" } else { "NOT applied now" }
    );
    println!(
        "| lesson   | claims | load   | load ms | verified | waiting | removed by you | edited by you | rewritten | open | old tally (verified/waiting/resolvedByYou/rewritten) | review | items | w/ sentence | in paragraph |"
    );
    println!(
        "|----------|--------|--------|---------|----------|---------|----------------|---------------|-----------|------|-------------------------------------------------------|--------|-------|-------------|--------------|"
    );
    for result in &results {
        let counts = result.counts.unwrap_or_default();
        let old = result.old.as_ref().map_or_else(
            || "-".to_owned(),
            |old| {
                let field = |name: &str| {
                    old.get(name)
                        .map_or_else(|| "absent".to_owned(), Value::to_string)
                };
                format!(
                    "{}/{}/{}/{}",
                    field("verified"),
                    field("waiting"),
                    field("resolvedByYou"),
                    field("rewritten")
                )
            },
        );
        println!(
            "| {:<8} | {:>6} | {:<6} | {:>7} | {:>8} | {:>7} | {:>14} | {:>13} | {:>9} | {:>4} | {:<53} | {:<6} | {:>5} | {:>11} | {:>12} |",
            short(&result.lesson_id),
            result.claims,
            result.load,
            result.load_time.as_millis(),
            counts.verified,
            counts.queued,
            counts.removed_by_you,
            counts.edited_by_you,
            counts.rewritten,
            counts.open,
            old,
            result.review,
            result.items,
            result.with_sentence,
            result.in_paragraph
        );
    }
    println!(
        "standing: v1 without v2 {owed} rows → {} model judgements owed ({} cited by held lessons, {} on waiting claims) · not keyable {not_keyable} · key errors {key_errors}",
        owed_keys.len(),
        owed_all.len(),
        owed_waiting.len()
    );
    let panics = results.iter().filter(|result| result.panicked).count();
    let failures = results.iter().filter(|result| result.failed).count();
    println!(
        "lessons {} · panics {panics} · failures {failures}",
        results.len()
    );
    assert_eq!(panics, 0, "a lesson's load or review panicked; see above");
    assert_eq!(
        failures, 0,
        "a lesson's load, review or stored activity failed; see above"
    );
    Ok(())
}
