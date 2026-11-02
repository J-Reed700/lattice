//! The admission sweep over a program's saved pages.
use super::*;
use crate::features::learning::{repository::LearningRepository, tests};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn words(count: usize) -> String {
    "word ".repeat(count)
}

async fn active_program(pool: &SqlitePool) -> Result<String> {
    let repo = LearningRepository::new(pool.clone());
    let program = tests::fixture();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    Ok(program.summary.id)
}

/// Saves a page as a web source added by `by`; returns the version's id.
async fn save(
    library: &LearningSourceLibraryRepository,
    program_id: &str,
    by: SourceAddedBy,
    (url, title, text): (&str, &str, &str),
) -> Result<String> {
    let (operation_id, source_id, version_id) = (id(), id(), id());
    library
        .add_by(
            NewLearningSource {
                operation_id: &operation_id,
                source_id: &source_id,
                version_id: &version_id,
                program_id,
                kind: LearningSourceKind::Web,
                operation_kind: "add_web",
                origin: url,
                requested_url: Some(url),
                freshness: LearningSourcePolicy::Fixed,
            },
            CapturedLearningSource {
                title: title.into(),
                publisher: None,
                requested_url: Some(url.into()),
                resolved_url: Some(url.into()),
                text: text.into(),
                truncated: false,
                extraction_version: "test_v1".into(),
                signals: crate::features::learning::references::trust::capture::not_extracted(
                    "test fixture",
                ),
            },
            id(),
            by,
        )
        .await?;
    Ok(version_id)
}

#[tokio::test]
async fn the_sweep_marks_a_page_the_machine_saved_that_is_not_a_source_and_leaves_it_out_of_retrieval(
) -> Result<()> {
    let pool = tests::pool().await?;
    let program_id = active_program(&pool).await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let repo = LearningRepository::new(pool.clone());
    let article = format!("# Reading files\n{}", words(400));
    save(
        &library,
        &program_id,
        SourceAddedBy::Research,
        ("https://docs.example.org/guide", "Reading files", &article),
    )
    .await?;
    let login = save(
        &library,
        &program_id,
        SourceAddedBy::Research,
        ("https://access.example.com/login", "Sign in", &words(65)),
    )
    .await?;
    // A short page the user saved is theirs to keep.
    save(
        &library,
        &program_id,
        SourceAddedBy::User,
        ("https://example.org/mine", "My notes", &words(20)),
    )
    .await?;
    let held = repo.verification_sources(&program_id).await?;

    let refused = library.sweep_admission(&program_id).await?;

    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].version_id, login);
    assert_eq!(refused[0].title, "Sign in");
    assert_eq!(refused[0].reason, "login page, 65 words");
    // It leaves what claims are checked against and what citations name,
    // but stays in the library for the user to delete.
    let after = repo.verification_sources(&program_id).await?;
    assert_eq!(after.len(), held.len() - 1);
    assert!(after.iter().all(|source| source.id != login));
    assert!(library
        .prepare_sources(&program_id)
        .await?
        .iter()
        .all(|source| source.id != login));
    assert!(library
        .active_versions(&program_id)
        .await?
        .iter()
        .all(|version| version.id != login));
    assert_eq!(library.refused_sources(&program_id).await?, refused);
    // Each page is read once under the gate's version.
    assert!(library.sweep_admission(&program_id).await?.is_empty());
    assert_eq!(library.refused_sources(&program_id).await?, refused);
    Ok(())
}

/// The acceptance report reads the floors by eye: what the gate refused and
/// the short pages it let through.
#[tokio::test]
async fn the_short_pages_the_gate_admitted_are_listed_for_the_floors_to_be_read() -> Result<()> {
    let pool = tests::pool().await?;
    let program_id = active_program(&pool).await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let page = |name: &str, count: usize| {
        (
            format!("https://docs.example.org/{name}"),
            name.to_owned(),
            words(count),
        )
    };
    let pages = [
        page("changelog", 200),
        page("guide", 399),
        page("manual", 400),
    ];
    for (url, title, text) in &pages {
        save(
            &library,
            &program_id,
            SourceAddedBy::Research,
            (url, title, text),
        )
        .await?;
    }
    // Nothing is listed before the gate has read the pages.
    assert!(library.admitted_under(&program_id, 400).await?.is_empty());

    library.sweep_admission(&program_id).await?;

    let short = library.admitted_under(&program_id, 400).await?;
    let listed: Vec<(&str, usize)> = short
        .iter()
        .filter(|page| ["changelog", "guide", "manual"].contains(&page.title.as_str()))
        .map(|page| (page.title.as_str(), page.words))
        .collect();
    assert_eq!(listed, [("changelog", 200), ("guide", 399)]);
    Ok(())
}
