#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! The course index against the shared fake embedding model: a build, a
//! build cut short and resumed, a new model, a deleted course, a replaced
//! version, and a course searched without any model.

use super::*;
use crate::features::learning::dto::{AcceptLearningProgramRequestDto, LearningSourceDto};
use crate::features::learning::portability_dto::{
    DeleteLearningSourceRequestDto, ReimportLearningSourceRequestDto,
};
use crate::features::learning::reference_collection::ReferenceCollection;
use crate::features::learning::repository::LearningRepository;
use crate::features::learning::source_library::{
    CapturedLearningSource, LearningSourceLibraryRepository,
};
use crate::features::learning::sources::InitialReference;
use crate::features::learning::tests;
use crate::features::search::trait_def::HybridSearchUseCaseFactory;
use crate::infrastructure::passage_index::testing::{FakeEmbedder, FixedEmbedders};
use crate::infrastructure::passage_index::vectors::vectors_path;
use crate::infrastructure::passage_index::PassageStore;
use lattice_core::shared::runtime::background::BackgroundTasks;
use lattice_core::shared::runtime::jobs::JobRuntime;
use lattice_core::shared::Result;
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Every course's index under `base`, embedding with whatever `embedders`
/// holds, searched through the library's own orchestrator.
pub(in crate::features::learning) fn indexes(
    pool: &SqlitePool,
    base: &Path,
    embedders: Arc<FixedEmbedders>,
) -> Arc<CourseIndexes> {
    CourseIndexes::new(
        base.to_path_buf(),
        pool.clone(),
        embedders,
        Arc::new(HybridSearchUseCaseFactory),
        JobRuntime::new(pool.clone()),
        Arc::new(BackgroundTasks::default()),
    )
}

/// A course whose sources are `texts`, captured as web pages; its program id
/// and the source ids, in order.
pub(super) async fn course(pool: &SqlitePool, texts: &[String]) -> Result<(String, Vec<String>)> {
    let mut program = tests::fixture();
    program.sources = texts
        .iter()
        .enumerate()
        .map(|(i, text)| LearningSourceDto {
            id: uuid::Uuid::new_v4().to_string(),
            title: format!("Page {i}"),
            url: Some(format!("https://example.org/{i}")),
            excerpt: text.clone(),
            acquired_at: 1,
        })
        .collect();
    let references: Vec<_> = program
        .sources
        .iter()
        .map(|source| InitialReference {
            source_id: source.id.clone(),
            origin: source.url.clone().unwrap(),
            captured: CapturedLearningSource {
                title: source.title.clone(),
                publisher: None,
                requested_url: source.url.clone(),
                resolved_url: source.url.clone(),
                text: source.excerpt.clone(),
                truncated: false,
                extraction_version: "fixture_full_text".into(),
                signals: crate::features::learning::references::trust::capture::not_extracted(
                    "test fixture",
                ),
            },
        })
        .collect();
    let repo = LearningRepository::new(pool.clone());
    repo.create_with_references(&program, &references).await?;
    // Accepted, so its sources can be deleted and reimported.
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    let sources = program.sources.iter().map(|s| s.id.clone()).collect();
    Ok((program.summary.id, sources))
}

/// One short page per topic: each is one passage.
pub(super) fn pages(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("Topic {i} explains how widget{i} handles retries and backoff."))
        .collect()
}

async fn active(pool: &SqlitePool, program: &str) -> Vec<LearningSourceDto> {
    LearningSourceLibraryRepository::new(pool.clone())
        .verification_sources(program)
        .await
        .unwrap()
}

async fn store_of(courses: &CourseIndexes, program: &str) -> PassageStore {
    PassageStore::open(&courses.dir_of(program).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_build_indexes_and_embeds_every_active_version_under_its_title() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let model = FakeEmbedder::new("model-a");
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(Arc::clone(&model))),
    );
    let (program, _) = course(&pool, &pages(3)).await?;

    assert_eq!(
        courses.ensure_current_quietly(&program).await?,
        IndexMode::Hybrid
    );
    let sources = active(&pool, &program).await;
    let store = store_of(&courses, &program).await;
    let documents = store.documents("").await?;
    assert_eq!(documents.len(), 3);
    assert!(sources
        .iter()
        .all(|source| documents.contains_key(&source.id)));
    let counts = store.counts("").await?;
    assert_eq!(counts.passages_embedded, counts.passages_total);
    // Embedded under the page's title; stored without it.
    let embedded = model.embedded();
    assert_eq!(embedded.len(), counts.passages_total as usize);
    assert!(embedded.iter().all(|text| text.starts_with("Page ")));
    assert!(vectors_path(&courses.dir_of(&program)?, "model-a").exists());

    // Nothing changed: nothing is embedded again.
    model.forget_calls();
    assert_eq!(
        courses.ensure_current_quietly(&program).await?,
        IndexMode::Hybrid
    );
    assert!(model.embedded().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_build_cut_short_resumes_where_it_stopped() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let model = FakeEmbedder::new("model-a");
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(Arc::clone(&model))),
    );
    // One passage per page, so the first batch finishes whole pages.
    let total = crate::infrastructure::passage_index::build::BATCH_SIZE + 4;
    let (program, _) = course(&pool, &pages(total)).await?;

    let cut = CancellationToken::new();
    *model.cancel_after.lock() = Some((1, cut.clone()));
    let mode = courses
        .ensure_current(&program, &cut, &mut sync::Silent)
        .await?;
    let batch = crate::infrastructure::passage_index::build::BATCH_SIZE as u32;
    assert_eq!(
        mode,
        IndexMode::Partial {
            embedded: batch,
            total: total as u32
        }
    );

    *model.cancel_after.lock() = None;
    model.forget_calls();
    assert_eq!(
        courses.ensure_current_quietly(&program).await?,
        IndexMode::Hybrid
    );
    // Only the pages the cut left are embedded on resuming.
    assert_eq!(model.embedded().len(), total - batch as usize);
    Ok(())
}

