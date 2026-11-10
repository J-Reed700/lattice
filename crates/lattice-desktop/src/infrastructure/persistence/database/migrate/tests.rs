//! A database recorded by a build that embedded the migrations with other line endings opens in
//! this one. Any other difference in a recorded checksum is left for sqlx to refuse.

use super::{reconcile_line_endings, run_migrations};
use crate::infrastructure::persistence::database::DatabaseConnection;
use lattice_core::shared::persistence::DatabaseHandle;
use parking_lot::Mutex;
use sha2::{Digest, Sha384};
use sqlx::migrate::{Migration, Migrator};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::borrow::Cow;
use std::io::Write;
use std::sync::Arc;

/// A recorded migration: its version and its checksum in hex.
type Row = (i64, String);

fn embedded() -> Migrator {
    sqlx::migrate!("./migrations")
}

async fn empty_database() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect("sqlite::memory:")
        .await
        .unwrap()
}

/// A database this build has migrated, as an earlier start of this build leaves it.
async fn migrated_database() -> SqlitePool {
    let pool = empty_database().await;
    embedded().run(&pool).await.unwrap();
    pool
}

/// `sql` with the other kind of line ending: CRLF where it has LF, LF where it has CRLF.
fn other_line_endings(sql: &str) -> String {
    if sql.contains("\r\n") {
        sql.replace("\r\n", "\n")
    } else {
        sql.replace('\n', "\r\n")
    }
}

/// The checksum sqlx records for `sql`: its SHA-384.
fn checksum_of(sql: &str) -> Vec<u8> {
    Vec::from(Sha384::digest(sql.as_bytes()).as_slice())
}

async fn recorded(pool: &SqlitePool) -> Vec<Row> {
    let rows: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await
            .unwrap();
    rows.into_iter()
        .map(|(version, checksum)| (version, hex::encode(checksum)))
        .collect()
}

/// What `migrator` embeds, in the order `recorded` lists the rows.
fn embedded_rows(migrator: &Migrator) -> Vec<Row> {
    migrator
        .iter()
        .map(|migration| (migration.version, hex::encode(&*migration.checksum)))
        .collect()
}

async fn set_checksum(pool: &SqlitePool, version: i64, checksum: &[u8]) {
    let changed = sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = ?2")
        .bind(checksum)
        .bind(version)
        .execute(pool)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(changed, 1, "migration {version} has a row");
}

async fn set_other_line_endings(pool: &SqlitePool, migration: &Migration) {
    let other = checksum_of(&other_line_endings(&migration.sql));
    assert_ne!(
        other.as_slice(),
        &*migration.checksum,
        "migration {} has line endings to switch",
        migration.version
    );
    set_checksum(pool, migration.version, &other).await;
}

