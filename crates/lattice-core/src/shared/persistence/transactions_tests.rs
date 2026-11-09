use super::Transactions;
use crate::shared::testing::sqlite::file_options;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const HELD: Duration = Duration::from_millis(300);

async fn open() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(file_options(dir.path().join("t.db")))
        .await
        .unwrap();
    sqlx::query("CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO notes(body) VALUES('first')")
        .execute(&pool)
        .await
        .unwrap();
    (dir, pool)
}

/// Another connection takes the write lock now and commits after `HELD`.
async fn another_writer(pool: &SqlitePool) -> tokio::task::JoinHandle<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("INSERT INTO notes(body) VALUES('other')")
        .execute(&mut *tx)
        .await
        .unwrap();
    tokio::spawn(async move {
        tokio::time::sleep(HELD).await;
        tx.commit().await.unwrap();
    })
}

async fn notes(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM notes")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_write_transaction_that_reads_first_waits_for_the_writer_ahead_of_it() {
    let (_dir, pool) = open().await;
    let other = another_writer(&pool).await;
    let started = Instant::now();
    let mut tx = pool.begin_write().await.unwrap();
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM notes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO notes(body) VALUES('mine')")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    other.await.unwrap();
    assert_eq!(seen, 2, "it read after the other writer committed");
    assert!(started.elapsed() >= HELD / 2, "it waited for the lock");
    assert_eq!(notes(&pool).await, 3);
}

#[tokio::test]
async fn a_writer_meanwhile_waits_for_a_write_transaction_instead_of_failing() {
    let (_dir, pool) = open().await;
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let other = {
        let pool = pool.clone();
        tokio::spawn(async move {
            sqlx::query("INSERT INTO notes(body) VALUES('other')")
                .execute(&pool)
                .await
                .map(|_| ())
        })
    };
    tokio::time::sleep(HELD).await;
    sqlx::query("INSERT INTO notes(body) VALUES('mine')")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    other.await.unwrap().unwrap();
    assert_eq!(notes(&pool).await, 3);
}

#[tokio::test]
async fn a_read_transaction_that_writes_fails_at_once_with_database_is_locked() {
    let (_dir, pool) = open().await;
    let other = another_writer(&pool).await;
    let mut tx = pool.begin_read().await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let started = Instant::now();
    let error = sqlx::query("INSERT INTO notes(body) VALUES('mine')")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("database is locked"), "{error}");
    assert!(
        started.elapsed() < HELD / 2,
        "the busy timeout did not wait"
    );
    drop(tx);
    other.await.unwrap();
}

fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "tests" && name != "testing" {
                sources(&path, found);
            }
        } else if name.ends_with(".rs")
            && name != "tests.rs"
            && name != "test_support.rs"
            && name != "transactions.rs"
            && !name.ends_with("_tests.rs")
        {
            found.push(path);
        }
    }
}

fn line_of(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .map_or(1, |before| before.matches('\n').count() + 1)
}

fn deferred_begins(text: &str) -> Vec<usize> {
    let production = text
        .find("#[cfg(test)]")
        .and_then(|end| text.get(..end))
        .unwrap_or(text);
    let mut found = Vec::new();
    for (offset, _) in production.match_indices(".begin()") {
        let after = production.get(offset + ".begin()".len()..).unwrap_or("");
        if after.trim_start().starts_with(".await") {
            found.push(offset);
        }
    }
    for (offset, _) in production.match_indices("begin_with(") {
        let after = production.get(offset + "begin_with(".len()..).unwrap_or("");
        let argument = after.split(')').next().unwrap_or("");
        if !argument.contains("IMMEDIATE") {
            found.push(offset);
        }
    }
    found.sort_unstable();
    found
        .into_iter()
        .map(|offset| line_of(production, offset))
        .collect()
}

#[test]
fn no_production_code_begins_a_deferred_sqlite_transaction() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for name in [
        "lattice-core",
        "lattice-desktop",
        "lattice-index",
        "lattice-learning",
        "lattice-store",
    ] {
        sources(&crates.join(name).join("src"), &mut files);
    }
    assert!(
        files.len() > 100,
        "the scan found only {} files",
        files.len()
    );
    let mut offenders = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for line in deferred_begins(&text) {
            offenders.push(format!(
                "{}:{line}: use begin_write (a transaction that writes) or begin_read (one that never does)",
                file.strip_prefix(&crates).unwrap_or(file).display()
            ));
        }
    }
    assert!(offenders.is_empty(), "{}", offenders.join("\n"));
}