#[tokio::test]
async fn a_failing_model_leaves_the_rest_to_words_and_the_next_run_finishes() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let model = FakeEmbedder::new("model-a");
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(Arc::clone(&model))),
    );
    let total = crate::infrastructure::passage_index::build::BATCH_SIZE + 4;
    let (program, _) = course(&pool, &pages(total)).await?;

    model.fail_after.store(1, Ordering::SeqCst);
    let mode = courses.ensure_current_quietly(&program).await?;
    assert!(
        matches!(mode, IndexMode::Partial { embedded, total: all } if embedded < all),
        "{mode:?}"
    );
    assert_eq!(mode.label(), "partial");

    model.fail_after.store(usize::MAX, Ordering::SeqCst);
    assert_eq!(
        courses.ensure_current_quietly(&program).await?,
        IndexMode::Hybrid
    );
    Ok(())
}

#[tokio::test]
async fn a_new_embedding_model_embeds_every_passage_again_and_keeps_the_passages() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let first = FakeEmbedder::new("model-a");
    let embedders = FixedEmbedders::new(Some(Arc::clone(&first)));
    let courses = indexes(&pool, temp.path(), Arc::clone(&embedders));
    let (program, _) = course(&pool, &pages(3)).await?;
    courses.ensure_current_quietly(&program).await?;
    let before = store_of(&courses, &program).await.counts("").await?;

    let second = FakeEmbedder::new("model-b");
    embedders.set(Some(Arc::clone(&second)));
    let course_search = courses.search(&program, CancellationToken::new()).await?;
    assert_eq!(
        course_search.ensure_current(&mut sync::Silent).await?,
        IndexMode::Hybrid
    );
    assert_eq!(second.embedded().len(), before.passages_total as usize);
    let dir = courses.dir_of(&program)?;
    assert!(vectors_path(&dir, "model-b").exists());
    assert!(!vectors_path(&dir, "model-a").exists());
    assert_eq!(course_search.view().identity.as_deref(), Some("model-b"));
    let after = course_search.view().store.counts("").await?;
    assert_eq!(after.passages_total, before.passages_total);
    Ok(())
}

#[tokio::test]
async fn deleting_a_course_removes_its_index_and_the_sweep_removes_orphans() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(FakeEmbedder::new("model-a"))),
    );
    let (kept, _) = course(&pool, &pages(2)).await?;
    let (deleted, _) = course(&pool, &pages(2)).await?;
    courses.ensure_current_quietly(&kept).await?;
    courses.ensure_current_quietly(&deleted).await?;
    assert!(courses.dir_of(&deleted)?.exists());

    courses.delete(&deleted).await;
    assert!(!courses.dir_of(&deleted)?.exists());

    // A course deleted while the app was not running.
    let orphan = courses.dir_of(&uuid::Uuid::new_v4().to_string())?;
    std::fs::create_dir_all(&orphan).unwrap();
    let unrelated = temp.path().join("not-a-course");
    std::fs::create_dir_all(&unrelated).unwrap();
    courses.sweep().await?;
    assert!(!orphan.exists());
    assert!(courses.dir_of(&kept)?.exists());
    assert!(
        unrelated.exists(),
        "only directories named by a program id are swept"
    );
    Ok(())
}

#[tokio::test]
async fn a_replaced_version_leaves_the_index_and_its_replacement_is_searched() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(FakeEmbedder::new("model-a"))),
    );
    let (program, source_ids) = course(&pool, &pages(2)).await?;
    courses.ensure_current_quietly(&program).await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    // The version the replaced source has now; sources created together list
    // in id order, so it is looked up by its source.
    let old_version = library
        .workspace(&program)
        .await?
        .sources
        .into_iter()
        .find(|source| source.id == source_ids[0])
        .and_then(|source| source.active_version_id)
        .unwrap();
    library
        .delete_source(&DeleteLearningSourceRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.clone(),
            source_id: source_ids[0].clone(),
            expected_revision: 0,
            reason: "Replaced".into(),
        })
        .await?;
    let new_version = uuid::Uuid::new_v4().to_string();
    library
        .reimport_source(&ReimportLearningSourceRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.clone(),
            source_id: source_ids[0].clone(),
            version_id: new_version.clone(),
            expected_revision: 1,
            replacement_text: Some("Quasar emissions were measured again.".into()),
        })
        .await?;

    let course_search = courses.search(&program, CancellationToken::new()).await?;
    let sources = active(&pool, &program).await;
    let references = ReferenceCollection::current(Some(&course_search), &sources).await?;
    let store = course_search.view().store;
    let documents = store.documents("").await?;
    assert!(!documents.contains_key(&old_version));
    assert!(documents.contains_key(&new_version));
    let hits = references.retrieve("quasar emissions", 1).await?;
    assert_eq!(hits[0].source_id, new_version);
    assert!(references
        .retrieve("widget0 retries", 4)
        .await?
        .iter()
        .all(|hit| hit.source_id != old_version));
    Ok(())
}

