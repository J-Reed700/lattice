//! Which outline pages the outline's research found travels with a course
//! pack (claim classes design §3.5(4)), so the sweep of a course moved to
//! another computer reads the pages research found and leaves what the user
//! chose.
use super::*;
use crate::features::learning::{
    dto::LearningSourceDto,
    pack::{decode_learning_pack, sections::SOURCE_LIBRARY_ENTRY},
};

/// A saved page or document, short enough for the admission gate to refuse.
fn source(title: &str, url: Option<&str>) -> LearningSourceDto {
    LearningSourceDto {
        id: id(),
        title: title.into(),
        url: url.map(str::to_owned),
        excerpt: format!("{title} says little."),
        acquired_at: 1,
    }
}

/// A course whose library holds `sources`, each saved as the outline's.
async fn course_with(pool: &sqlx::SqlitePool, sources: Vec<LearningSourceDto>) -> Result<String> {
    let mut program = fixture();
    program.sources = sources;
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    Ok(program.summary.id)
}

/// Each saved page's title with its mark, by title.
async fn marks(pool: &sqlx::SqlitePool, program_id: &str) -> Result<Vec<(String, i64)>> {
    Ok(sqlx::query_as("SELECT v.title,s.found_by_search FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? ORDER BY v.title")
        .bind(program_id)
        .fetch_all(pool)
        .await?)
}

fn mark(title: &str, found: i64) -> (String, i64) {
    (title.to_owned(), found)
}

/// The pack the exporter writes for the course.
async fn exported(home: &LearningPackRepository, program_id: &str) -> Result<String> {
    Ok(home.export(&move_request(program_id)).await?.exports[0]
        .destination_path
        .clone())
}

#[tokio::test]
async fn a_pack_carries_which_outline_pages_research_found() -> Result<()> {
    let home = pool().await?;
    let course = course_with(
        &home,
        vec![
            source("Found by search", Some("https://example.org/found")),
            source("Chosen address", Some("https://example.org/chosen")),
            source("Chosen document", None),
        ],
    )
    .await?;
    sqlx::query("UPDATE learning_source_library SET found_by_search=1 WHERE program_id=? AND origin='https://example.org/found'")
        .bind(&course)
        .execute(&home)
        .await?;
    let kept = [
        mark("Chosen address", 0),
        mark("Chosen document", 0),
        mark("Found by search", 1),
    ];
    assert_eq!(marks(&home, &course).await?, kept);
    let home_dir = tempfile::tempdir()?;
    let path = exported(&packs(&home, home_dir.path()), &course).await?;

    let fresh = pool().await?;
    let fresh_dir = tempfile::tempdir()?;
    super::r#move::import(
        &packs(&fresh, fresh_dir.path()),
        &path,
        LearningPackConflictPolicy::MergeSafe,
    )
    .await?;

    assert_eq!(marks(&fresh, &course).await?, kept);
    // The sweep of the moved course reads the page research found, and not
    // what the user chose.
    let refused = LearningSourceLibraryRepository::new(fresh.clone())
        .sweep_admission(&course)
        .await?;
    let titles: Vec<&str> = refused.iter().map(|page| page.title.as_str()).collect();
    assert_eq!(titles, ["Found by search"]);
    Ok(())
}

#[tokio::test]
async fn a_pack_written_before_the_mark_imports_by_the_migrations_rule() -> Result<()> {
    let home = pool().await?;
    let course = course_with(
        &home,
        vec![
            source("Outline page", Some("https://example.org/outline")),
            source("Outline document", None),
            source("Page the user added", Some("https://example.org/added")),
            source("Page research added", Some("https://example.org/research")),
        ],
    )
    .await?;
    for (origin, added_by) in [
        ("https://example.org/added", "user"),
        ("https://example.org/research", "research"),
    ] {
        sqlx::query(
            "UPDATE learning_source_library SET added_by=? WHERE program_id=? AND origin=?",
        )
        .bind(added_by)
        .bind(&course)
        .bind(origin)
        .execute(&home)
        .await?;
    }
    let home_dir = tempfile::tempdir()?;
    let path = exported(&packs(&home, home_dir.path()), &course).await?;
    // The pack as an exporter from before the mark wrote it: no source says
    // whether research found it.
    let decoded = decode_learning_pack(&std::fs::read(&path)?)?;
    let old = repack(&decoded, |input| {
        let entry = input
            .entries
            .iter_mut()
            .find(|entry| entry.path == SOURCE_LIBRARY_ENTRY)
            .expect("the source library entry");
        let mut library: serde_json::Value =
            serde_json::from_slice(&entry.bytes).expect("the library is JSON");
        for source in library["sources"].as_array_mut().expect("the sources") {
            source
                .as_object_mut()
                .expect("a source")
                .remove("foundBySearch");
        }
        entry.bytes = serde_json::to_vec(&library).expect("the library as JSON");
    })?;
    let old_path = home_dir.path().join("before-the-mark.lattice-learning");
    std::fs::write(&old_path, old)?;

    let fresh = pool().await?;
    let fresh_dir = tempfile::tempdir()?;
    super::r#move::import(
        &packs(&fresh, fresh_dir.path()),
        old_path.to_str().expect("a UTF-8 path"),
        LearningPackConflictPolicy::MergeSafe,
    )
    .await?;

    // An outline page saved from the web reads as found, as the migration
    // reads it; nothing else does.
    assert_eq!(
        marks(&fresh, &course).await?,
        [
            mark("Outline document", 0),
            mark("Outline page", 1),
            mark("Page research added", 0),
            mark("Page the user added", 0),
        ]
    );
    Ok(())
}
