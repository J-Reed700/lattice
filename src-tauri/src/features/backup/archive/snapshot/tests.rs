//! Tests for snapshotting the database and packing and extracting payloads.
use super::extract::{checked_entry_name, classify_entry};
use super::payload::ExactReader;
use super::*;
use crate::features::backup::archive::format::{
    Manifest, DB_ENTRY, FORMAT_VERSION, MANIFEST_ENTRY,
};
use std::io;
use std::io::Cursor;
use tempfile::tempdir;

type TestBuilder = tar::Builder<zstd::Encoder<'static, Vec<u8>>>;

async fn migrated_pool(db_path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Delete);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn read_only_pool(db_path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true)
        .journal_mode(SqliteJournalMode::Delete);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap()
}

async fn count(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

fn write_file(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

// ---------------------------------------------------------------- snapshot

#[tokio::test]
async fn snapshot_clears_derivable_tables_and_keeps_everything_else() {
    let dir = tempdir().unwrap();
    let live = dir.path().join("lattice.db");
    let pool = migrated_pool(&live).await;

    sqlx::query(
        "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) \
             VALUES ('doc-1', '/tmp/a.md', 'a.md', 12, '2026-09-16T00:00:00Z', 'abc')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO text_chunks (id, document_id, content, chunk_index) \
             VALUES ('chunk-1', 'doc-1', 'hello world', 0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO tags (id, name) VALUES ('tag-1', 'research')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension) \
             VALUES ('emb-1', 'chunk-1', ?, 'bge-small', 4)",
    )
    .bind(vec![0u8, 1, 2, 3])
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chat_starter_cache (fingerprint, starters_json, created_at) \
             VALUES ('fp', '[]', '2026-09-16T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let dest = dir.path().join("snapshot.db");
    let report = snapshot_database(&pool, &dest).await.unwrap();
    pool.close().await;

    assert_eq!(report.path, dest);
    assert!(report.bytes > 0, "snapshot should not be empty");
    assert!(report.migration_version.is_some());
    assert!(report
        .excluded_tables
        .iter()
        .any(|t| t == "text_embeddings"));
    assert!(report
        .excluded_tables
        .iter()
        .any(|t| t == "chat_starter_cache"));
    for suffix in DB_SIDECAR_SUFFIXES {
        assert!(
            !sidecar_path(&dest, suffix).exists(),
            "snapshot left a {suffix} sidecar behind"
        );
    }

    let snap = read_only_pool(&dest).await;
    assert_eq!(count(&snap, "SELECT COUNT(*) FROM documents").await, 1);
    assert_eq!(count(&snap, "SELECT COUNT(*) FROM tags").await, 1);
    assert_eq!(count(&snap, "SELECT COUNT(*) FROM text_chunks").await, 1);
    assert_eq!(
        count(&snap, "SELECT COUNT(*) FROM text_embeddings").await,
        0
    );
    assert_eq!(
        count(&snap, "SELECT COUNT(*) FROM chat_starter_cache").await,
        0
    );
    // FTS content rides along verbatim: the triggers that fill it only
    // fire on writes to `text_chunks`, which a restore never replays.
    assert_eq!(count(&snap, "SELECT COUNT(*) FROM chunks_fts").await, 1);
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&snap)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
    snap.close().await;

    let info = validate_snapshot(&dest).await.unwrap();
    assert_eq!(info.document_count, 1);
    assert_eq!(info.conversation_count, 0);
    assert_eq!(info.migration_version, report.migration_version);
    assert!(info.table_count > 10, "schema should survive the snapshot");
}

#[tokio::test]
async fn snapshot_refuses_an_existing_destination_and_unsafe_paths() {
    let dir = tempdir().unwrap();
    let live = dir.path().join("lattice.db");
    let pool = migrated_pool(&live).await;

    let taken = dir.path().join("taken.db");
    std::fs::write(&taken, b"x").unwrap();
    assert!(matches!(
        snapshot_database(&pool, &taken).await,
        Err(ArchiveError::Other(_))
    ));

    let nasty = dir.path().join("evil'; DROP TABLE documents; --.db");
    assert!(matches!(
        snapshot_database(&pool, &nasty).await,
        Err(ArchiveError::Other(_))
    ));
    pool.close().await;
}

#[test]
fn detects_external_content_fts_tables() {
    assert_eq!(
        fts_external_content_table("CREATE VIRTUAL TABLE t USING fts5(c, content='documents')")
            .as_deref(),
        Some("documents")
    );
    assert_eq!(
        fts_external_content_table("CREATE VIRTUAL TABLE t USING fts5(c, content=\"docs\")")
            .as_deref(),
        Some("docs")
    );
    // The schema this app actually ships: no `content=` option at all.
    assert_eq!(
        fts_external_content_table(
            "CREATE VIRTUAL TABLE chunks_fts USING fts5(chunk_id UNINDEXED, content)"
        ),
        None
    );
}

#[tokio::test]
async fn referenced_blobs_come_from_the_snapshot_not_the_live_database() {
    let dir = tempdir().unwrap();
    let live = dir.path().join("lattice.db");
    let pool = migrated_pool(&live).await;

    // Two documents on one blob, one on another (spelled in upper case,
    // the way a hand-edited row might be), and one whose checksum is not
    // a digest at all.
    for (id, checksum) in [
        ("doc-1", SHA.to_string()),
        ("doc-2", SHA.to_string()),
        ("doc-3", ORPHAN_SHA.to_uppercase()),
        ("doc-4", "not-a-digest".to_string()),
    ] {
        sqlx::query(
            "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) \
                 VALUES (?, ?, ?, 12, '2026-09-16T00:00:00Z', ?)",
        )
        .bind(id)
        .bind(format!("/tmp/{id}.md"))
        .bind(format!("{id}.md"))
        .bind(&checksum)
        .execute(&pool)
        .await
        .unwrap();
    }

    let dest = dir.path().join("snapshot.db");
    snapshot_database(&pool, &dest).await.unwrap();

    // A document deleted after the snapshot was taken is not in the
    // snapshot, so its blob is not referenced by the archive either.
    sqlx::query("DELETE FROM documents WHERE id = 'doc-3'")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let hashes = referenced_blob_hashes(&dest).await.unwrap().unwrap();
    assert_eq!(
        hashes,
        HashSet::from([SHA.to_string(), ORPHAN_SHA.to_string()]),
        "distinct, lower-cased, and only things shaped like a digest"
    );
    for suffix in DB_SIDECAR_SUFFIXES {
        assert!(
            !sidecar_path(&dest, suffix).exists(),
            "reading the referenced set left a {suffix} sidecar behind"
        );
    }
}

#[tokio::test]
async fn a_database_that_cannot_answer_packs_everything() {
    let dir = tempdir().unwrap();

    // No documents table at all.
    let other = dir.path().join("other.db");
    let options = SqliteConnectOptions::new()
        .filename(&other)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Delete);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE notes (id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    assert!(referenced_blob_hashes(&other).await.unwrap().is_none());

    // A documents table from somewhere else, with no checksum column.
    sqlx::query("CREATE TABLE documents (id TEXT PRIMARY KEY, title TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    assert!(referenced_blob_hashes(&other).await.unwrap().is_none());

    // Not a database at all.
    let garbage = dir.path().join("garbage.db");
    std::fs::write(&garbage, b"nope").unwrap();
    assert!(referenced_blob_hashes(&garbage).await.is_err());
}

// ----------------------------------------------------------------- payload

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
    files_root: PathBuf,
    vault_root: PathBuf,
    settings: PathBuf,
    inputs: ArchiveInputs,
}

const SHA: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
/// A blob no document references: an import that died before it
/// committed, or a document deleted before the sweep ran.
const ORPHAN_SHA: &str = "5feceb66ffc86f38d952786c6d696c79c2dbc239dd4e91b46729d73a27fb57e9";
const PDF_BYTES: &[u8] = b"%PDF-1.4 pretend this is a pdf";
const ORPHAN_BYTES: &[u8] = b"%PDF-1.4 nothing points at this";
const STRAY_BYTES: &[u8] = b"not a blob";
const NOTE_NAME: &str = "\u{fc}n\u{ef}code note.md";
const NOTE_BYTES: &[u8] = "# h\u{e9}llo w\u{f6}rld\n".as_bytes();
const SETTINGS_BYTES: &[u8] = br#"{"theme":"dark"}"#;
const DB_BYTES: &[u8] = b"SQLite format 3\0not really, but bytes are bytes";

fn fixture() -> Fixture {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();

    let db = root.join("snapshot.db");
    write_file(&db, DB_BYTES);

    let files_root = root.join("files");
    write_file(&files_root.join(SHA).join("name.pdf"), PDF_BYTES);

    let vault_root = root.join("vault");
    write_file(
        &vault_root.join("notes").join("sub").join(NOTE_NAME),
        NOTE_BYTES,
    );
    write_file(
        &vault_root.join("excluded").join("secret.md"),
        b"must not ship",
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        vault_root.join("notes").join("sub").join(NOTE_NAME),
        vault_root.join("shortcut.md"),
    )
    .unwrap();

    let settings = root.join("settings.json");
    write_file(&settings, SETTINGS_BYTES);

    let inputs = ArchiveInputs {
        db_snapshot: db.clone(),
        migration_version: Some(20_260_916_010_000),
        files_root: Some(files_root.clone()),
        // The planner tests say explicitly which blobs are referenced;
        // `None` here keeps the base fixture packing what it always did.
        referenced_blob_hashes: None,
        vault_root: Some(vault_root.clone()),
        web_archive_root: None,
        settings_file: Some(settings.clone()),
        exclude: vec![vault_root.join("excluded")],
        app_version: "1.2.3".to_string(),
        created_at: "2026-09-16T12:00:00Z".to_string(),
    };

    Fixture {
        _dir: dir,
        root,
        db,
        files_root,
        vault_root,
        settings,
        inputs,
    }
}

#[test]
fn payload_round_trips_and_honours_excludes_and_symlinks() {
    let fx = fixture();
    let mut events: Vec<ProgressEvent> = Vec::new();
    let (manifest, bytes) =
        write_payload(Vec::new(), &fx.inputs, &mut |event| events.push(event)).unwrap();

    let expected_bytes =
        (DB_BYTES.len() + PDF_BYTES.len() + NOTE_BYTES.len() + SETTINGS_BYTES.len()) as u64;
    assert_eq!(manifest.version, FORMAT_VERSION);
    assert_eq!(manifest.db_entry, DB_ENTRY);
    assert_eq!(manifest.files_count, 1);
    assert_eq!(
        manifest.vault_file_count, 1,
        "symlink and excluded subtree must not count"
    );
    assert!(manifest.settings_included);
    assert_eq!(manifest.total_plaintext_bytes, expected_bytes);
    assert_eq!(manifest.migration_version, Some(20_260_916_010_000));
    assert_eq!(manifest.excluded_tables.len(), EXCLUDED_TABLES.len());
    assert!(manifest.files_root.is_some());
    assert!(manifest.vault_root.is_some());

    assert!(matches!(events.first(), Some(ProgressEvent::Scanning)));
    assert!(matches!(events.last(), Some(ProgressEvent::Done)));
    let entries: Vec<&ProgressEvent> = events
        .iter()
        .filter(|e| matches!(e, ProgressEvent::Entry { .. }))
        .collect();
    assert_eq!(entries.len(), 4, "db + 1 file + 1 vault note + settings");
    if let Some(ProgressEvent::Entry {
        bytes_done,
        bytes_total,
        ..
    }) = entries.last()
    {
        assert_eq!(*bytes_done, expected_bytes);
        assert_eq!(*bytes_total, expected_bytes);
    } else {
        panic!("expected a final Entry event");
    }

    // read_manifest sees the same manifest without unpacking anything.
    let from_stream = read_manifest(Cursor::new(bytes.clone())).unwrap();
    assert_eq!(from_stream, manifest);

    let dest = fx.root.join("extracted");
    let extracted = extract_payload(Cursor::new(bytes), &dest, &mut |_| {}).unwrap();
    assert_eq!(extracted.manifest, manifest);
    assert_eq!(std::fs::read(&extracted.db_path).unwrap(), DB_BYTES);

    let files_dir = extracted.files_dir.clone().unwrap();
    assert_eq!(
        std::fs::read(files_dir.join(SHA).join("name.pdf")).unwrap(),
        PDF_BYTES
    );
    let vault_dir = extracted.vault_dir.clone().unwrap();
    assert_eq!(
        std::fs::read(vault_dir.join("notes").join("sub").join(NOTE_NAME)).unwrap(),
        NOTE_BYTES
    );
    assert!(
        !vault_dir.join("excluded").exists(),
        "excluded subtree leaked"
    );
    assert!(
        !vault_dir.join("shortcut.md").exists(),
        "symlink was packed"
    );
    assert_eq!(
        std::fs::read(extracted.settings_file.clone().unwrap()).unwrap(),
        SETTINGS_BYTES
    );

    // Sources untouched.
    assert!(fx.db.exists() && fx.files_root.exists() && fx.settings.exists());
    assert!(fx.vault_root.join("excluded").join("secret.md").exists());
}

/// Web documents point their `file_path` into the web archive, and the
/// reader opens the page snapshot from there. A backup that left the
/// folder behind would restore documents whose originals are gone.
#[test]
fn payload_carries_saved_web_articles() {
    let fx = fixture();
    let web_archive_root = fx.root.join("web-archive");
    let article_dir = web_archive_root
        .join("example.com")
        .join("an-article-abc123");
    write_file(&article_dir.join("article.md"), b"# An article");
    write_file(&article_dir.join("page.html"), b"<html>snapshot</html>");
    let inputs = ArchiveInputs {
        web_archive_root: Some(web_archive_root),
        ..fx.inputs.clone()
    };

    let (manifest, bytes) = write_payload(Vec::new(), &inputs, &mut |_| {}).unwrap();
    let (base_manifest, _) = write_payload(Vec::new(), &fx.inputs, &mut |_| {}).unwrap();
    assert_eq!(
        manifest.total_plaintext_bytes,
        base_manifest.total_plaintext_bytes
            + (b"# An article".len() + b"<html>snapshot</html>".len()) as u64
    );

    let dest = fx.root.join("extracted-web");
    let extracted = extract_payload(Cursor::new(bytes), &dest, &mut |_| {}).unwrap();
    let web_dir = extracted.web_archive_dir.clone().unwrap();
    let restored_article = web_dir.join("example.com").join("an-article-abc123");
    assert_eq!(
        std::fs::read(restored_article.join("article.md")).unwrap(),
        b"# An article"
    );
    assert_eq!(
        std::fs::read(restored_article.join("page.html")).unwrap(),
        b"<html>snapshot</html>"
    );
}

#[test]
fn payload_without_optional_inputs_still_round_trips() {
    let fx = fixture();
    let inputs = ArchiveInputs {
        db_snapshot: fx.db.clone(),
        migration_version: None,
        files_root: None,
        referenced_blob_hashes: None,
        vault_root: None,
        web_archive_root: None,
        settings_file: None,
        exclude: Vec::new(),
        app_version: "1.2.3".to_string(),
        created_at: "2026-09-16T12:00:00Z".to_string(),
    };
    let (manifest, bytes) = write_payload(Vec::new(), &inputs, &mut |_| {}).unwrap();
    assert_eq!(manifest.files_count, 0);
    assert_eq!(manifest.vault_file_count, 0);
    assert!(!manifest.settings_included);

    let dest = fx.root.join("extracted-minimal");
    let extracted = extract_payload(Cursor::new(bytes), &dest, &mut |_| {}).unwrap();
    assert!(extracted.files_dir.is_none());
    assert!(extracted.vault_dir.is_none());
    assert!(extracted.web_archive_dir.is_none());
    assert!(extracted.settings_file.is_none());
    assert_eq!(std::fs::read(&extracted.db_path).unwrap(), DB_BYTES);
}

// --------------------------------------------------- referenced blobs only

/// Everything a real library root collects that is not the user's data:
/// an orphan blob, a directory that is not named after a hash at all, and
/// a file the OS dropped in the root.
fn litter_the_library(files_root: &Path) {
    write_file(&files_root.join(ORPHAN_SHA).join("ghost.pdf"), ORPHAN_BYTES);
    write_file(
        &files_root.join("not-a-hash").join("thing.pdf"),
        STRAY_BYTES,
    );
    write_file(&files_root.join(".DS_Store"), STRAY_BYTES);
}

fn expected_referenced_bytes() -> u64 {
    (DB_BYTES.len() + PDF_BYTES.len() + NOTE_BYTES.len() + SETTINGS_BYTES.len()) as u64
}

#[test]
fn payload_packs_only_the_blobs_the_database_references() {
    let fx = fixture();
    litter_the_library(&fx.files_root);

    let inputs = ArchiveInputs {
        referenced_blob_hashes: Some(HashSet::from([SHA.to_string()])),
        ..fx.inputs.clone()
    };
    let (manifest, bytes) = write_payload(Vec::new(), &inputs, &mut |_| {}).unwrap();

    assert_eq!(
        manifest.files_count, 1,
        "only the referenced blob is packed"
    );
    assert_eq!(
        manifest.total_plaintext_bytes,
        expected_referenced_bytes(),
        "skipped blobs must not be counted as packed bytes"
    );

    let dest = fx.root.join("extracted-referenced");
    let extracted = extract_payload(Cursor::new(bytes), &dest, &mut |_| {}).unwrap();
    let files_dir = extracted.files_dir.clone().unwrap();
    assert_eq!(
        std::fs::read(files_dir.join(SHA).join("name.pdf")).unwrap(),
        PDF_BYTES
    );
    assert!(
        !files_dir.join(ORPHAN_SHA).exists(),
        "an orphan blob was archived"
    );
    assert!(
        !files_dir.join("not-a-hash").exists(),
        "a directory that is not a blob was archived"
    );
    assert!(
        !files_dir.join(".DS_Store").exists(),
        "a stray file was archived"
    );

    // Backing up never deletes: the library is exactly as it was.
    assert!(fx.files_root.join(ORPHAN_SHA).join("ghost.pdf").exists());
    assert!(fx.files_root.join(".DS_Store").exists());
}

#[test]
fn a_database_that_references_nothing_packs_no_blobs() {
    let fx = fixture();
    litter_the_library(&fx.files_root);

    let inputs = ArchiveInputs {
        referenced_blob_hashes: Some(HashSet::new()),
        ..fx.inputs.clone()
    };
    let (manifest, bytes) = write_payload(Vec::new(), &inputs, &mut |_| {}).unwrap();

    assert_eq!(manifest.files_count, 0);
    assert!(
        manifest.files_root.is_some(),
        "the library root is still recorded even when nothing came from it"
    );

    let dest = fx.root.join("extracted-none");
    let extracted = extract_payload(Cursor::new(bytes), &dest, &mut |_| {}).unwrap();
    assert!(extracted.files_dir.is_none(), "no files entries at all");
}

#[test]
fn without_a_referenced_set_every_blob_directory_is_packed() {
    let fx = fixture();
    litter_the_library(&fx.files_root);

    // `None` is the fallback for a snapshot that cannot be asked, and
    // what the tests with no database use: blobs are packed unfiltered,
    // but things that are not blobs still are not.
    let (manifest, bytes) = write_payload(Vec::new(), &fx.inputs, &mut |_| {}).unwrap();
    assert_eq!(manifest.files_count, 2);

    let dest = fx.root.join("extracted-unfiltered");
    let extracted = extract_payload(Cursor::new(bytes), &dest, &mut |_| {}).unwrap();
    let files_dir = extracted.files_dir.clone().unwrap();
    assert!(files_dir.join(SHA).join("name.pdf").exists());
    assert!(files_dir.join(ORPHAN_SHA).join("ghost.pdf").exists());
    assert!(!files_dir.join("not-a-hash").exists());
    assert!(!files_dir.join(".DS_Store").exists());
}

#[test]
fn only_64_lowercase_hex_names_are_blob_directories() {
    assert!(is_blob_hash(SHA));
    assert!(is_blob_hash(ORPHAN_SHA));
    assert!(
        !is_blob_hash(&SHA.to_uppercase()),
        "the library writes lower case"
    );
    assert!(!is_blob_hash("deadbeef"), "too short");
    assert!(!is_blob_hash(&format!("{SHA}0")), "too long");
    assert!(!is_blob_hash(&"g".repeat(64)), "not hex");
    assert!(!is_blob_hash(""));
}

#[test]
fn write_payload_fails_without_a_snapshot() {
    let dir = tempdir().unwrap();
    let inputs = ArchiveInputs {
        db_snapshot: dir.path().join("nope.db"),
        ..ArchiveInputs::default()
    };
    assert!(matches!(
        write_payload(Vec::new(), &inputs, &mut |_| {}),
        Err(ArchiveError::Other(_))
    ));
}

// ---------------------------------------------------------- hostile tarballs

fn manifest_fixture() -> Manifest {
    Manifest {
        version: FORMAT_VERSION,
        created_at: "2026-09-16T12:00:00Z".to_string(),
        app_version: "1.2.3".to_string(),
        db_entry: DB_ENTRY.to_string(),
        excluded_tables: Vec::new(),
        migration_version: None,
        files_root: None,
        files_count: 0,
        vault_root: None,
        vault_file_count: 0,
        settings_included: false,
        total_plaintext_bytes: 0,
    }
}

fn append_manifest(builder: &mut TestBuilder) {
    let json = serde_json::to_vec(&manifest_fixture()).unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(json.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    builder
        .append_data(&mut header, MANIFEST_ENTRY, json.as_slice())
        .unwrap();
}

fn append_db(builder: &mut TestBuilder) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(DB_BYTES.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    builder
        .append_data(&mut header, DB_ENTRY, DB_BYTES)
        .unwrap();
}

/// tar-rs refuses to *write* `..` or absolute names, so those headers are
/// stamped by hand to model what a hostile archive would contain.
fn append_raw_name(builder: &mut TestBuilder, name: &[u8], data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    {
        let raw = header.as_mut_bytes();
        raw[..name.len()].copy_from_slice(name);
    }
    header.set_cksum();
    builder.append(&header, data).unwrap();
}

fn pack<F: FnOnce(&mut TestBuilder)>(with_manifest_first: bool, fill: F) -> Vec<u8> {
    let encoder = zstd::Encoder::new(Vec::new(), 1).unwrap();
    let mut builder = tar::Builder::new(encoder);
    if with_manifest_first {
        append_manifest(&mut builder);
    }
    fill(&mut builder);
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap()
}

fn extract_into_fresh(
    bytes: Vec<u8>,
    label: &str,
) -> (tempfile::TempDir, Result<ExtractedPayload, ArchiveError>) {
    let dir = tempdir().unwrap();
    let dest = dir.path().join(label);
    let result = extract_payload(Cursor::new(bytes), &dest, &mut |_| {});
    (dir, result)
}

#[test]
fn extract_rejects_parent_dir_entries() {
    let bytes = pack(true, |b| {
        append_db(b);
        append_raw_name(b, b"../evil", b"pwned");
    });
    let (_dir, result) = extract_into_fresh(bytes, "dest");
    assert!(
        matches!(result, Err(ArchiveError::Corrupt(_))),
        "{result:?}"
    );
}

#[test]
fn extract_rejects_absolute_entries() {
    let bytes = pack(true, |b| {
        append_db(b);
        append_raw_name(b, b"/etc/evil", b"pwned");
    });
    let (_dir, result) = extract_into_fresh(bytes, "dest");
    assert!(
        matches!(result, Err(ArchiveError::Corrupt(_))),
        "{result:?}"
    );
}

#[test]
fn extract_rejects_symlink_entries() {
    let bytes = pack(true, |b| {
        append_db(b);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_mtime(0);
        b.append_link(&mut header, "vault/shortcut.md", "/etc/passwd")
            .unwrap();
    });
    let (_dir, result) = extract_into_fresh(bytes, "dest");
    assert!(
        matches!(result, Err(ArchiveError::Corrupt(_))),
        "{result:?}"
    );
}

#[test]
fn extract_rejects_entries_outside_the_known_prefixes() {
    let bytes = pack(true, |b| {
        append_db(b);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(3);
        header.set_mode(0o644);
        header.set_mtime(0);
        b.append_data(&mut header, "models/llm.gguf", &b"abc"[..])
            .unwrap();
    });
    let (_dir, result) = extract_into_fresh(bytes, "dest");
    assert!(
        matches!(result, Err(ArchiveError::Corrupt(_))),
        "{result:?}"
    );
}

#[test]
fn extract_requires_the_manifest_first() {
    let bytes = pack(false, |b| {
        append_db(b);
        append_manifest(b);
    });
    let (_dir, result) = extract_into_fresh(bytes, "dest");
    assert!(
        matches!(result, Err(ArchiveError::Corrupt(_))),
        "{result:?}"
    );
    assert!(matches!(
        read_manifest(Cursor::new(pack(false, append_db))),
        Err(ArchiveError::Corrupt(_))
    ));
}

#[test]
fn extract_requires_a_database_entry() {
    let bytes = pack(true, |_| {});
    let (_dir, result) = extract_into_fresh(bytes, "dest");
    assert!(
        matches!(result, Err(ArchiveError::Corrupt(_))),
        "{result:?}"
    );
}

#[test]
fn extract_refuses_a_non_empty_destination() {
    let fx = fixture();
    let (_manifest, bytes) = write_payload(Vec::new(), &fx.inputs, &mut |_| {}).unwrap();
    let dest = fx.root.join("occupied");
    write_file(&dest.join("already-here.txt"), b"hi");
    let result = extract_payload(Cursor::new(bytes), &dest, &mut |_| {});
    assert!(matches!(result, Err(ArchiveError::Other(_))), "{result:?}");
}

#[test]
fn entry_names_are_validated() {
    assert!(checked_entry_name(Path::new("vault/a/b.md")).is_ok());
    assert!(checked_entry_name(Path::new("../x")).is_err());
    assert!(checked_entry_name(Path::new("/x")).is_err());
    assert!(checked_entry_name(Path::new("")).is_err());
    assert!(classify_entry("manifest.json").is_ok());
    assert!(classify_entry("db/lattice.db").is_ok());
    assert!(classify_entry("files/aa/b.pdf").is_ok());
    assert!(classify_entry("vault").is_ok());
    assert!(classify_entry("web-archive").is_ok());
    assert!(classify_entry("web-archive/example.com/a/page.html").is_ok());
    assert!(classify_entry("settings.json").is_ok());
    assert!(classify_entry("db/other.db").is_err());
    assert!(classify_entry("elsewhere/x").is_err());
}

// -------------------------------------------------------- validate_snapshot

#[tokio::test]
async fn validate_snapshot_rejects_non_databases_and_truncated_files() {
    let dir = tempdir().unwrap();

    let missing = dir.path().join("absent.db");
    assert!(matches!(
        validate_snapshot(&missing).await,
        Err(ArchiveError::Corrupt(_))
    ));

    let garbage = dir.path().join("garbage.db");
    std::fs::write(&garbage, b"this is definitely not a sqlite database at all").unwrap();
    assert!(validate_snapshot(&garbage).await.is_err());

    let live = dir.path().join("lattice.db");
    let pool = migrated_pool(&live).await;
    let good = dir.path().join("good.db");
    snapshot_database(&pool, &good).await.unwrap();
    pool.close().await;
    assert!(validate_snapshot(&good).await.is_ok());

    let raw = std::fs::read(&good).unwrap();
    assert!(raw.len() > 8192, "schema snapshot should span many pages");
    let truncated = dir.path().join("truncated.db");
    std::fs::write(&truncated, &raw[..4096]).unwrap();
    assert!(
        validate_snapshot(&truncated).await.is_err(),
        "a truncated database must not validate"
    );
}

#[tokio::test]
async fn a_snapshot_reports_the_memory_layout_it_stores_so_restore_can_refuse_it() {
    let dir = tempdir().unwrap();
    let live = dir.path().join("lattice.db");
    let pool = migrated_pool(&live).await;

    // A conversation with memory state written by *this* build.
    sqlx::query("INSERT INTO conversations (id, title, model_name) VALUES ('c1', 'T', 'm')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO conversation_memory_state (conversation_id, schema_version) \
             VALUES ('c1', ?)",
    )
    .bind(lattice_core::domain::conversation::memory::MEMORY_SCHEMA_VERSION)
    .execute(&pool)
    .await
    .unwrap();

    let dest = dir.path().join("current.db");
    snapshot_database(&pool, &dest).await.unwrap();
    let info = validate_snapshot(&dest).await.unwrap();
    assert!(info.memory_tables_present);
    assert_eq!(
        info.memory_schema_version,
        Some(lattice_core::domain::conversation::memory::MEMORY_SCHEMA_VERSION)
    );

    // Now a snapshot a *newer* build would have written. The migration
    // version is identical — the schema is one squashed migration edited in
    // place — so this is the only signal that distinguishes the two layouts.
    sqlx::query("UPDATE conversation_memory_state SET schema_version = 99")
        .execute(&pool)
        .await
        .unwrap();
    let future = dir.path().join("future.db");
    snapshot_database(&pool, &future).await.unwrap();
    pool.close().await;

    let future_info = validate_snapshot(&future).await.unwrap();
    assert_eq!(future_info.memory_schema_version, Some(99));
    assert_eq!(
        future_info.migration_version, info.migration_version,
        "the migration number cannot tell these two layouts apart, which is why \
             memory_schema_version exists"
    );
}

#[tokio::test]
async fn a_backup_without_a_memory_ledger_is_restorable_rather_than_rejected() {
    let dir = tempdir().unwrap();
    let live = dir.path().join("lattice.db");
    let pool = migrated_pool(&live).await;
    // No memory rows at all: the state every conversation is in before its
    // first compaction, and the state every pre-feature backup is in.
    let dest = dir.path().join("empty-memory.db");
    snapshot_database(&pool, &dest).await.unwrap();
    pool.close().await;

    let info = validate_snapshot(&dest).await.unwrap();
    assert!(
        info.memory_tables_present,
        "the tables exist even when empty"
    );
    assert_eq!(
        info.memory_schema_version, None,
        "an absent ledger must not read as an unsupported one"
    );
}

#[test]
fn exact_reader_pads_a_shrinking_source() {
    let mut reader = ExactReader::new(Cursor::new(b"abc".to_vec()), 6);
    let mut out = Vec::new();
    io::copy(&mut reader, &mut out).unwrap();
    assert_eq!(out, b"abc\0\0\0");
    assert_eq!(reader.padded, 3);

    let mut reader = ExactReader::new(Cursor::new(b"abcdef".to_vec()), 3);
    let mut out = Vec::new();
    io::copy(&mut reader, &mut out).unwrap();
    assert_eq!(out, b"abc");
    assert_eq!(reader.padded, 0);
}
