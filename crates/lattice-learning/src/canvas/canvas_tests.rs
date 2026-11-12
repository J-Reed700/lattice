use crate::{
    canvas_repository::{validate_scene, LearningCanvasRepository},
    dto::*,
    repository::LearningRepository,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::time::Duration;

fn scene(label: &str, count: usize) -> serde_json::Value {
    serde_json::json!({
        "type":"excalidraw", "version":2,
        "elements":(0..count).map(|index| serde_json::json!({"id":format!("{label}-{index}"),"type":"rectangle"})).collect::<Vec<_>>(),
        "appState":{"viewBackgroundColor":"#ffffff"}, "files":{}
    })
}

async fn active_program(db: &Database) -> Result<LearningProgramDto> {
    let repo = LearningRepository::new(db.clone());
    let program = crate::tests::fixture();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    repo.get(&program.summary.id).await
}

async fn persistent_pool(path: std::path::PathBuf) -> Result<SqlitePool> {
    let options = crate::test_support::file_options(path);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .map_err(|error| lattice_core::shared::error::AppError::Database(error.to_string()))?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;
    Ok(pool)
}

async fn concurrent_pool(path: std::path::PathBuf) -> Result<SqlitePool> {
    let options = crate::test_support::file_options(path).busy_timeout(Duration::from_secs(15));
    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .map_err(AppError::from)
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[tokio::test]
async fn workspaces_batch_snapshots_without_mixing_canvases_or_programs() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool);
    let program = active_program(&db).await?;
    let other = active_program(&db).await?;
    let repo = LearningCanvasRepository::new(db);
    let (empty, reads) = lattice_core::shared::testing::query_counts::count_reads(
        repo.workspace(&program.summary.id),
    )
    .await;
    assert!(empty?.canvases.is_empty());
    assert_eq!(reads, 2);
    let mut expected = std::collections::HashMap::new();
    for index in 0..4 {
        let program_id = if index < 3 {
            &program.summary.id
        } else {
            &other.summary.id
        };
        let canvas_id = uuid();
        let create = CreateLearningCanvasRequestDto {
            operation_id: uuid(),
            canvas_id: canvas_id.clone(),
            program_id: program_id.clone(),
            lesson_id: None,
            title: format!("Canvas {index}"),
            description: String::new(),
            scene_json: scene(&format!("scene-{index}"), index + 1),
        };
        repo.create(&create, &validate_scene(&create.scene_json)?)
            .await?;
        let snapshot_id = (index != 1).then(uuid);
        if let Some(snapshot_id) = &snapshot_id {
            repo.create_snapshot(&CreateLearningCanvasSnapshotRequestDto {
                operation_id: uuid(),
                snapshot_id: snapshot_id.clone(),
                program_id: program_id.clone(),
                canvas_id: canvas_id.clone(),
                expected_revision: 0,
                name: format!("Snapshot {index}"),
            })
            .await?;
        }
        if index < 3 {
            expected.insert(canvas_id, (snapshot_id, create.scene_json));
        }
    }
    let (workspace, reads) = lattice_core::shared::testing::query_counts::count_reads(
        repo.workspace(&program.summary.id),
    )
    .await;
    let workspace = workspace?;
    assert_eq!(reads, 3);
    assert_eq!(workspace.canvases.len(), 3);
    for canvas in workspace.canvases {
        let (snapshot, scene) = expected.remove(&canvas.id).unwrap();
        assert_eq!(canvas.scene_json, scene);
        assert_eq!(canvas.snapshots.len(), usize::from(snapshot.is_some()));
        if let Some(id) = snapshot {
            assert_eq!(canvas.snapshots[0].id, id);
            assert_eq!(canvas.snapshots[0].canvas_id, canvas.id);
            assert_eq!(canvas.snapshots[0].scene_json, scene);
        }
    }
    Ok(())
}