/// Notes in `updates_seen` the version of each `_sqlx_migrations` row updated from here on.
async fn watch_updates(pool: &SqlitePool) {
    sqlx::query("CREATE TABLE updates_seen (version INTEGER NOT NULL)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"
        CREATE TRIGGER migration_updated AFTER UPDATE ON _sqlx_migrations
        BEGIN
            INSERT INTO updates_seen (version) VALUES (OLD.version);
        END
        "#,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn updates_seen(pool: &SqlitePool) -> Vec<i64> {
    sqlx::query_scalar("SELECT version FROM updates_seen ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap()
}

fn refusal(version: i64) -> String {
    format!("migration {version} was previously applied but has been modified")
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    /// The lines `run_migrations` logged itself, without the database driver's.
    fn migrator_lines(&self) -> Vec<String> {
        let target = module_path!().trim_end_matches("::tests");
        let text = String::from_utf8(self.0.lock().clone()).unwrap();
        text.lines()
            .filter(|line| line.contains(target))
            .map(str::to_owned)
            .collect()
    }
}

/// Captures what this thread logs at INFO and above, until the returned guard is dropped.
fn capture_logs() -> (Captured, tracing::subscriber::DefaultGuard) {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    // A callsite first hit on another test's thread while this subscriber was being installed can
    // keep the "nobody listens" interest it computed then.
    tracing::callsite::rebuild_interest_cache();
    (captured, guard)
}

/// `run_migrations` logged its own two lines and nothing else: nothing was reconciled.
fn assert_nothing_reconciled_was_logged(logs: &Captured) {
    let lines = logs.migrator_lines();
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert!(
        lines[0].contains("Starting database migrations"),
        "{lines:#?}"
    );
    assert!(
        lines[1].contains("All migrations completed successfully"),
        "{lines:#?}"
    );
}

#[tokio::test]
async fn a_database_recorded_with_the_other_line_endings_opens_with_this_builds_checksums() {
    let pool = migrated_database().await;
    let migrator = embedded();
    for migration in migrator.iter() {
        set_other_line_endings(&pool, migration).await;
    }
    watch_updates(&pool).await;
    let (logs, _capture) = capture_logs();

    run_migrations(&pool).await.unwrap();

    assert_eq!(recorded(&pool).await, embedded_rows(&migrator));
    let versions: Vec<i64> = migrator.iter().map(|migration| migration.version).collect();
    assert_eq!(updates_seen(&pool).await, versions);
    let lines = logs.migrator_lines();
    assert_eq!(lines.len(), versions.len() + 2, "{lines:#?}");
    for version in &versions {
        let named = version.to_string();
        assert!(
            lines.iter().any(|line| line.contains(&named)),
            "migration {version} is logged: {lines:#?}"
        );
    }
}

#[tokio::test]
async fn only_rows_with_other_line_endings_are_updated_and_a_second_start_updates_none() {
    let pool = migrated_database().await;
    let migrator = embedded();
    let mut switched = Vec::new();
    for migration in migrator.iter().step_by(2) {
        set_other_line_endings(&pool, migration).await;
        switched.push(migration.version);
    }
    watch_updates(&pool).await;

    run_migrations(&pool).await.unwrap();

    assert_eq!(recorded(&pool).await, embedded_rows(&migrator));
    assert_eq!(updates_seen(&pool).await, switched);

    sqlx::query("DELETE FROM updates_seen")
        .execute(&pool)
        .await
        .unwrap();
    let (logs, _capture) = capture_logs();
    run_migrations(&pool).await.unwrap();
    assert!(updates_seen(&pool).await.is_empty());
    assert_nothing_reconciled_was_logged(&logs);
}

#[tokio::test]
async fn a_checksum_that_is_not_the_other_line_endings_of_the_same_sql_is_refused_and_left_alone() {
    let pool = migrated_database().await;
    let migrator = embedded();
    let migrations: Vec<_> = migrator.iter().collect();
    let target = migrations[3];
    let neighbour = migrations[4];
    let first_line_crlf = target.sql.replace("\r\n", "\n").replacen('\n', "\r\n", 1);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "a line added",
            checksum_of(&format!("{}-- edited\n", target.sql)),
        ),
        ("a trailing space", checksum_of(&format!("{} ", target.sql))),
        ("line endings of two kinds", checksum_of(&first_line_crlf)),
        ("another migration", neighbour.checksum.to_vec()),
        (
            "another migration with the other line endings",
            checksum_of(&other_line_endings(&neighbour.sql)),
        ),
        ("one byte", vec![1]),
        ("zeros", vec![0; 48]),
    ];

    for (what, checksum) in cases {
        set_checksum(&pool, target.version, &checksum).await;

        let error = run_migrations(&pool).await.unwrap_err().to_string();

        assert!(error.contains(&refusal(target.version)), "{what}: {error}");
        let mut expected = embedded_rows(&migrator);
        expected[3].1 = hex::encode(&checksum);
        assert_eq!(recorded(&pool).await, expected, "{what}");
    }
}

#[tokio::test]
async fn a_real_change_is_refused_and_left_alone_when_other_rows_differ_only_in_line_endings() {
    let pool = migrated_database().await;
    let migrator = embedded();
    for migration in migrator.iter() {
        set_other_line_endings(&pool, migration).await;
    }
    let migrations: Vec<_> = migrator.iter().collect();
    let changed = migrations[5];
    let edited = checksum_of(&format!("{}-- edited\n", changed.sql));
    set_checksum(&pool, changed.version, &edited).await;

    let error = run_migrations(&pool).await.unwrap_err().to_string();

    assert!(error.contains(&refusal(changed.version)), "{error}");
    let mut expected = embedded_rows(&migrator);
    expected[5].1 = hex::encode(&edited);
    assert_eq!(recorded(&pool).await, expected);
}

#[tokio::test]
async fn a_migration_this_build_does_not_embed_is_refused_as_before() {
    let pool = migrated_database().await;
    let migrator = embedded();
    let newer = migrator
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap()
        + 1;
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
         VALUES (?1, 'from a newer build', 1, ?2, 0)",
    )
    .bind(newer)
    .bind(vec![7u8; 48])
    .execute(&pool)
    .await
    .unwrap();

    let error = run_migrations(&pool).await.unwrap_err().to_string();

    assert!(
        error.contains(&format!(
            "migration {newer} was previously applied but is missing in the resolved migrations"
        )),
        "{error}"
    );
    let mut expected = embedded_rows(&migrator);
    expected.push((newer, hex::encode(vec![7u8; 48])));
    assert_eq!(recorded(&pool).await, expected);
}