#[tokio::test]
async fn without_an_embedding_model_the_course_is_searched_by_words_and_says_so() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(&pool, temp.path(), FixedEmbedders::new(None));
    let (program, _) = course(&pool, &pages(3)).await?;
    let course_search = courses.search(&program, CancellationToken::new()).await?;
    let sources = active(&pool, &program).await;

    let references = ReferenceCollection::current(Some(&course_search), &sources).await?;
    assert_eq!(references.mode(), IndexMode::Lexical);
    assert_eq!(references.mode().label(), "lexical");
    assert_eq!(references.embedding_model(), None);
    assert!(course_search.view().hybrid.is_none());
    let hits = references.retrieve("widget1 retries", 2).await?;
    assert!(hits[0].text.contains("widget1"));
    assert!(hits.iter().all(|hit| hit.retrieval_kind == "lexical"));
    // The passages are in the index, ready to embed once a model is active.
    let counts = course_search.view().store.counts("").await?;
    assert_eq!(counts.passages_total, 3);
    assert_eq!(counts.passages_embedded, 0);
    Ok(())
}

/// The builds queued for `program`.
pub(super) async fn builds(
    pool: &SqlitePool,
    program: &str,
) -> Vec<lattice_core::shared::runtime::jobs::JobRecord> {
    lattice_core::shared::runtime::jobs::JobStore::new(pool.clone())
        .list_for_subject(COURSE_INDEX, program)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_command_on_an_unindexed_course_searches_at_once_and_queues_the_embedding() -> Result<()>
{
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let model = FakeEmbedder::new("model-a");
    // No build handler is registered, so the queued build stays queued.
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(Arc::clone(&model))),
    );
    let (program, _) = course(&pool, &pages(3)).await?;
    let course_search = courses.search(&program, CancellationToken::new()).await?;
    let sources = active(&pool, &program).await;

    let references = ReferenceCollection::searchable_now(Some(&course_search), &sources).await?;
    assert_eq!(
        references.mode(),
        IndexMode::Partial {
            embedded: 0,
            total: 3
        }
    );
    assert!(model.embedded().is_empty(), "a command never embeds");
    // The new versions are chunked in, so they are found by their words.
    let hits = references.retrieve("widget2 retries", 1).await?;
    assert!(hits[0].text.contains("widget2"));
    assert_eq!(hits[0].retrieval_kind, "partial");
    let queued = builds(&pool, &program).await;
    assert_eq!(queued.len(), 1);
    assert!(!queued[0].status.is_finished());
    Ok(())
}

#[tokio::test]
async fn startup_queues_builds_for_stale_courses_and_none_for_current_ones() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(
        &pool,
        temp.path(),
        FixedEmbedders::new(Some(FakeEmbedder::new("model-a"))),
    );
    let (current, _) = course(&pool, &pages(2)).await?;
    let (never_indexed, _) = course(&pool, &pages(2)).await?;
    let (changed, changed_sources) = course(&pool, &pages(2)).await?;
    assert_eq!(
        courses.ensure_current_quietly(&current).await?,
        IndexMode::Hybrid
    );
    assert_eq!(
        courses.ensure_current_quietly(&changed).await?,
        IndexMode::Hybrid
    );
    // A source deleted since the index was last current.
    LearningSourceLibraryRepository::new(pool.clone())
        .delete_source(&DeleteLearningSourceRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: changed.clone(),
            source_id: changed_sources[0].clone(),
            expected_revision: 0,
            reason: "No longer used".into(),
        })
        .await?;

    assert_eq!(courses.backfill().await?, 2);
    assert!(builds(&pool, &current).await.is_empty());
    assert_eq!(builds(&pool, &never_indexed).await.len(), 1);
    assert_eq!(builds(&pool, &changed).await.len(), 1);
    // Asked again, the same revisions are the same builds.
    courses.backfill().await?;
    assert_eq!(builds(&pool, &never_indexed).await.len(), 1);
    assert_eq!(builds(&pool, &changed).await.len(), 1);
    Ok(())
}

#[tokio::test]
async fn without_an_embedding_model_startup_queues_nothing() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(&pool, temp.path(), FixedEmbedders::new(None));
    let (program, _) = course(&pool, &pages(2)).await?;
    assert_eq!(courses.backfill().await?, 0);
    assert!(builds(&pool, &program).await.is_empty());
    Ok(())
}
