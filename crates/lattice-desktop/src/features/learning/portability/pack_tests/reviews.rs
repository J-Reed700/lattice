//! Lesson claim reviews in a pack: a published lesson's review travels with
//! it, an unfinished one stays behind and says so, a copy gives every review
//! record its own identity, and a replace keeps what lives on this computer.
use super::{
    id, move_request, packs, preview_request, r#move::import, r#move::preview_for, repack,
};
use crate::features::learning::{
    pack::decode_learning_pack,
    portability_dto::{LearningPackConflictPolicy, LearningPackStaysBehindKind},
    repository::LearningRepository,
    tests::{pool, ready_program},
};
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

/// A lesson's review, its job and one claim with two events, an audit and
/// the example it labelled. Returns the job and claim IDs.
async fn reviewed_lesson(
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
    review_state: &str,
    job_status: &str,
) -> Result<(String, String)> {
    let (job_id, claim_id) = (id(), id());
    let requested = serde_json::json!({
        "programId": program_id,
        "requestJson": serde_json::json!({ "lessonIds": [lesson_id] }).to_string(),
    });
    sqlx::query("INSERT INTO jobs(id,kind,subject_id,operation_id,payload_hash,status,requested_json,progress_current,progress_total,progress_message,staged_result_json,result_ref,created_at,finished_at) VALUES(?,'learning.lesson_preparation',?,?,'payload',?,?,1,1,'Done','{\"lesson\":\"staged\"}',?,1,2)")
        .bind(&job_id)
        .bind(program_id)
        .bind(id())
        .bind(job_status)
        .bind(requested.to_string())
        .bind(format!("needs_review:{lesson_id}"))
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) VALUES(?,?,1,7)")
        .bind(&job_id)
        .bind(program_id)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO learning_lesson_reviews(lesson_id,program_id,job_id,state,revision,draft_sha256,checker_model,opened_at,updated_at) VALUES(?,?,?,?,2,'draft-sha','checker',1,2)")
        .bind(lesson_id)
        .bind(program_id)
        .bind(&job_id)
        .bind(review_state)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO learning_lesson_claims(claim_id,lesson_id,unit_index,unit_kind,field,quote,statement,state_json,evidence_json,updated_at) VALUES(?,?,0,'block','body','Water boils at 100 C.','Water boils at 100 C at sea level.',?,'[]',2)")
        .bind(&claim_id)
        .bind(lesson_id)
        .bind(serde_json::json!({ "confirmed": { "claimId": claim_id } }).to_string())
        .execute(pool)
        .await?;
    for (ordinal, actor, note) in [
        (0, "machine", "queued"),
        (1, "user", "I checked this one myself"),
    ] {
        sqlx::query("INSERT INTO learning_lesson_claim_events(claim_id,ordinal,actor,kind,payload_json,job_id,at) VALUES(?,?,?,'note',?,?,?)")
            .bind(&claim_id)
            .bind(ordinal)
            .bind(actor)
            .bind(serde_json::json!({ "note": note }).to_string())
            .bind(&job_id)
            .bind(ordinal + 1)
            .execute(pool)
            .await?;
    }
    sqlx::query("INSERT INTO learning_lesson_audits(lesson_id,draft_sha256,verifier_model,policy,state,batches_json,started_at,finished_at) VALUES(?,'draft-sha','verifier','policy','completed',?,1,2)")
        .bind(lesson_id)
        .bind(serde_json::json!([{ "claimIds": [claim_id] }]).to_string())
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO learning_checker_examples(id,program_id,lesson_id,claim_id,checker_model,context_window,policy,claim,passages_json,local_verdict,label,label_source,at) VALUES(?,?,?,?,'checker',8192,'policy','Water boils at 100 C.','[]','unsupported','true','user_confirm',3)")
        .bind(id())
        .bind(program_id)
        .bind(lesson_id)
        .bind(&claim_id)
        .execute(pool)
        .await?;
    Ok((job_id, claim_id))
}

