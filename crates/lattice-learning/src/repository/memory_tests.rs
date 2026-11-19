//! Tests for lesson notes, card drafts and the program memory workspace.
use super::*;
use crate::generation::GeneratedLearningCardDraft;
use crate::tests::{fixture, pool, ready_program};
use sqlx::SqlitePool;

async fn fk_pool() -> Result<SqlitePool> {
    let pool = pool().await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;
    Ok(pool)
}

async fn accepted_unprepared_program(repo: &LearningRepository) -> Result<LearningProgramDto> {
    let program = fixture();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    repo.get(&program.summary.id).await
}

fn manual(
    program: &LearningProgramDto,
    lesson_id: &str,
    sources: &[&str],
) -> SaveLearningCardDraftRequestDto {
    SaveLearningCardDraftRequestDto {
        draft_id: None,
        program_id: program.summary.id.clone(),
        lesson_id: lesson_id.to_owned(),
        question: "  Which river fed the earliest Egyptian farms?  ".into(),
        answer: " The Nile ".into(),
        explanation: " Its yearly flood left fertile silt. ".into(),
        source_ids: sources.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn generated(sources: &[&str]) -> GeneratedLearningCardDraft {
    GeneratedLearningCardDraft {
        question: "What does chlorophyll absorb?".into(),
        answer: "Mostly red and blue light.".into(),
        explanation: "Green light is reflected, which is why leaves look green.".into(),
        source_ids: sources.iter().map(|s| (*s).to_owned()).collect(),
    }
}

async fn cite_in_first_block(pool: &SqlitePool, lesson_id: &str, source_id: &str) -> Result<()> {
    sqlx::query("UPDATE learning_blocks SET source_ids_json=? WHERE lesson_id=? AND ordinal=0")
        .bind(json(&vec![source_id.to_owned()])?)
        .bind(lesson_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn add_source(
    pool: &SqlitePool,
    program_id: &str,
    id: &str,
    url: Option<&str>,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_sources(id,program_id,title,url,excerpt,acquired_at) VALUES(?,?,?,?,?,1)")
        .bind(id)
        .bind(program_id)
        .bind(format!("Source {id}"))
        .bind(url)
        .bind("Bread rises because yeast releases carbon dioxide.")
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn discarding_a_draft_is_idempotent_and_blocks_later_acceptance() -> Result<()> {
    let pool = fk_pool().await?;
    let database = Database::new(pool.clone());
    let repo = LearningRepository::new(database.clone());
    let program = ready_program(&repo).await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let program_id = program.summary.id.clone();

    let draft_id = repo
        .save_manual_draft(&manual(&program, &lesson_id, &[]))
        .await?;
    let pending = repo.memory_state(&program_id).await?;
    assert_eq!(pending.drafts.len(), 1);
    assert_eq!(
        pending.drafts[0].question,
        "Which river fed the earliest Egyptian farms?"
    );
    assert_eq!(pending.drafts[0].answer, "The Nile");
    assert_eq!(pending.drafts[0].origin, LearningCardDraftOrigin::Manual);

    repo.discard_card_draft(&program_id, &draft_id).await?;
    repo.discard_card_draft(&program_id, &draft_id).await?;
    assert!(repo.memory_state(&program_id).await?.drafts.is_empty());

    let accept = repo.accept_card_draft(&program_id, &draft_id).await;
    assert!(
        matches!(accept, Err(AppError::InvalidInput(_))),
        "{accept:?}"
    );

    let missing = repo.discard_card_draft(&program_id, "no-such-draft").await;
    assert!(matches!(missing, Err(AppError::NotFound(_))), "{missing:?}");

    let other = ready_program(&repo).await?;
    let foreign = repo.discard_card_draft(&other.summary.id, &draft_id).await;
    assert!(matches!(foreign, Err(AppError::NotFound(_))), "{foreign:?}");

    let accepted_id = repo
        .save_manual_draft(&manual(&program, &lesson_id, &[]))
        .await?;
    repo.accept_card_draft(&program_id, &accepted_id).await?;
    let discard_accepted = repo.discard_card_draft(&program_id, &accepted_id).await;
    assert!(
        matches!(discard_accepted, Err(AppError::NotFound(_))),
        "{discard_accepted:?}"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM learning_card_drafts WHERE id=?")
        .bind(&accepted_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(status, "accepted");
    Ok(())
}

#[tokio::test]
async fn replaying_an_accepted_draft_reports_a_deleted_card() -> Result<()> {
    let pool = fk_pool().await?;
    let database = Database::new(pool.clone());
    let repo = LearningRepository::new(database.clone());
    let program = ready_program(&repo).await?;
    let lesson_id = program.modules[0].lessons[1].id.clone();
    let program_id = program.summary.id.clone();
    let draft_id = repo
        .save_manual_draft(&manual(&program, &lesson_id, &[]))
        .await?;
    let card_id = repo.accept_card_draft(&program_id, &draft_id).await?;
    assert_eq!(
        repo.memory_state(&program_id).await?.accepted_cards.len(),
        1
    );

    sqlx::query("DELETE FROM study_cards WHERE id=?")
        .bind(&card_id)
        .execute(&pool)
        .await?;
    let replay = repo.accept_card_draft(&program_id, &draft_id).await;
    assert!(matches!(replay, Err(AppError::NotFound(_))), "{replay:?}");
    let state = repo.memory_state(&program_id).await?;
    assert!(state.accepted_cards.is_empty());
    assert!(state.drafts.is_empty());

    let unknown = repo.accept_card_draft(&program_id, "no-such-draft").await;
    assert!(matches!(unknown, Err(AppError::NotFound(_))), "{unknown:?}");
    Ok(())
}

#[tokio::test]
async fn memory_tools_require_an_accepted_program_and_a_prepared_lesson() -> Result<()> {
    let pool = fk_pool().await?;
    let database = Database::new(pool.clone());
    let repo = LearningRepository::new(database.clone());

    let missing = repo.memory_state("no-such-program").await;
    assert!(matches!(missing, Err(AppError::NotFound(_))), "{missing:?}");

    let draft = fixture();
    repo.create(&draft).await?;
    let draft_lesson = draft.modules[0].lessons[0].id.clone();
    let rejected = repo
        .ensure_lesson_note(&draft.summary.id, &draft_lesson, "Notes", "")
        .await;
    assert!(
        matches!(rejected, Err(AppError::InvalidInput(_))),
        "{rejected:?}"
    );
    let untouched = repo.memory_state(&draft.summary.id).await?;
    assert!(untouched.journal_id.is_none() && untouched.deck_id.is_none());
    let roots: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_memory WHERE program_id=?")
        .bind(&draft.summary.id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(roots, 0, "a rejected ensure must roll back its root row");

    let active = accepted_unprepared_program(&repo).await?;
    let lesson_id = active.modules[0].lessons[0].id.clone();
    let (note_id, created) = repo
        .ensure_lesson_note(
            &active.summary.id,
            &lesson_id,
            "  Fermentation notes  ",
            "body",
        )
        .await?;
    assert!(created);
    let title: String = sqlx::query_scalar("SELECT title FROM daily_notes_workspace WHERE id=?")
        .bind(&note_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(title, "Fermentation notes");
    let state = repo.memory_state(&active.summary.id).await?;
    assert!(state.journal_id.is_some() && state.deck_id.is_some());
    assert_eq!(state.lesson_notes, vec![(lesson_id.clone(), note_id)]);

    let unprepared = repo
        .save_manual_draft(&manual(&active, &lesson_id, &[]))
        .await;
    assert!(
        matches!(unprepared, Err(AppError::InvalidInput(_))),
        "{unprepared:?}"
    );
    let unprepared_generated = repo
        .save_generated_drafts(&active.summary.id, &lesson_id, &[generated(&[])])
        .await;
    assert!(
        matches!(unprepared_generated, Err(AppError::InvalidInput(_))),
        "{unprepared_generated:?}"
    );

    let foreign_lesson = repo
        .ensure_lesson_note(&active.summary.id, &draft_lesson, "Notes", "")
        .await;
    assert!(
        matches!(foreign_lesson, Err(AppError::NotFound(_))),
        "{foreign_lesson:?}"
    );
    Ok(())
}

#[tokio::test]
async fn draft_sources_must_be_unique_owned_and_cited_by_the_lesson() -> Result<()> {
    let pool = fk_pool().await?;
    let database = Database::new(pool.clone());
    let repo = LearningRepository::new(database.clone());
    let program = ready_program(&repo).await?;
    let other = ready_program(&repo).await?;
    let program_id = program.summary.id.clone();
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let cited = program.sources[0].id.clone();
    let uncited = "uncited-source";
    add_source(
        &pool,
        &program_id,
        uncited,
        Some("https://example.org/bread"),
    )
    .await?;

    let duplicate = repo
        .save_manual_draft(&manual(&program, &lesson_id, &[&cited, &cited]))
        .await;
    assert!(
        matches!(duplicate, Err(AppError::InvalidInput(_))),
        "{duplicate:?}"
    );
    let outside = repo
        .save_manual_draft(&manual(&program, &lesson_id, &[&other.sources[0].id]))
        .await;
    assert!(
        matches!(outside, Err(AppError::InvalidInput(_))),
        "{outside:?}"
    );
    // A learner may cite any program source, even one the lesson never used.
    repo.save_manual_draft(&manual(&program, &lesson_id, &[uncited]))
        .await?;

    cite_in_first_block(&pool, &lesson_id, &cited).await?;
    let not_in_lesson = repo
        .save_generated_drafts(&program_id, &lesson_id, &[generated(&[uncited])])
        .await;
    assert!(
        matches!(not_in_lesson, Err(AppError::InvalidInput(_))),
        "{not_in_lesson:?}"
    );
    let uncited_generated = repo
        .save_generated_drafts(&program_id, &lesson_id, &[generated(&[])])
        .await;
    assert!(
        matches!(uncited_generated, Err(AppError::InvalidInput(_))),
        "{uncited_generated:?}"
    );

    // One bad draft rolls back the whole generated batch.
    let batch = repo
        .save_generated_drafts(
            &program_id,
            &lesson_id,
            &[generated(&[&cited]), generated(&[uncited])],
        )
        .await;
    assert!(batch.is_err());
    let generated_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_card_drafts WHERE program_id=? AND origin='generated'",
    )
    .bind(&program_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(generated_rows, 0);

    repo.save_generated_drafts(&program_id, &lesson_id, &[generated(&[&cited])])
        .await?;
    let state = repo.memory_state(&program_id).await?;
    let saved = state
        .drafts
        .iter()
        .find(|d| d.origin == LearningCardDraftOrigin::Generated)
        .expect("generated draft");
    assert_eq!(saved.source_ids, vec![cited.clone()]);

    // Editing a generated draft keeps the generated rule: it must stay cited.
    let mut clear_sources = manual(&program, &lesson_id, &[]);
    clear_sources.draft_id = Some(saved.id.clone());
    let cleared = repo.save_manual_draft(&clear_sources).await;
    assert!(
        matches!(cleared, Err(AppError::InvalidInput(_))),
        "{cleared:?}"
    );

    let mut missing = manual(&program, &lesson_id, &[]);
    missing.draft_id = Some("no-such-draft".into());
    let missing = repo.save_manual_draft(&missing).await;
    assert!(matches!(missing, Err(AppError::NotFound(_))), "{missing:?}");
    Ok(())
}

#[tokio::test]
async fn accepting_drafts_records_citations_or_an_honest_fallback() -> Result<()> {
    let pool = fk_pool().await?;
    let database = Database::new(pool.clone());
    let repo = LearningRepository::new(database.clone());
    let program = ready_program(&repo).await?;
    let program_id = program.summary.id.clone();
    let lesson = program.modules[1].lessons[0].clone();
    add_source(&pool, &program_id, "doc-77:chunk-3", None).await?;
    add_source(
        &pool,
        &program_id,
        "web-bread",
        Some("https://example.org/yeast"),
    )
    .await?;

    let cited_id = repo
        .save_manual_draft(&manual(
            &program,
            &lesson.id,
            &["doc-77:chunk-3", "web-bread"],
        ))
        .await?;
    let cited_card = repo.accept_card_draft(&program_id, &cited_id).await?;
    let manual_id = repo
        .save_manual_draft(&manual(&program, &lesson.id, &[]))
        .await?;
    let manual_card = repo.accept_card_draft(&program_id, &manual_id).await?;
    repo.save_generated_drafts(&program_id, &lesson.id, &[generated(&[])])
        .await?;
    let generated_id: String = sqlx::query_scalar(
        "SELECT id FROM learning_card_drafts WHERE program_id=? AND origin='generated'",
    )
    .bind(&program_id)
    .fetch_one(&pool)
    .await?;
    let generated_card = repo.accept_card_draft(&program_id, &generated_id).await?;

    let state = repo.memory_state(&program_id).await?;
    let deck_id = state.deck_id.clone().expect("deck");
    let deck = StudyRepository::new(database.clone()).get(&deck_id).await?;
    let card = |id: &str| {
        deck.cards
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .expect("card")
    };

    let cited = card(&cited_card);
    assert_eq!(
        cited.question,
        "Which river fed the earliest Egyptian farms?"
    );
    assert_eq!(cited.topic, lesson.title);
    assert_eq!(cited.citations.len(), 2);
    assert_eq!(cited.citations[0].document_id, "doc-77");
    assert_eq!(cited.citations[0].chunk_id, "chunk-3");
    assert_eq!(cited.citations[0].file_path, "");
    assert_eq!(cited.citations[0].url, None);
    assert_eq!(cited.citations[1].document_id, "");
    assert_eq!(cited.citations[1].chunk_id, "web-bread");
    assert_eq!(
        cited.citations[1].url.as_deref(),
        Some("https://example.org/yeast")
    );
    assert_eq!(cited.source.chunk_id, cited.citations[0].chunk_id);

    let personal = card(&manual_card);
    assert_eq!(personal.citations.len(), 1);
    assert_eq!(personal.source.file_name, "Personal learning card");
    assert_eq!(
        personal.source.chunk_id,
        format!("learning-memory:{manual_id}")
    );

    let authored = card(&generated_card);
    assert_eq!(authored.source.file_name, lesson.title);
    assert!(authored.source.excerpt.starts_with("AI-authored"));

    assert_eq!(state.accepted_cards.len(), 3);
    let origin = state
        .accepted_cards
        .iter()
        .find(|c| c.card_id == cited_card)
        .expect("origin row");
    assert_eq!(origin.origin, LearningCardDraftOrigin::Manual);
    assert_eq!(
        origin.source_ids,
        vec!["doc-77:chunk-3".to_owned(), "web-bread".to_owned()]
    );
    assert!(state.drafts.is_empty());

    // Once the cited source is gone the pending draft can no longer be accepted.
    let orphan_id = repo
        .save_manual_draft(&manual(&program, &lesson.id, &["web-bread"]))
        .await?;
    sqlx::query("DELETE FROM learning_sources WHERE program_id=? AND id='web-bread'")
        .bind(&program_id)
        .execute(&pool)
        .await?;
    let orphan = repo.accept_card_draft(&program_id, &orphan_id).await;
    assert!(
        matches!(orphan, Err(AppError::InvalidInput(_))),
        "{orphan:?}"
    );
    Ok(())
}

/// The notebook picker prints a notebook's icon as text, so a course's notebook
/// gets an emoji, not the name of an icon.
#[tokio::test]
async fn a_course_notebook_gets_an_emoji_for_its_icon() -> Result<()> {
    let pool = fk_pool().await?;
    let database = Database::new(pool.clone());
    let repo = LearningRepository::new(database.clone());
    let active = accepted_unprepared_program(&repo).await?;
    let lesson_id = active.modules[0].lessons[0].id.clone();
    repo.ensure_lesson_note(&active.summary.id, &lesson_id, "Notes", "")
        .await?;
    let journal_id = repo
        .memory_state(&active.summary.id)
        .await?
        .journal_id
        .unwrap();
    let icon: Option<String> = sqlx::query_scalar("SELECT icon FROM journals WHERE id=?")
        .bind(&journal_id)
        .fetch_one(&pool)
        .await?;
    let icon = icon.unwrap_or_default();
    assert!(
        !icon.is_empty() && !icon.chars().any(|c| c.is_ascii_alphanumeric()),
        "{icon:?}"
    );
    Ok(())
}

/// The notebooks the Studio made before get the emoji a new one gets; a
/// notebook of the user's own keeps its icon.
#[tokio::test]
async fn the_course_notebooks_made_before_get_the_emoji_too() -> Result<()> {
    const ICON_FIX: &str = "20261102120000";
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await?;
    let earlier = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../lattice-desktop/migrations"
    ))
    .unwrap()
    {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.as_str() < ICON_FIX {
            std::fs::copy(&path, earlier.path().join(&name)).unwrap();
        }
    }
    sqlx::migrate::Migrator::new(earlier.path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();
    for statement in [
        "INSERT INTO learning_programs (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) \
         VALUES ('p1', 'Tides', 'Read tide tables', 'active', 1, 'none', 30, 'qwen', 1000)",
        "INSERT INTO journals (id, name, icon) VALUES ('journal_learning_p1', 'Studio · Tides · p1', 'book-open')",
        "INSERT INTO journals (id, name, icon) VALUES ('mine', 'Mine', 'book-open')",
        "INSERT INTO learning_memory (program_id, journal_id, deck_id, created_at) VALUES ('p1', 'journal_learning_p1', NULL, 1000)",
    ] {
        sqlx::query(statement).execute(&pool).await?;
    }

    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .unwrap();

    let database = Database::new(pool.clone());
    let repo = LearningRepository::new(database.clone());
    let active = accepted_unprepared_program(&repo).await?;
    let lesson_id = active.modules[0].lessons[0].id.clone();
    repo.ensure_lesson_note(&active.summary.id, &lesson_id, "Notes", "")
        .await?;
    let new_icon: Option<String> = sqlx::query_scalar(
        "SELECT j.icon FROM journals j JOIN learning_memory m ON m.journal_id=j.id WHERE m.program_id=?",
    )
    .bind(&active.summary.id)
    .fetch_one(&pool)
    .await?;
    let icon_of = |id: &'static str| {
        sqlx::query_scalar::<_, Option<String>>("SELECT icon FROM journals WHERE id=?")
            .bind(id)
            .fetch_one(&pool)
    };
    let old_icon = icon_of("journal_learning_p1").await?;
    assert!(
        old_icon.as_deref().is_some_and(
            |icon| !icon.is_empty() && !icon.chars().any(|c| c.is_ascii_alphanumeric())
        ),
        "{old_icon:?}"
    );
    assert_eq!(old_icon, new_icon);
    assert_eq!(icon_of("mine").await?.as_deref(), Some("book-open"));
    Ok(())
}