#[test]
fn excalidraw_validation_checks_shape_size_and_disallows_unmanaged_files() {
    let valid = scene("a", 3);
    assert_eq!(
        validate_scene(&valid)
            .map(|checked| checked.element_count)
            .ok(),
        Some(3)
    );
    for malformed in [
        serde_json::json!([]),
        serde_json::json!({"type":"other","version":2,"elements":[],"appState":{},"files":{}}),
        serde_json::json!({"type":"excalidraw","version":2.5,"elements":[],"appState":{},"files":{}}),
        serde_json::json!({"type":"excalidraw","version":2,"elements":{},"appState":{},"files":{}}),
        serde_json::json!({"type":"excalidraw","version":2,"elements":[],"appState":[],"files":{}}),
    ] {
        assert!(validate_scene(&malformed).is_err());
    }
    let mut files = scene("files", 0);
    files["files"] = serde_json::json!({"img":{"dataURL":"data:image/png;base64,abc"}});
    assert!(validate_scene(&files).is_err());
    let mut dangling_image = scene("image", 1);
    dangling_image["elements"][0]["type"] = serde_json::json!("image");
    assert!(validate_scene(&dangling_image).is_err());
    for embedded_type in ["embeddable", "iframe"] {
        let mut embedded = scene("embedded", 1);
        embedded["elements"][0]["type"] = serde_json::json!(embedded_type);
        assert!(validate_scene(&embedded).is_err(), "{embedded_type}");
    }
    let mut missing_element_id = scene("missing-id", 1);
    missing_element_id["elements"][0]
        .as_object_mut()
        .map(|object| object.remove("id"));
    assert!(validate_scene(&missing_element_id).is_err());
    let mut missing_element_type = scene("missing-type", 1);
    missing_element_type["elements"][0]
        .as_object_mut()
        .map(|object| object.remove("type"));
    assert!(validate_scene(&missing_element_type).is_err());
    let mut deleted_elements = scene("deleted", 2);
    deleted_elements["elements"][0]["isDeleted"] = serde_json::json!(true);
    assert_eq!(
        validate_scene(&deleted_elements)
            .map(|checked| checked.element_count)
            .ok(),
        Some(1)
    );
    let mut invalid_version = scene("version", 0);
    invalid_version["version"] = serde_json::json!(0);
    assert!(validate_scene(&invalid_version).is_err());
    assert!(validate_scene(&scene("too-many", 5_001)).is_err());
    let oversized = serde_json::json!({"type":"excalidraw","version":2,"elements":[],"appState":{"large":"x".repeat(5 * 1024 * 1024)},"files":{}});
    assert!(validate_scene(&oversized).is_err());
}

