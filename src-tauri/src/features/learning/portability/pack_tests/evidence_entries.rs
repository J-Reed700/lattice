//! Learner evidence travels one table per entry and canvases one per entry,
//! so heavy history no longer has to fit a single entry.
use super::{move_request, packs, preview_request, repack};
use crate::features::learning::{
    pack::{decode_learning_pack, LearningPackEntry, LearningPackEntryKind},
    pack_repository::LearningPackRepository,
    portability_dto::{ApplyLearningPackImportRequestDto, LearningPackConflictPolicy},
    repository::LearningRepository,
    tests::{fixture, pool},
};
use crate::shared::error::Result;
use sqlx::SqlitePool;

async fn course(pool: &SqlitePool) -> Result<String> {
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    Ok(program.summary.id)
}

async fn export(pool: &SqlitePool, dir: &std::path::Path, program_id: &str) -> Result<String> {
    let workspace = packs(pool, dir).export(&move_request(program_id)).await?;
    Ok(workspace.exports[0].destination_path.clone())
}

async fn merge_into(packs: &LearningPackRepository, path: &str) -> Result<()> {
    let previewed = packs
        .preview(&preview_request(
            path,
            LearningPackConflictPolicy::MergeSafe,
        ))
        .await?;
    let preview = &previewed.import_previews[0];
    assert!(preview.can_apply, "{:?}", preview.conflicts);
    packs
        .apply(&ApplyLearningPackImportRequestDto {
            operation_id: super::id(),
            preview_id: preview.id.clone(),
            expected_root_sha256: preview.manifest.root_sha256.clone(),
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_multi_megabyte_evidence_table_round_trips_in_its_own_entry() -> Result<()> {
    let home = pool().await?;
    let program_id = course(&home).await?;
    let large = serde_json::to_string(&"x".repeat(9 * 1024 * 1024))?;
    let operation_id = super::id();
    sqlx::query("INSERT INTO learning_operations(operation_id,program_id,scope,kind,subject_id,payload_hash,result_json,created_at) VALUES(?,?,'curriculum','long_history',NULL,'large-hash',?,1)")
        .bind(&operation_id).bind(&program_id).bind(&large).execute(&home).await?;
    let home_dir = tempfile::tempdir()?;
    let path = export(&home, home_dir.path(), &program_id).await?;
    let decoded = decode_learning_pack(&tokio::fs::read(&path).await?)?;
    let table = &decoded.entries["evidence/tables/learning_operations.json"];
    assert!(table.len() > 8 * 1024 * 1024);
    assert!(!decoded
        .entries
        .contains_key("evidence/learning-aggregate.json"));

    let fresh = pool().await?;
    let fresh_dir = tempfile::tempdir()?;
    merge_into(&packs(&fresh, fresh_dir.path()), &path).await?;
    let restored: String =
        sqlx::query_scalar("SELECT result_json FROM learning_operations WHERE operation_id=?")
            .bind(&operation_id)
            .fetch_one(&fresh)
            .await?;
    assert_eq!(restored, large);
    Ok(())
}

#[tokio::test]
async fn a_table_over_the_import_cap_is_refused_at_export_and_backup() -> Result<()> {
    const CAP: i64 = 50_000;
    let home = pool().await?;
    let program_id = course(&home).await?;
    let home_dir = tempfile::tempdir()?;
    let home_packs = packs(&home, home_dir.path());
    let earlier = export(&home, home_dir.path(), &program_id).await?;
    sqlx::query("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<=?) INSERT INTO learning_operations(operation_id,program_id,scope,kind,subject_id,payload_hash,result_json,created_at) SELECT 'heavy-'||i,?,'recall','review',NULL,'hash','null',i FROM n")
        .bind(CAP).bind(&program_id).execute(&home).await?;
    let names_cap = |error: &crate::shared::error::AppError| {
        let text = error.to_string();
        assert!(
            text.contains("learning_operations") && text.contains(&CAP.to_string()),
            "{text}"
        );
    };

    let refused = home_packs
        .export(&move_request(&program_id))
        .await
        .expect_err("no computer could import this pack");
    names_cap(&refused);
    assert_eq!(home_packs.workspace(&program_id).await?.exports.len(), 1);

    // Replacing the course would first back it up into a pack nobody could
    // restore, so the replacement is refused and the course stays as it was.
    let request = preview_request(&earlier, LearningPackConflictPolicy::ReplaceAfterBackup);
    let previewed = home_packs.preview(&request).await?;
    let preview = previewed
        .import_previews
        .iter()
        .find(|preview| preview.id == request.preview_id)
        .expect("the replace preview");
    assert!(preview.can_apply, "{:?}", preview.conflicts);
    let refused = home_packs
        .apply(&ApplyLearningPackImportRequestDto {
            operation_id: super::id(),
            preview_id: preview.id.clone(),
            expected_root_sha256: preview.manifest.root_sha256.clone(),
        })
        .await
        .expect_err("the backup could not be restored");
    names_cap(&refused);
    let kept: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM learning_operations WHERE program_id=? AND kind='review'",
    )
    .bind(&program_id)
    .fetch_one(&home)
    .await?;
    assert_eq!(kept, CAP + 1);
    let mut files = tokio::fs::read_dir(home_dir.path().join("learning-packs")).await?;
    let mut count = 0;
    while files.next_entry().await?.is_some() {
        count += 1;
    }
    assert_eq!(count, 1, "only the earlier pack is on disk");
    Ok(())
}

#[tokio::test]
async fn a_missing_or_extra_evidence_table_is_rejected() -> Result<()> {
    let home = pool().await?;
    let program_id = course(&home).await?;
    let home_dir = tempfile::tempdir()?;
    let path = export(&home, home_dir.path(), &program_id).await?;
    let decoded = decode_learning_pack(&tokio::fs::read(&path).await?)?;
    let tables = decoded
        .entries
        .keys()
        .filter(|path| path.starts_with("evidence/tables/"))
        .count();
    assert_eq!(tables, 32);

    let missing = repack(&decoded, |input| {
        input
            .entries
            .retain(|entry| entry.path != "evidence/tables/study_reviews.json")
    })?;
    let extra = repack(&decoded, |input| {
        input.entries.push(LearningPackEntry {
            path: "evidence/tables/learning_unknown.json".into(),
            kind: LearningPackEntryKind::Evidence,
            bytes: br#"{"table":"learning_unknown","columns":[],"rows":[]}"#.to_vec(),
        })
    })?;
    let fresh = pool().await?;
    let fresh_dir = tempfile::tempdir()?;
    let fresh_packs = packs(&fresh, fresh_dir.path());
    for (name, bytes, reason) in [
        ("missing", missing, "missing study_reviews"),
        ("extra", extra, "unexpected table"),
    ] {
        let file = fresh_dir.path().join(format!("{name}.lattice-learning"));
        tokio::fs::write(&file, bytes).await?;
        let error = fresh_packs
            .preview(&preview_request(
                &file.to_string_lossy(),
                LearningPackConflictPolicy::MergeSafe,
            ))
            .await
            .expect_err("the evidence table set is exact");
        assert!(error.to_string().contains(reason), "{name}: {error}");
    }
    assert!(LearningRepository::new(fresh.clone())
        .list()
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn two_canvases_travel_as_two_entries() -> Result<()> {
    let home = pool().await?;
    let program_id = course(&home).await?;
    for (canvas, title) in [("canvas-one", "Sketch"), ("canvas-two", "Diagram")] {
        let canvas_id = format!("{program_id}-{canvas}");
        sqlx::query("INSERT INTO learning_canvases(id,program_id,lesson_id,title,description,scene_json,element_count,revision,created_at,updated_at) VALUES(?,?,NULL,?,'A drawing','{\"elements\":[1]}',1,2,10,11)")
            .bind(&canvas_id).bind(&program_id).bind(title).execute(&home).await?;
        sqlx::query("INSERT INTO learning_canvas_snapshots(id,program_id,canvas_id,name,title,description,scene_json,element_count,canvas_revision,created_at) VALUES(?,?,?,'v1',?,'Saved','{\"elements\":[]}',0,1,12)")
            .bind(format!("{canvas_id}-snapshot")).bind(&program_id).bind(&canvas_id).bind(title)
            .execute(&home).await?;
    }
    let home_dir = tempfile::tempdir()?;
    let path = export(&home, home_dir.path(), &program_id).await?;
    let decoded = decode_learning_pack(&tokio::fs::read(&path).await?)?;
    let canvases: Vec<&str> = decoded
        .entries
        .keys()
        .filter(|path| path.starts_with("program/canvases/"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        canvases,
        [
            format!("program/canvases/{program_id}-canvas-one.json"),
            format!("program/canvases/{program_id}-canvas-two.json"),
        ]
    );
    assert!(!decoded.entries.contains_key("program/canvases.json"));

    let fresh = pool().await?;
    let fresh_dir = tempfile::tempdir()?;
    merge_into(&packs(&fresh, fresh_dir.path()), &path).await?;
    let restored: Vec<(String, String, String, i64)> = sqlx::query_as("SELECT c.id,c.title,s.id,c.revision FROM learning_canvases c JOIN learning_canvas_snapshots s ON s.canvas_id=c.id WHERE c.program_id=? ORDER BY c.id")
        .bind(&program_id).fetch_all(&fresh).await?;
    assert_eq!(
        restored,
        [
            (
                format!("{program_id}-canvas-one"),
                "Sketch".to_owned(),
                format!("{program_id}-canvas-one-snapshot"),
                2
            ),
            (
                format!("{program_id}-canvas-two"),
                "Diagram".to_owned(),
                format!("{program_id}-canvas-two-snapshot"),
                2
            ),
        ]
    );
    Ok(())
}
