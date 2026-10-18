#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! Who added a source, finding a saved source again, and the frozen page.

use super::*;
use crate::features::learning::{repository::LearningRepository, tests};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn program(pool: &SqlitePool) -> String {
    let repo = LearningRepository::new(pool.clone());
    let program = tests::fixture();
    repo.create(&program).await.unwrap();
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await
    .unwrap();
    program.summary.id
}

fn page(text: &str) -> CapturedLearningSource {
    CapturedLearningSource {
        title: "Boiling at altitude".into(),
        publisher: None,
        requested_url: Some("https://Example.org/boiling/".into()),
        resolved_url: Some("https://example.org/boiling".into()),
        text: text.into(),
        truncated: false,
        extraction_version: "test_v1".into(),
    }
}

async fn add_web(
    repo: &LearningSourceLibraryRepository,
    program_id: &str,
    text: &str,
    added_by: Option<SourceAddedBy>,
) -> String {
    let version_id = id();
    let source = NewLearningSource {
        operation_id: &id(),
        source_id: &id(),
        version_id: &version_id,
        program_id,
        kind: LearningSourceKind::Web,
        operation_kind: "add_web",
        origin: "https://Example.org/boiling/",
        requested_url: Some("https://Example.org/boiling/"),
        freshness: LearningSourcePolicy::Manual,
    };
    match added_by {
        Some(added_by) => repo
            .add_by(source, page(text), id(), added_by)
            .await
            .unwrap(),
        None => repo.add(source, page(text), id()).await.unwrap(),
    }
    version_id
}

#[tokio::test]
async fn sources_record_who_added_them() {
    let pool = tests::pool().await.unwrap();
    let program_id = program(&pool).await;
    let repo = LearningSourceLibraryRepository::new(pool.clone());
    let by_user = add_web(
        &repo,
        &program_id,
        "Water boils sooner up high.",
        Some(SourceAddedBy::User),
    )
    .await;
    let by_research = add_web(&repo, &program_id, "Air pressure falls with height.", None).await;

    let workspace = repo.workspace(&program_id).await.unwrap();
    let added_by = |version: &str| {
        workspace
            .sources
            .iter()
            .find(|source| source.active_version_id.as_deref() == Some(version))
            .map(|source| SourceAddedBy::from(source.added_by))
            .unwrap()
    };
    assert_eq!(added_by(&by_user), SourceAddedBy::User);
    assert_eq!(added_by(&by_research), SourceAddedBy::Research);
    // The course's own references, saved when it was created.
    assert!(workspace
        .sources
        .iter()
        .filter(
            |source| source.active_version_id.as_deref() != Some(by_user.as_str())
                && source.active_version_id.as_deref() != Some(by_research.as_str())
        )
        .all(|source| SourceAddedBy::from(source.added_by) == SourceAddedBy::Outline));
}

#[tokio::test]
async fn a_saved_source_is_found_by_address_text_and_version() {
    let pool = tests::pool().await.unwrap();
    let program_id = program(&pool).await;
    let repo = LearningSourceLibraryRepository::new(pool.clone());
    let text = "Water boils sooner up high.";
    let version = add_web(&repo, &program_id, text, Some(SourceAddedBy::User)).await;

    for address in [
        "https://example.org/boiling",
        "https://Example.org/boiling/",
    ] {
        let found = repo.saved_web_source(&program_id, address).await.unwrap();
        assert_eq!(
            found.map(|saved| saved.version_id),
            Some(version.clone()),
            "{address}"
        );
    }
    assert!(repo
        .saved_web_source(&program_id, "https://example.org/freezing")
        .await
        .unwrap()
        .is_none());
    let digest = LearningSourceLibraryRepository::text_digest(&format!("  {text}\r\n"));
    let found = repo
        .saved_with_digest(&program_id, &digest)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.version_id, version);
    assert_eq!(found.title, "Boiling at altitude");
    assert_eq!(found.url.as_deref(), Some("https://example.org/boiling"));
    assert_eq!(
        repo.saved_version(&program_id, &version)
            .await
            .unwrap()
            .map(|saved| saved.added_by),
        Some(SourceAddedBy::User)
    );
}

#[tokio::test]
async fn a_frozen_page_is_recorded_on_its_version() {
    let pool = tests::pool().await.unwrap();
    let program_id = program(&pool).await;
    let repo = LearningSourceLibraryRepository::new(pool.clone());
    let version = add_web(
        &repo,
        &program_id,
        "Water boils sooner up high.",
        Some(SourceAddedBy::User),
    )
    .await;
    assert_eq!(
        repo.saved_version(&program_id, &version)
            .await
            .unwrap()
            .unwrap()
            .snapshot_hash,
        None
    );
    repo.link_snapshot(&program_id, &version, "frozen-sha")
        .await
        .unwrap();
    let workspace = repo.workspace(&program_id).await.unwrap();
    let summary = workspace
        .sources
        .iter()
        .find_map(|source| source.active_version.as_ref().filter(|v| v.id == version))
        .unwrap();
    assert_eq!(summary.snapshot_hash.as_deref(), Some("frozen-sha"));
}
