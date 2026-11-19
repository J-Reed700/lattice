//! The decision table of [`may_sweep`], run through real files in a tempdir
//! and a real migrated database.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use tempfile::TempDir;

use super::*;

const A: &str = "6f1c0d1e-3b5a-4c52-9d0b-0a1b2c3d4e5f";
const B: &str = "b7e2f4a9-8c61-4d3e-a5f0-9e8d7c6b5a40";

async fn migrated_pool() -> SqlitePool {
    lattice_core::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap()
}

fn files_in(home: &Path) -> LibraryFiles {
    LibraryFiles {
        owner_file: home.join("owner"),
        roots: ["files", "web-snapshots", "chat-images"]
            .map(|name| home.join(name))
            .to_vec(),
    }
}

/// A file one folder down in `root`, where every root keeps its files.
fn put_file(root: &Path) {
    let folder = root.join("0123");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("paper.txt"), "imported").unwrap();
}

async fn record(pool: &SqlitePool, id: &str) {
    sqlx::query("INSERT INTO library_owner (id, owner_id) VALUES (1, ?)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

async fn recorded(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar("SELECT owner_id FROM library_owner WHERE id = 1")
        .fetch_optional(pool)
        .await
        .unwrap()
}

async fn add_document(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) \
         VALUES ('doc', '/library/doc.txt', 'doc.txt', 1, '2026-11-01T00:00:00Z', 'abc')",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// What the owner file holds, if it exists.
fn filed(home: &Path) -> Option<String> {
    std::fs::read_to_string(home.join("owner"))
        .ok()
        .map(|text| text.trim().to_string())
}

/// A column of the table that may be either way.
#[derive(Clone, Copy)]
enum Has {
    No,
    Yes,
    Either,
}

impl Has {
    fn values(self) -> &'static [bool] {
        match self {
            Has::No => &[false],
            Has::Yes => &[true],
            Has::Either => &[false, true],
        }
    }
}

/// What a start leaves in the database and in the owner file.
#[derive(Clone, Copy, Debug)]
enum Then {
    /// Neither changes.
    Nothing,
    /// A new id is written to both.
    NewId,
    /// The id the database holds is written to the file that was missing.
    FileFromDatabase,
}

struct Row {
    name: &'static str,
    database: Option<&'static str>,
    file: Option<&'static str>,
    files: Has,
    documents: Has,
    sweeps: bool,
    then: Then,
}

const TABLE: &[Row] = &[
    Row {
        name: "new install",
        database: None,
        file: None,
        files: Has::No,
        documents: Has::Either,
        sweeps: true,
        then: Then::NewId,
    },
    Row {
        name: "upgrade of an install that has documents",
        database: None,
        file: None,
        files: Has::Yes,
        documents: Has::Yes,
        sweeps: true,
        then: Then::NewId,
    },
    Row {
        name: "new database beside old files",
        database: None,
        file: None,
        files: Has::Yes,
        documents: Has::No,
        sweeps: false,
        then: Then::Nothing,
    },
    Row {
        name: "new database beside another database's library",
        database: None,
        file: Some(A),
        files: Has::Either,
        documents: Has::Either,
        sweeps: false,
        then: Then::Nothing,
    },
    Row {
        name: "the ids agree",
        database: Some(A),
        file: Some(A),
        files: Has::Either,
        documents: Has::Either,
        sweeps: true,
        then: Then::Nothing,
    },
    Row {
        name: "the ids differ",
        database: Some(A),
        file: Some(B),
        files: Has::Either,
        documents: Has::Either,
        sweeps: false,
        then: Then::Nothing,
    },
    Row {
        name: "owner file lost and no files left",
        database: Some(A),
        file: None,
        files: Has::No,
        documents: Has::Either,
        sweeps: true,
        then: Then::FileFromDatabase,
    },
    Row {
        name: "owner file lost and files remain",
        database: Some(A),
        file: None,
        files: Has::Yes,
        documents: Has::Either,
        sweeps: false,
        then: Then::Nothing,
    },
];

/// Run one case of `row` from a clean slate and say what went wrong, if
/// anything.
async fn problem_with(
    row: &Row,
    has_files: bool,
    has_documents: bool,
    db: &Database,
    pool: &SqlitePool,
) -> Option<String> {
    sqlx::query("DELETE FROM library_owner")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM documents")
        .execute(pool)
        .await
        .unwrap();
    let home = TempDir::new().unwrap();
    if let Some(id) = row.database {
        record(pool, id).await;
    }
    if let Some(id) = row.file {
        std::fs::write(home.path().join("owner"), id).unwrap();
    }
    if has_files {
        put_file(&home.path().join("files"));
    }
    if has_documents {
        add_document(pool).await;
    }

    let swept = may_sweep(db, &files_in(home.path())).await;
    let (database, file) = (recorded(pool).await, filed(home.path()));

    let mut problems = Vec::new();
    if swept != row.sweeps {
        problems.push(format!("may_sweep answered {swept}"));
    }
    let as_expected = match row.then {
        Then::Nothing => database.as_deref() == row.database && file.as_deref() == row.file,
        Then::NewId => {
            database
                .as_deref()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
                && file == database
        }
        Then::FileFromDatabase => {
            database.as_deref() == row.database && file.as_deref() == row.database
        }
    };
    if !as_expected {
        problems.push(format!(
            "expected {:?}, found database {database:?} and file {file:?}",
            row.then
        ));
    }
    (!problems.is_empty()).then(|| problems.join("; "))
}

#[tokio::test]
async fn every_row_of_the_decision_table_holds() {
    let pool = migrated_pool().await;
    let db = Database::new(pool.clone());
    let mut failures = Vec::new();
    let mut cases = 0;
    for row in TABLE {
        for &has_files in row.files.values() {
            for &has_documents in row.documents.values() {
                cases += 1;
                if let Some(problem) = problem_with(row, has_files, has_documents, &db, &pool).await
                {
                    failures.push(format!(
                        "{} (files: {has_files}, documents: {has_documents}): {problem}",
                        row.name
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {cases} cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[tokio::test]
async fn a_file_in_any_one_root_counts_as_files() {
    let pool = migrated_pool().await;
    let db = Database::new(pool.clone());
    for root in ["files", "web-snapshots", "chat-images"] {
        let home = TempDir::new().unwrap();
        put_file(&home.path().join(root));

        assert!(
            !may_sweep(&db, &files_in(home.path())).await,
            "a new database beside a file in {root}"
        );
        assert_eq!(recorded(&pool).await, None);
        assert_eq!(filed(home.path()), None);
    }
}

#[tokio::test]
async fn folders_without_files_do_not_count_as_files() {
    let pool = migrated_pool().await;
    let db = Database::new(pool.clone());
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("files").join("0123")).unwrap();
    std::fs::create_dir_all(home.path().join("chat-images").join("prepared")).unwrap();

    assert!(may_sweep(&db, &files_in(home.path())).await);
    assert!(recorded(&pool).await.is_some());
}

#[tokio::test]
async fn the_database_that_took_the_files_keeps_them_on_every_later_start() {
    let pool = migrated_pool().await;
    let db = Database::new(pool.clone());
    let home = TempDir::new().unwrap();
    let files = files_in(home.path());

    assert!(may_sweep(&db, &files).await);
    let id = recorded(&pool).await.expect("a new install records its id");
    put_file(&home.path().join("files"));

    for start in 2..=3 {
        assert!(may_sweep(&db, &files).await, "start {start}");
        assert_eq!(recorded(&pool).await.as_deref(), Some(id.as_str()));
        assert_eq!(filed(home.path()).as_deref(), Some(id.as_str()));
    }
}

#[tokio::test]
async fn any_error_means_no_and_leaves_both_sides_as_they_were() {
    let pool = migrated_pool().await;
    let db = Database::new(pool.clone());

    // The owner file cannot be read: it is a folder.
    let home = TempDir::new().unwrap();
    std::fs::create_dir(home.path().join("owner")).unwrap();
    assert!(
        !may_sweep(&db, &files_in(home.path())).await,
        "an unreadable owner file"
    );
    assert_eq!(recorded(&pool).await, None);

    // The owner file cannot be written: the folder it belongs in is a file.
    // The id is not kept without its file.
    let home = TempDir::new().unwrap();
    std::fs::write(home.path().join("blocked"), "").unwrap();
    let mut files = files_in(home.path());
    files.owner_file = home.path().join("blocked").join("owner");
    assert!(!may_sweep(&db, &files).await, "an unwritable owner file");
    assert_eq!(recorded(&pool).await, None);

    // The database cannot be read: it was never migrated.
    let bare = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let home = TempDir::new().unwrap();
    assert!(
        !may_sweep(&Database::new(bare), &files_in(home.path())).await,
        "a database without the table"
    );
    assert_eq!(filed(home.path()), None);
}