#[tokio::test]
async fn canvas_revisions_operations_snapshots_and_restore_are_transactional() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;
    let program = active_program(&db).await?;
    let lesson_id = program
        .modules
        .first()
        .and_then(|m| m.lessons.first())
        .map(|l| l.id.clone())
        .ok_or_else(|| {
            lattice_core::shared::error::AppError::InvalidState("fixture lesson missing".into())
        })?;
    let repo = LearningCanvasRepository::new(db.clone());
    let canvas_id = uuid();
    let create = CreateLearningCanvasRequestDto {
        operation_id: uuid(),
        canvas_id: canvas_id.clone(),
        program_id: program.summary.id.clone(),
        lesson_id: Some(lesson_id),
        title: "Concept map".into(),
        description: "First thought".into(),
        scene_json: scene("initial", 1),
    };
    let initial_scene = validate_scene(&create.scene_json)?;
    repo.create(&create, &initial_scene).await?;
    repo.create(&create, &initial_scene).await?;
    let mut changed_create = create.clone();
    changed_create.title = "Different title".into();
    assert!(repo.create(&changed_create, &initial_scene).await.is_err());
    let duplicate_canvas = CreateLearningCanvasRequestDto {
        operation_id: uuid(),
        ..create.clone()
    };
    assert!(repo
        .create(&duplicate_canvas, &initial_scene)
        .await
        .is_err());
    let initial = repo.workspace(&program.summary.id).await?;
    assert_eq!(initial.canvases.len(), 1);
    assert_eq!(initial.canvases[0].element_count, 1);
    assert_eq!(initial.canvases[0].revision, 0);

    let save = SaveLearningCanvasRequestDto {
        operation_id: uuid(),
        program_id: program.summary.id.clone(),
        canvas_id: canvas_id.clone(),
        expected_revision: 0,
        title: "Concept map".into(),
        description: "Expanded".into(),
        scene_json: scene("expanded", 2),
    };
    let expanded = validate_scene(&save.scene_json)?;
    repo.save(&save, &expanded).await?;
    repo.save(&save, &expanded).await?;
    let mut changed_save = save.clone();
    changed_save.description = "Different payload".into();
    assert!(repo.save(&changed_save, &expanded).await.is_err());
    let stale = SaveLearningCanvasRequestDto {
        operation_id: uuid(),
        ..save.clone()
    };
    crate::tests::assert_conflict(repo.save(&stale, &expanded).await);

    let snapshot = CreateLearningCanvasSnapshotRequestDto {
        operation_id: uuid(),
        snapshot_id: uuid(),
        program_id: program.summary.id.clone(),
        canvas_id: canvas_id.clone(),
        expected_revision: 1,
        name: "Expanded checkpoint".into(),
    };
    repo.create_snapshot(&snapshot).await?;
    repo.create_snapshot(&snapshot).await?;
    let mut changed_snapshot = snapshot.clone();
    changed_snapshot.name = "Changed checkpoint".into();
    assert!(repo.create_snapshot(&changed_snapshot).await.is_err());
    let duplicate_snapshot = CreateLearningCanvasSnapshotRequestDto {
        operation_id: uuid(),
        ..snapshot.clone()
    };
    assert!(repo.create_snapshot(&duplicate_snapshot).await.is_err());
    let later = SaveLearningCanvasRequestDto {
        operation_id: uuid(),
        program_id: program.summary.id.clone(),
        canvas_id: canvas_id.clone(),
        expected_revision: 1,
        title: "Later".into(),
        description: "Later edit".into(),
        scene_json: scene("later", 1),
    };
    let later_scene = validate_scene(&later.scene_json)?;
    repo.save(&later, &later_scene).await?;

    let restore = RestoreLearningCanvasSnapshotRequestDto {
        operation_id: uuid(),
        pre_restore_snapshot_id: uuid(),
        program_id: program.summary.id.clone(),
        canvas_id: canvas_id.clone(),
        snapshot_id: snapshot.snapshot_id.clone(),
        expected_revision: 2,
    };
    repo.restore(&restore).await?;
    repo.restore(&restore).await?;
    let mut changed_restore = restore.clone();
    changed_restore.expected_revision = 3;
    assert!(repo.restore(&changed_restore).await.is_err());
    crate::tests::assert_conflict(
        repo.restore(&RestoreLearningCanvasSnapshotRequestDto {
            operation_id: uuid(),
            pre_restore_snapshot_id: uuid(),
            expected_revision: 0,
            ..restore.clone()
        })
        .await,
    );
    crate::tests::assert_conflict(
        repo.create_snapshot(&CreateLearningCanvasSnapshotRequestDto {
            operation_id: uuid(),
            snapshot_id: uuid(),
            expected_revision: 0,
            ..snapshot.clone()
        })
        .await,
    );
    let restored = repo.workspace(&program.summary.id).await?;
    let canvas = restored
        .canvases
        .first()
        .ok_or_else(|| lattice_core::shared::error::AppError::NotFound("canvas".into()))?;
    assert_eq!(canvas.revision, 3);
    assert_eq!(canvas.element_count, 2);
    assert_eq!(canvas.scene_json, scene("expanded", 2));
    assert_eq!(canvas.snapshots.len(), 2);
    let checkpoint = canvas
        .snapshots
        .iter()
        .find(|s| s.id == snapshot.snapshot_id)
        .ok_or_else(|| lattice_core::shared::error::AppError::NotFound("checkpoint".into()))?;
    assert_eq!(checkpoint.scene_json, scene("expanded", 2));
    assert!(canvas
        .snapshots
        .iter()
        .any(|s| s.id == restore.pre_restore_snapshot_id && s.title == "Later"));

    let mut other_program = crate::tests::fixture();
    let other_repo = LearningRepository::new(db.clone());
    other_repo.create(&other_program).await?;
    other_repo
        .accept(&AcceptLearningProgramRequestDto {
            program_id: other_program.summary.id.clone(),
            expected_revision: 0,
            title: other_program.summary.title.clone(),
        })
        .await?;
    other_program = other_repo.get(&other_program.summary.id).await?;
    assert!(repo
        .workspace(&other_program.summary.id)
        .await?
        .canvases
        .is_empty());
    let wrong_owner = SaveLearningCanvasRequestDto {
        program_id: other_program.summary.id.clone(),
        operation_id: uuid(),
        ..save.clone()
    };
    assert!(repo.save(&wrong_owner, &expanded).await.is_err());

    let learning_repo = LearningRepository::new(db.clone());
    learning_repo.delete(&program.summary.id).await?;
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_canvases WHERE program_id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(remaining, 0);
    Ok(())
}

