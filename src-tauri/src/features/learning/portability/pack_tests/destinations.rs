//! Where packs are written: suffixed names in the app's folder, and a file
//! the user chose, which is never inside the app's data or a link.
use super::{move_request, packs, picks, saved, Picks};
use crate::features::learning::{
    pack::decode_learning_pack,
    pack_repository::pack_file_name,
    portability_dto::SaveLearningPackAsResultDto,
    repository::LearningRepository,
    tests::{fixture, pool},
};
use crate::shared::error::{AppError, Result};
use std::path::Path;

async fn course(pool: &sqlx::SqlitePool) -> Result<String> {
    titled_course(pool, &fixture().summary.title).await
}

async fn titled_course(pool: &sqlx::SqlitePool, title: &str) -> Result<String> {
    let mut program = fixture();
    program.summary.title = title.into();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    Ok(program.summary.id)
}

fn names(dir: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<_>>()?;
    names.sort();
    Ok(names)
}

#[test]
fn pack_file_names_drop_separators_and_cap_their_length() {
    assert_eq!(
        pack_file_name("Rust: ownership/borrowing"),
        "Rust ownershipborrowing.lattice-learning"
    );
    assert_eq!(pack_file_name("  a\\b\u{7}c\n "), "abc.lattice-learning");
    assert_eq!(
        pack_file_name("What is C++? <intro>"),
        "What is C++ intro.lattice-learning"
    );
    assert_eq!(pack_file_name(" /:\\ "), "course.lattice-learning");
    assert_eq!(pack_file_name(""), "course.lattice-learning");
    assert_eq!(pack_file_name(".."), "course.lattice-learning");
    // A leading dot would make a hidden file.
    assert_eq!(pack_file_name(".hidden"), "hidden.lattice-learning");
    assert_eq!(pack_file_name(" ... dots"), "dots.lattice-learning");
    assert_eq!(pack_file_name(". "), "course.lattice-learning");
    // Windows device names, in any case, with or without an extension.
    for reserved in [
        "CON",
        "con",
        "Prn",
        "aux",
        "NUL",
        "nul.txt",
        "COM1",
        "com9",
        "LPT1",
        "lpt9.notes",
        "Aux .md",
    ] {
        assert_eq!(
            pack_file_name(reserved),
            "course.lattice-learning",
            "{reserved}"
        );
    }
    for ordinary in [
        "Console",
        "COM10",
        "COM0",
        "LPT",
        "Null set",
        "Auxiliary verbs",
    ] {
        assert_eq!(
            pack_file_name(ordinary),
            format!("{ordinary}.lattice-learning")
        );
    }
    let long = pack_file_name(&"é".repeat(100));
    let stem = long.strip_suffix(".lattice-learning").expect("extension");
    // 'é' is two bytes: the cut lands on the char boundary at or below 80.
    assert_eq!(stem.len(), 80);
    assert!(stem.chars().all(|c| c == 'é'));
    let long = pack_file_name(&format!("a{}", "é".repeat(100)));
    assert_eq!(
        long.strip_suffix(".lattice-learning").map(str::len),
        Some(79)
    );
}

#[tokio::test]
async fn managed_exports_are_named_after_the_course_and_take_the_next_free_suffix() -> Result<()> {
    let pool = pool().await?;
    let program_id = titled_course(&pool, "Shared name").await?;
    let app_data = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());
    for _ in 0..3 {
        packs.export(&move_request(&program_id)).await?;
    }
    assert_eq!(
        names(&app_data.path().join("learning-packs"))?,
        [
            "Shared name (2).lattice-learning",
            "Shared name (3).lattice-learning",
            "Shared name.lattice-learning",
        ]
    );
    let workspace = packs.workspace(&program_id).await?;
    assert_eq!(workspace.exports.len(), 3);
    Ok(())
}

#[tokio::test]
async fn save_as_writes_and_records_the_chosen_file_and_a_retry_asks_nothing() -> Result<()> {
    let pool = pool().await?;
    let program_id = course(&pool).await?;
    let app_data = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());
    let target = elsewhere.path().join("course.lattice-learning");
    let request = move_request(&program_id);
    let export = saved(packs.save_as(&request, &picks(&target)).await?)?;
    let written = std::fs::canonicalize(elsewhere.path())?.join("course.lattice-learning");
    assert_eq!(export.destination_path, written.to_string_lossy());
    let decoded = decode_learning_pack(&tokio::fs::read(&target).await?)?;
    assert_eq!(decoded.manifest.root_sha256, export.manifest.root_sha256);
    assert_eq!(packs.workspace(&program_id).await?.exports[0].id, export.id);
    // The same request replays without a dialog: same record, no second file.
    let replay = saved(packs.save_as(&request, &Picks(None)).await?)?;
    assert_eq!(replay.id, export.id);
    assert_eq!(names(elsewhere.path())?, ["course.lattice-learning"]);
    Ok(())
}

