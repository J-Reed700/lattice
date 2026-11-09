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

fn find(bytes: &[u8], from: usize, pattern: &[u8]) -> Option<usize> {
    let rest = bytes.get(from..)?;
    rest.windows(pattern.len())
        .position(|window| window == pattern)
        .map(|position| from + position)
}

fn item_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    let mut parens = 0_i32;
    let mut braces = 0_i32;
    let mut entered = false;
    while let Some(&c) = bytes.get(i) {
        let next = bytes.get(i + 1).copied();
        if c == b'/' && next == Some(b'/') {
            i = find(bytes, i, b"\n").unwrap_or(bytes.len());
        } else if c == b'/' && next == Some(b'*') {
            i = find(bytes, i + 2, b"*/").map_or(bytes.len(), |end| end + 2);
        } else if c == b'"' {
            i += 1;
            while let Some(&inside) = bytes.get(i) {
                if inside == b'"' {
                    break;
                }
                i += if inside == b'\\' { 2 } else { 1 };
            }
            i += 1;
        } else if c == b'r'
            && !i
                .checked_sub(1)
                .and_then(|before| bytes.get(before))
                .is_some_and(|&before| before.is_ascii_alphanumeric() || before == b'_')
            && raw_string_end(bytes, i).is_some()
        {
            i = raw_string_end(bytes, i).unwrap_or(bytes.len());
        } else if c == b'\'' {
            if next == Some(b'\\') {
                i = find(bytes, i + 2, b"'").map_or(bytes.len(), |end| end + 1);
            } else if bytes.get(i + 2) == Some(&b'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else {
            match c {
                b'(' | b'[' => parens += 1,
                b')' | b']' => parens -= 1,
                b';' if parens == 0 && braces == 0 => return None,
                b'{' if parens == 0 => {
                    braces += 1;
                    entered = true;
                }
                b'}' if parens == 0 => {
                    braces -= 1;
                    if entered && braces == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
    None
}

fn raw_string_end(bytes: &[u8], at: usize) -> Option<usize> {
    let mut j = at + 1;
    let mut hashes = 0;
    while bytes.get(j) == Some(&b'#') {
        hashes += 1;
        j += 1;
    }
    if bytes.get(j) != Some(&b'"') {
        return None;
    }
    let mut closing = vec![b'"'];
    closing.extend(std::iter::repeat_n(b'#', hashes));
    Some(find(bytes, j + 1, &closing).map_or(bytes.len(), |end| end + closing.len()))
}

fn without_test_items(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut blanked = bytes.to_vec();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let next = offset + line.len();
        if line.trim() == "#[cfg(test)]" {
            if let Some(end) = item_end(bytes, next) {
                if let Some(span) = blanked.get_mut(offset..end) {
                    for byte in span {
                        if *byte != b'\n' {
                            *byte = b' ';
                        }
                    }
                }
            }
        }
        offset = next;
    }
    String::from_utf8_lossy(&blanked).into_owned()
}

fn deferred_begins(text: &str) -> Vec<usize> {
    let production = without_test_items(text);
    let production = production.as_str();
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
