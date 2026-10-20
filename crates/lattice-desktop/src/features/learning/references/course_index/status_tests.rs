#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! The status read: ready, indexing (queued and running), no model and
//! unavailable, and that it queues a build for a course whose index is behind
//! while never opening or building the index itself.

use super::tests::{builds, course, indexes, pages};
use super::*;
use crate::features::learning::tests;
use crate::infrastructure::passage_index::testing::{FakeEmbedder, FixedEmbedders};
use lattice_core::shared::runtime::jobs::{JobStatus, JobStore};
use lattice_core::shared::Result;
use std::sync::Arc;

#[tokio::test]
async fn a_current_index_is_ready_and_queues_nothing() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(FakeEmbedder::new("model-a"))),
    );
    let (program, _) = course(&pool, &pages(2)).await?;
    courses.ensure_current_quietly(&program).await?;

    assert_eq!(
        courses.status(&program).await,
        CourseIndexStatusDto::Ready {
            mode: "hybrid".into()
        }
    );
    assert!(builds(&pool, &program).await.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_course_behind_its_index_queues_one_build_and_never_opens_the_index() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let model = FakeEmbedder::new("model-a");
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(Arc::clone(&model))),
    );
    let (program, _) = course(&pool, &pages(3)).await?;

    let first = courses.status(&program).await;
    assert_eq!(
        first,
        CourseIndexStatusDto::Indexing {
            embedded: 0,
            total: 0,
            references_new: 3,
            passages_per_second: None
        }
    );
    // Read only: no directory made, nothing chunked or embedded.
    assert!(!courses.dir_of(&program)?.exists());
    assert!(model.embedded().is_empty());
    let queued = builds(&pool, &program).await;
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].status, JobStatus::Pending);

    // Asked again while the build waits: the same build, still indexing.
    assert!(matches!(
        courses.status(&program).await,
        CourseIndexStatusDto::Indexing { .. }
    ));
    assert_eq!(builds(&pool, &program).await.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_running_build_reports_its_own_progress_and_pace() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(FakeEmbedder::new("model-a"))),
    );
    let (program, _) = course(&pool, &pages(3)).await?;
    courses.status(&program).await;
    let build = builds(&pool, &program).await.remove(0);
    let store = JobStore::new(pool.clone());
    store.claim(&build.id).await?;
    store
        .progress(
            &build.id,
            40,
            "Embedding passages: 12 of 30",
            Some(&serde_json::json!({
                "phase": "embedding",
                "references": 3,
                "embedded": 12,
                "total": 30,
                "perSecond": 6.0,
            })),
        )
        .await?;

    assert_eq!(
        courses.status(&program).await,
        CourseIndexStatusDto::Indexing {
            embedded: 12,
            total: 30,
            references_new: 3,
            passages_per_second: Some(6.0)
        }
    );
    Ok(())
}

#[tokio::test]
async fn without_an_embedding_model_the_course_is_searched_by_words_and_nothing_is_queued(
) -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(&pool, temp.path(), FixedEmbedders::new(None));
    let (program, _) = course(&pool, &pages(2)).await?;

    assert_eq!(
        courses.status(&program).await,
        CourseIndexStatusDto::NoEmbeddingModel
    );
    assert!(builds(&pool, &program).await.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_unreadable_index_or_a_bad_id_is_unavailable_with_its_reason() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(FakeEmbedder::new("model-a"))),
    );
    assert!(matches!(
        courses.status("not-a-program").await,
        CourseIndexStatusDto::Unavailable { reason } if reason.contains("Invalid program ID")
    ));

    let (program, _) = course(&pool, &pages(2)).await?;
    let dir = courses.dir_of(&program)?;
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(crate::infrastructure::passage_index::store::DB_FILE),
        "not a database",
    )
    .unwrap();
    assert!(matches!(
        courses.status(&program).await,
        CourseIndexStatusDto::Unavailable { .. }
    ));
    assert!(builds(&pool, &program).await.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_build_that_could_not_finish_is_reported_and_not_queued_again() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(FakeEmbedder::new("model-a"))),
    );
    let (program, _) = course(&pool, &pages(2)).await?;
    courses.status(&program).await;
    let build = builds(&pool, &program).await.remove(0);
    let store = JobStore::new(pool.clone());
    store.claim(&build.id).await?;
    store
        .complete(
            &build.id,
            &program,
            "0 of 2 reference passages are embedded; the rest are searched by their words only",
        )
        .await?;

    let status = courses.status(&program).await;
    assert!(
        matches!(&status, CourseIndexStatusDto::Unavailable { reason } if reason.contains("0 of 2")),
        "{status:?}"
    );
    let after = builds(&pool, &program).await;
    assert_eq!(after.len(), 1);
    assert!(after[0].status.is_finished(), "nothing new is queued");
    Ok(())
}