#[tokio::test]
async fn a_dismissed_save_dialog_writes_and_records_nothing() -> Result<()> {
    let pool = pool().await?;
    let program_id = course(&pool).await?;
    let app_data = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());
    let result = packs
        .save_as(&move_request(&program_id), &Picks(None))
        .await?;
    assert!(matches!(result, SaveLearningPackAsResultDto::Cancelled));
    assert!(packs.workspace(&program_id).await?.exports.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_replayed_export_whose_file_was_deleted_is_written_again() -> Result<()> {
    let pool = pool().await?;
    let program_id = course(&pool).await?;
    let app_data = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());

    let request = move_request(&program_id);
    let target = elsewhere.path().join("course.lattice-learning");
    let first = saved(packs.save_as(&request, &picks(&target)).await?)?;
    tokio::fs::remove_file(&target).await?;
    let again = saved(packs.save_as(&request, &picks(&target)).await?)?;
    assert_ne!(again.id, first.id);
    decode_learning_pack(&tokio::fs::read(&target).await?)?;
    // The ledger now answers with the new file, without a dialog.
    let replayed = saved(packs.save_as(&request, &Picks(None)).await?)?;
    assert_eq!(replayed.id, again.id);

    // The same holds for the app's own folder.
    let managed = move_request(&program_id);
    let written = packs.export(&managed).await?;
    let path = written
        .exports
        .iter()
        .find(|export| export.id != again.id)
        .map(|export| export.destination_path.clone())
        .expect("managed export");
    tokio::fs::remove_file(&path).await?;
    let rewritten = packs.export(&managed).await?;
    decode_learning_pack(&tokio::fs::read(&path).await?)?;
    assert_eq!(rewritten.exports.len(), 2);
    assert!(rewritten
        .exports
        .iter()
        .any(|export| export.destination_path == path));
    Ok(())
}

#[tokio::test]
async fn save_as_refuses_app_data_links_and_non_pack_files() -> Result<()> {
    let pool = pool().await?;
    let program_id = course(&pool).await?;
    let app_data = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());

    for inside in [
        app_data.path().join("course.lattice-learning"),
        app_data
            .path()
            .join("learning-packs")
            .join("course.lattice-learning"),
    ] {
        if let Some(parent) = inside.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let error = packs
            .save_as(&move_request(&program_id), &picks(&inside))
            .await
            .expect_err("inside the app data folder");
        assert!(
            error
                .to_string()
                .contains("outside Lattice's own data folder"),
            "{error}"
        );
    }

    let real = elsewhere.path().join("real.txt");
    tokio::fs::write(&real, b"keep me").await?;
    #[cfg(unix)]
    {
        let link = elsewhere.path().join("link.lattice-learning");
        std::os::unix::fs::symlink(&real, &link)?;
        let error = packs
            .save_as(&move_request(&program_id), &picks(&link))
            .await
            .expect_err("a link is never followed or replaced");
        assert!(error.to_string().contains("link or a folder"), "{error}");
        assert!(tokio::fs::symlink_metadata(&link)
            .await?
            .file_type()
            .is_symlink());
    }

    let folder = elsewhere.path().join("folder.lattice-learning");
    tokio::fs::create_dir(&folder).await?;
    assert!(packs
        .save_as(&move_request(&program_id), &picks(&folder))
        .await
        .is_err());

    let error = packs
        .save_as(&move_request(&program_id), &picks(&real))
        .await
        .expect_err("an existing non-pack file");
    assert!(
        error.to_string().contains("not a .lattice-learning file"),
        "{error}"
    );
    assert_eq!(tokio::fs::read(&real).await?, b"keep me");

    let missing = elsewhere
        .path()
        .join("no-such-folder")
        .join("course.lattice-learning");
    assert!(packs
        .save_as(&move_request(&program_id), &picks(&missing))
        .await
        .is_err());
    assert!(packs.workspace(&program_id).await?.exports.is_empty());
    Ok(())
}

#[tokio::test]
async fn save_as_adds_the_extension_and_replaces_an_existing_pack_atomically() -> Result<()> {
    let pool = pool().await?;
    let program_id = course(&pool).await?;
    let app_data = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());
    let first = saved(
        packs
            .save_as(
                &move_request(&program_id),
                &picks(&elsewhere.path().join("My course")),
            )
            .await?,
    )?;
    assert!(first
        .destination_path
        .ends_with("My course.lattice-learning"));
    let second = saved(
        packs
            .save_as(
                &move_request(&program_id),
                &picks(&elsewhere.path().join("My course.lattice-learning")),
            )
            .await?,
    )?;
    assert_eq!(second.destination_path, first.destination_path);
    assert_ne!(second.manifest.pack_id, first.manifest.pack_id);
    let decoded = decode_learning_pack(&tokio::fs::read(&second.destination_path).await?)?;
    assert_eq!(decoded.manifest.pack_id, second.manifest.pack_id);
    // No temporary file is left beside the replaced pack.
    assert_eq!(names(elsewhere.path())?, ["My course.lattice-learning"]);
    Ok(())
}

#[tokio::test]
async fn save_as_never_replaces_a_pack_the_dialog_did_not_name() -> Result<()> {
    let pool = pool().await?;
    let program_id = course(&pool).await?;
    let app_data = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());
    let existing = saved(
        packs
            .save_as(
                &move_request(&program_id),
                &picks(&elsewhere.path().join("My course.lattice-learning")),
            )
            .await?,
    )?;
    // The user typed "My course" and the dialog did not add the extension:
    // it asked about "My course", never about the pack beside it.
    let error = packs
        .save_as(
            &move_request(&program_id),
            &picks(&elsewhere.path().join("My course")),
        )
        .await
        .expect_err("the existing pack was never confirmed");
    assert!(
        matches!(&error, AppError::InvalidInput(message)
            if message.contains("My course.lattice-learning already exists")),
        "{error}"
    );
    let kept = decode_learning_pack(&tokio::fs::read(&existing.destination_path).await?)?;
    assert_eq!(kept.manifest.pack_id, existing.manifest.pack_id);
    assert_eq!(names(elsewhere.path())?, ["My course.lattice-learning"]);
    assert_eq!(packs.workspace(&program_id).await?.exports.len(), 1);
    Ok(())
}