/// A tangent chat opened from `lesson_id`.
async fn tangent(
    pool: &SqlitePool,
    program_id: &str,
    module_id: &str,
    lesson_id: &str,
) -> Result<String> {
    let conversation = id();
    sqlx::query("INSERT INTO conversations (id,title) VALUES (?,'Tangent')")
        .bind(&conversation)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO learning_tangents(conversation_id,program_id,module_id,lesson_id,section_index,section_title,selected_text,context_message_count,created_at) VALUES(?,?,?,?,1,'Boiling','boils at 100 C',3,5)")
        .bind(&conversation)
        .bind(program_id)
        .bind(module_id)
        .bind(lesson_id)
        .execute(pool)
        .await?;
    Ok(conversation)
}

async fn audit_on(pool: &SqlitePool, program_id: &str) -> Result<()> {
    sqlx::query("UPDATE learning_programs SET verifier_audit='on',verifier_web_search=1,verifier_consent_at=9,verifier_consent_provider='provider' WHERE id=?")
        .bind(program_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn count(pool: &SqlitePool, sql: &str, bind: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(sql).bind(bind).fetch_one(pool).await?)
}

/// A course with a published lesson's review, a failed lesson's unfinished
/// one, a tangent chat and the Verifier audit on. Returns the course, the
/// published lesson, and its job and claim.
async fn course(pool: &SqlitePool) -> Result<(String, String, String, String)> {
    let program = ready_program(&LearningRepository::new(pool.clone())).await?;
    let module = &program.modules[0];
    let (published, failed) = (&module.lessons[0].id, &module.lessons[1].id);
    let (job, claim) =
        reviewed_lesson(pool, &program.summary.id, published, "ready", "completed").await?;
    reviewed_lesson(pool, &program.summary.id, failed, "verifying", "failed").await?;
    tangent(pool, &program.summary.id, &module.id, published).await?;
    audit_on(pool, &program.summary.id).await?;
    Ok((program.summary.id.clone(), published.clone(), job, claim))
}

#[tokio::test]
async fn a_published_lessons_review_moves_and_an_unfinished_one_stays_behind() -> Result<()> {
    let home = pool().await?;
    let (course, lesson, job, claim) = course(&home).await?;
    let home_dir = tempfile::tempdir()?;
    let home_packs = packs(&home, home_dir.path());
    let notice = home_packs.workspace(&course).await?.export_notice;
    for kind in [
        LearningPackStaysBehindKind::UnfinishedLessonReviews,
        LearningPackStaysBehindKind::TangentChats,
        LearningPackStaysBehindKind::VerifierConsent,
    ] {
        assert!(
            notice
                .stays_behind
                .iter()
                .any(|item| item.kind == kind && item.count == 1),
            "{kind:?} in {:?}",
            notice.stays_behind
        );
    }
    let pack = home_packs.export(&move_request(&course)).await?.exports[0]
        .destination_path
        .clone();

    let fresh = pool().await?;
    let fresh_dir = tempfile::tempdir()?;
    import(
        &packs(&fresh, fresh_dir.path()),
        &pack,
        LearningPackConflictPolicy::MergeSafe,
    )
    .await?;
    let review: (String, String, String) = sqlx::query_as(
        "SELECT lesson_id,job_id,state FROM learning_lesson_reviews WHERE program_id=?",
    )
    .bind(&course)
    .fetch_one(&fresh)
    .await?;
    assert_eq!(review, (lesson.clone(), job.clone(), "ready".into()));
    let notes: Vec<String> = sqlx::query_scalar(
        "SELECT json_extract(payload_json,'$.note') FROM learning_lesson_claim_events WHERE claim_id=? ORDER BY ordinal",
    )
    .bind(&claim)
    .fetch_all(&fresh)
    .await?;
    assert_eq!(notes, ["queued", "I checked this one myself"]);
    assert_eq!(
        count(
            &fresh,
            "SELECT COUNT(*) FROM learning_lesson_audits WHERE lesson_id=?",
            &lesson
        )
        .await?,
        1
    );
    assert_eq!(
        count(
            &fresh,
            "SELECT COUNT(*) FROM learning_checker_examples WHERE program_id=?",
            &course
        )
        .await?,
        2
    );
    // The job is a record of how the lesson was checked, never work to resume.
    let job_row: (String, Option<String>, Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT j.status,j.staged_result_json,j.retry_of_job_id,l.content_revision FROM jobs j JOIN learning_jobs l ON l.job_id=j.id WHERE j.id=?",
    )
    .bind(&job)
    .fetch_one(&fresh)
    .await?;
    assert_eq!(job_row, ("completed".into(), None, None, None));
    let unfinished_jobs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status<>'completed'")
            .fetch_one(&fresh)
            .await?;
    assert_eq!(unfinished_jobs, 0);
    assert_eq!(
        count(
            &fresh,
            "SELECT COUNT(*) FROM learning_lesson_reviews WHERE state<>'ready' AND program_id=?",
            &course
        )
        .await?,
        0
    );
    assert_eq!(
        count(
            &fresh,
            "SELECT COUNT(*) FROM learning_tangents WHERE program_id=?",
            &course
        )
        .await?,
        0
    );
    assert_eq!(count(&fresh, "SELECT COUNT(*) FROM learning_programs WHERE id=? AND verifier_audit='off' AND verifier_consent_at IS NULL", &course).await?, 1);
    Ok(())
}

#[tokio::test]
async fn a_copy_gives_every_review_record_its_own_identity() -> Result<()> {
    let pool = pool().await?;
    let (course, lesson, job, claim) = course(&pool).await?;
    let dir = tempfile::tempdir()?;
    let packs = packs(&pool, dir.path());
    let pack = packs.export(&move_request(&course)).await?.exports[0]
        .destination_path
        .clone();
    let copy = import(&packs, &pack, LearningPackConflictPolicy::CreateCopy)
        .await?
        .program_id;
    assert_ne!(copy, course);
    let (copy_lesson, copy_job): (String, String) =
        sqlx::query_as("SELECT lesson_id,job_id FROM learning_lesson_reviews WHERE program_id=?")
            .bind(&copy)
            .fetch_one(&pool)
            .await?;
    assert_ne!(copy_lesson, lesson);
    assert_eq!(count_lesson(&pool, &copy, &copy_lesson).await?, 1);
    assert_ne!(copy_job, job);
    let (requested, result_ref, operation): (String, String, String) =
        sqlx::query_as("SELECT requested_json,result_ref,operation_id FROM jobs WHERE id=?")
            .bind(&copy_job)
            .fetch_one(&pool)
            .await?;
    // The lesson is named inside JSON held in a string; it follows too.
    assert!(requested.contains(&copy_lesson) && requested.contains(&copy));
    assert!(!requested.contains(&lesson) && !requested.contains(&course));
    assert_eq!(result_ref, format!("needs_review:{copy_lesson}"));
    let original_operation: String = sqlx::query_scalar("SELECT operation_id FROM jobs WHERE id=?")
        .bind(&job)
        .fetch_one(&pool)
        .await?;
    assert_ne!(operation, original_operation);
    let copy_claim: String =
        sqlx::query_scalar("SELECT claim_id FROM learning_lesson_claims WHERE lesson_id=?")
            .bind(&copy_lesson)
            .fetch_one(&pool)
            .await?;
    assert_ne!(copy_claim, claim);
    let state: String =
        sqlx::query_scalar("SELECT state_json FROM learning_lesson_claims WHERE claim_id=?")
            .bind(&copy_claim)
            .fetch_one(&pool)
            .await?;
    assert!(state.contains(&copy_claim));
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM learning_lesson_claim_events WHERE claim_id=?",
            &copy_claim
        )
        .await?,
        2
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM learning_checker_examples WHERE claim_id=?",
            &copy_claim
        )
        .await?,
        1
    );
    let batches: String =
        sqlx::query_scalar("SELECT batches_json FROM learning_lesson_audits WHERE lesson_id=?")
            .bind(&copy_lesson)
            .fetch_one(&pool)
            .await?;
    assert!(batches.contains(&copy_claim));
    // The original's records are untouched.
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM learning_lesson_claims WHERE lesson_id=?",
            &lesson
        )
        .await?,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM learning_checker_examples WHERE claim_id=?",
            &claim
        )
        .await?,
        1
    );
    Ok(())
}