#[tokio::test]
async fn canvas_scene_and_snapshot_survive_a_database_restart() -> Result<()> {
    let directory = tempfile::tempdir()
        .map_err(|e| lattice_core::shared::error::AppError::Database(e.to_string()))?;
    let path = directory.path().join("canvas.db");
    let first = persistent_pool(path.clone()).await?;
    let first_db = Database::new(first.clone());
    let program = active_program(&first_db).await?;
    let canvas_id = uuid();
    let request = CreateLearningCanvasRequestDto {
        operation_id: uuid(),
        canvas_id: canvas_id.clone(),
        program_id: program.summary.id.clone(),
        lesson_id: None,
        title: "Persistent canvas".into(),
        description: "Restart check".into(),
        scene_json: scene("saved", 4),
    };
    let repo = LearningCanvasRepository::new(first_db.clone());
    repo.create(&request, &validate_scene(&request.scene_json)?)
        .await?;
    let snapshot = CreateLearningCanvasSnapshotRequestDto {
        operation_id: uuid(),
        snapshot_id: uuid(),
        program_id: program.summary.id.clone(),
        canvas_id: canvas_id.clone(),
        expected_revision: 0,
        name: "Saved state".into(),
    };
    repo.create_snapshot(&snapshot).await?;
    first.close().await;
    let second = persistent_pool(path).await?;
    let second_db = Database::new(second.clone());
    let reopened = LearningCanvasRepository::new(second_db.clone())
        .workspace(&program.summary.id)
        .await?;
    assert_eq!(
        reopened.canvases.first().map(|canvas| canvas.element_count),
        Some(4)
    );
    assert_eq!(
        reopened
            .canvases
            .first()
            .map(|canvas| canvas.snapshots.len()),
        Some(1)
    );
    second.close().await;
    Ok(())
}

#[tokio::test]
async fn concurrent_retry_of_identical_save_commits_one_revision_and_receipt() -> Result<()> {
    let directory = tempfile::tempdir()
        .map_err(|e| lattice_core::shared::error::AppError::Database(e.to_string()))?;
    let path = directory.path().join("canvas-concurrent.db");
    let setup_pool = persistent_pool(path.clone()).await?;
    let setup_db = Database::new(setup_pool.clone());
    let program = active_program(&setup_db).await?;
    let canvas_id = uuid();
    let create = CreateLearningCanvasRequestDto {
        operation_id: uuid(),
        canvas_id: canvas_id.clone(),
        program_id: program.summary.id.clone(),
        lesson_id: None,
        title: "Concurrent canvas".into(),
        description: String::new(),
        scene_json: scene("before", 1),
    };
    LearningCanvasRepository::new(setup_db.clone())
        .create(&create, &validate_scene(&create.scene_json)?)
        .await?;
    setup_pool.close().await;

    let pool = concurrent_pool(path).await?;
    let db = Database::new(pool.clone());
    let repository = LearningCanvasRepository::new(db.clone());
    let save = SaveLearningCanvasRequestDto {
        operation_id: uuid(),
        program_id: program.summary.id.clone(),
        canvas_id: canvas_id.clone(),
        expected_revision: 0,
        title: "Concurrent canvas".into(),
        description: "One write, two deliveries".into(),
        scene_json: scene("after", 2),
    };
    let scene = validate_scene(&save.scene_json)?;
    let (first, second) = tokio::join!(
        repository.save(&save, &scene),
        repository.save(&save, &scene),
    );
    first?;
    second?;

    let workspace = repository.workspace(&program.summary.id).await?;
    let canvas = workspace
        .canvases
        .first()
        .ok_or_else(|| lattice_core::shared::error::AppError::NotFound("canvas".into()))?;
    assert_eq!(canvas.revision, 1);
    assert_eq!(canvas.element_count, 2);
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_operations WHERE operation_id=? AND scope='canvas' AND kind='save'",
    )
    .bind(&save.operation_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(receipts, 1);
    pool.close().await;
    Ok(())
}