#[tokio::test]
async fn a_fresh_database_migrates_as_before() {
    let pool = empty_database().await;
    let (logs, _capture) = capture_logs();

    run_migrations(&pool).await.unwrap();

    assert_eq!(recorded(&pool).await, embedded_rows(&embedded()));
    let conversations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'conversations'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(conversations, 1);
    assert_nothing_reconciled_was_logged(&logs);
}

#[tokio::test]
async fn a_database_that_already_matches_is_not_touched() {
    let pool = migrated_database().await;
    let before = recorded(&pool).await;
    watch_updates(&pool).await;
    let (logs, _capture) = capture_logs();

    run_migrations(&pool).await.unwrap();

    assert_eq!(recorded(&pool).await, before);
    assert!(updates_seen(&pool).await.is_empty());
    assert_nothing_reconciled_was_logged(&logs);
}

#[tokio::test]
async fn a_database_file_recorded_with_the_other_line_endings_opens_on_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lattice.db");
    let migrator = embedded();

    let first_start = DatabaseConnection::new(path.clone()).await.unwrap();
    run_migrations(first_start.pool()).await.unwrap();
    for migration in migrator.iter() {
        set_other_line_endings(first_start.pool().sqlx_pool(), migration).await;
    }
    first_start.close().await.unwrap();

    let next_start = DatabaseConnection::new(path).await.unwrap();
    run_migrations(next_start.pool()).await.unwrap();

    assert_eq!(
        recorded(next_start.pool().sqlx_pool()).await,
        embedded_rows(&migrator)
    );
    next_start.close().await.unwrap();
}

/// `migrator` as a checkout with LF line endings, or with CRLF ones, embeds it.
fn with_line_endings(migrator: &Migrator, crlf: bool) -> Migrator {
    let migrations: Vec<Migration> = migrator
        .iter()
        .map(|migration| {
            let lf = migration.sql.replace("\r\n", "\n");
            let sql = if crlf { lf.replace('\n', "\r\n") } else { lf };
            Migration::new(
                migration.version,
                migration.description.clone(),
                migration.migration_type,
                Cow::Owned(sql),
                migration.no_tx,
            )
        })
        .collect();
    Migrator {
        migrations: Cow::Owned(migrations),
        ..Migrator::DEFAULT
    }
}

#[tokio::test]
async fn a_database_recorded_with_either_line_endings_is_reconciled_to_the_other() {
    for (recorded_with_crlf, opened_with_crlf) in [(true, false), (false, true)] {
        let recorded_by = with_line_endings(&embedded(), recorded_with_crlf);
        let opened_by = with_line_endings(&embedded(), opened_with_crlf);
        let (before, after) = (embedded_rows(&recorded_by), embedded_rows(&opened_by));
        assert!(
            before
                .iter()
                .zip(&after)
                .all(|(old, new)| old.0 == new.0 && old.1 != new.1),
            "every checksum differs between the two checkouts"
        );
        let pool = empty_database().await;
        recorded_by.run(&pool).await.unwrap();

        reconcile_line_endings(&pool, &opened_by).await.unwrap();

        assert_eq!(
            recorded(&pool).await,
            after,
            "recorded with CRLF: {recorded_with_crlf}"
        );
        opened_by.run(&pool).await.unwrap();
    }
}

#[tokio::test]
async fn reconciling_a_database_without_a_migrations_table_does_not_create_one() {
    let pool = empty_database().await;

    reconcile_line_endings(&pool, &embedded()).await.unwrap();

    let objects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(objects, 0);
}

#[tokio::test]
async fn the_rewrites_are_one_transaction_and_are_logged_only_once_committed() {
    let pool = migrated_database().await;
    let migrator = embedded();
    for migration in migrator.iter() {
        set_other_line_endings(&pool, migration).await;
    }
    let before = recorded(&pool).await;
    let third = migrator.iter().nth(2).unwrap().version;
    sqlx::query(&format!(
        r#"
        CREATE TRIGGER refuse_third BEFORE UPDATE ON _sqlx_migrations
        WHEN OLD.version = {third}
        BEGIN
            SELECT RAISE(ABORT, 'refused');
        END
        "#
    ))
    .execute(&pool)
    .await
    .unwrap();
    let (logs, _capture) = capture_logs();

    let error = reconcile_line_endings(&pool, &migrator)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("refused"), "{error}");
    assert_eq!(recorded(&pool).await, before);
    assert!(logs.migrator_lines().is_empty());
}