async fn count_lesson(pool: &SqlitePool, program_id: &str, lesson_id: &str) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM learning_lessons WHERE program_id=? AND id=?")
            .bind(program_id)
            .bind(lesson_id)
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn a_replace_warns_about_unfinished_reviews_and_keeps_what_lives_here() -> Result<()> {
    let pool = pool().await?;
    let (course, lesson, _, _) = course(&pool).await?;
    let dir = tempfile::tempdir()?;
    let packs = packs(&pool, dir.path());
    let pack = packs.export(&move_request(&course)).await?.exports[0]
        .destination_path
        .clone();
    let request = preview_request(&pack, LearningPackConflictPolicy::ReplaceAfterBackup);
    let previewed = packs.preview(&request).await?;
    let preview = preview_for(&previewed, &request);
    assert!(preview.can_apply, "{:?}", preview.conflicts);
    assert!(
        preview
            .warnings
            .iter()
            .any(|warning| warning.starts_with("Replacing discards 1 unfinished lesson review")),
        "{:?}",
        preview.warnings
    );
    import(
        &packs,
        &pack,
        LearningPackConflictPolicy::ReplaceAfterBackup,
    )
    .await?;
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM learning_tangents WHERE program_id=?",
            &course
        )
        .await?,
        1
    );
    let tangent_lesson: String =
        sqlx::query_scalar("SELECT lesson_id FROM learning_tangents WHERE program_id=?")
            .bind(&course)
            .fetch_one(&pool)
            .await?;
    assert_eq!(tangent_lesson, lesson);
    let verifier: (String, i64, Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT verifier_audit,verifier_web_search,verifier_consent_at,verifier_consent_provider FROM learning_programs WHERE id=?",
    )
    .bind(&course)
    .fetch_one(&pool)
    .await?;
    assert_eq!(verifier, ("on".into(), 1, Some(9), Some("provider".into())));
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM learning_lesson_reviews WHERE program_id=? AND state='ready'",
            &course
        )
        .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn a_pack_whose_review_job_would_run_here_is_refused() -> Result<()> {
    let home = pool().await?;
    let (course, ..) = course(&home).await?;
    let dir = tempfile::tempdir()?;
    let home_packs = packs(&home, dir.path());
    let path = home_packs.export(&move_request(&course)).await?.exports[0]
        .destination_path
        .clone();
    let decoded = decode_learning_pack(&std::fs::read(&path)?)?;
    let bytes = repack(&decoded, |input| {
        let entry = input
            .entries
            .iter_mut()
            .find(|entry| entry.path == "curriculum/reviews/jobs.json")
            .expect("the review jobs entry");
        let text = String::from_utf8(entry.bytes.clone()).expect("JSON text");
        entry.bytes = text.replace("\"completed\"", "\"pending\"").into_bytes();
    })?;
    let tampered = dir.path().join("tampered.lattice-learning");
    std::fs::write(&tampered, bytes)?;
    let fresh = pool().await?;
    let fresh_dir = tempfile::tempdir()?;
    let refused = packs(&fresh, fresh_dir.path())
        .preview(&preview_request(
            tampered.to_str().expect("UTF-8 path"),
            LearningPackConflictPolicy::MergeSafe,
        ))
        .await
        .expect_err("a pending job is refused");
    assert!(
        refused.to_string().contains("would start running here"),
        "{refused}"
    );
    Ok(())
}
