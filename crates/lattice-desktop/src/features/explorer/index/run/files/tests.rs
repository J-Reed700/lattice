use super::*;

fn temp_file(bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    std::fs::write(&path, bytes).unwrap();
    (dir, path)
}

/// A binary file is known from its first bytes: recorded with no chunks and
/// identified by its head and length, so a large video is never loaded or
/// hashed whole. Unchanged, the next run only touches it.
#[test]
fn a_binary_file_is_decided_from_its_head() {
    let mut bytes = vec![0u8, 1, 2, 3];
    bytes.resize(3 * fs::LINE_PIECE_BYTES, 7);
    let (_dir, path) = temp_file(&bytes);

    let Prepared::Replace { hash, chunks } = prepare(&path, None) else {
        panic!("a binary file is recorded");
    };
    assert!(hash.starts_with("binary:"), "{hash}");
    assert!(chunks.is_empty());
    assert!(matches!(prepare(&path, Some(&hash)), Prepared::Touch));
}

/// A large text file streams through the chunker with the same passages the
/// whole-text chunker makes, and the same content hash as hashing it whole.
#[test]
fn a_streamed_text_file_chunks_like_the_whole_text() {
    let text: String = (0..900)
        .map(|n| match n % 40 {
            0 => format!("fn item_{n}() {{\n"),
            39 => "}\n\n".to_string(),
            _ => format!("    let value_{n} = {n} * 2; // a line of code\n"),
        })
        .collect();
    let (_dir, path) = temp_file(text.as_bytes());

    let Prepared::Replace { hash, chunks } = prepare(&path, None) else {
        panic!("a text file is recorded");
    };
    assert_eq!(hash, format!("{:x}", Sha256::digest(text.as_bytes())));
    assert_eq!(chunks, chunker::chunk_text(&text));
    assert!(matches!(prepare(&path, Some(&hash)), Prepared::Touch));
}

/// A line far longer than one read, with no newline in it, is indexed in
/// parts that all point at that line; nothing of it or after it is lost.
#[test]
fn a_line_longer_than_a_read_is_indexed_in_parts() {
    let long_line = "word ".repeat(fs::LINE_PIECE_BYTES / 2);
    // Enough ordinary lines that the file is not taken for minified.
    let ordinary: String = (0..1_000).map(|n| format!("ordinary line {n}\n")).collect();
    let text = format!("{long_line}\n{ordinary}after the long line\n");
    let (_dir, path) = temp_file(text.as_bytes());

    let Prepared::Replace { chunks, .. } = prepare(&path, None) else {
        panic!("a text file is recorded");
    };
    let on_line_one: String = chunks
        .iter()
        .filter(|chunk| chunk.start_line == 1 && chunk.end_line == 1)
        .map(|chunk| chunk.body.as_str())
        .collect();
    assert_eq!(on_line_one, long_line);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.body.len() <= chunker::MAX_CHUNK_CHARS));
    let last = chunks.last().unwrap();
    assert!(last.body.ends_with("after the long line"), "{last:?}");
    assert_eq!(last.end_line, 1_002);
}

/// Past the bound, the walk keeps the files already indexed and then the
/// rest by path, whatever order the disk lists them in.
#[test]
fn a_walk_past_the_bound_keeps_indexed_files_then_the_first_by_path() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["e.txt", "a.txt", "d.txt", "b.txt", "c.txt"] {
        std::fs::write(dir.path().join(name), "text").unwrap();
    }
    let scope = Scope::open(dir.path().to_str().unwrap()).unwrap();
    let indexed: HashSet<String> = ["e.txt".to_string()].into();

    let Walked::Files { files, skipped } =
        walk(&scope, "", 3, &indexed, &CancellationToken::new(), None).unwrap()
    else {
        panic!("not cancelled");
    };
    let kept: Vec<&str> = files.iter().map(|file| file.relative.as_str()).collect();
    assert_eq!(kept, ["e.txt", "a.txt", "b.txt"]);
    assert_eq!(skipped, 2);
}
