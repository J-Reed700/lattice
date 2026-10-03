use super::chunker::{self, chunk_text, MAX_CHUNK_CHARS, MAX_CHUNK_LINES, MAX_LINE_CHARS};
use super::dto::{FolderIndexState, FolderIndexStatusDto};
use super::manager::{EmbedderSource, FolderIndexManager, ManagerConfig};
use super::run::{Indexer, Limits, Outcome, StatusCell};
use super::search::{fuse, match_expression, FolderHit, RRF_K};
use super::store::{self, FolderStore};
use super::tool;
use crate::application::ports::vector_search_port::VectorSearchPort;
use crate::application::ports::EmbeddingPort;
use crate::features::explorer::dto::ExplorerFocusDto;
use crate::features::explorer::prompt::{self, ExplorerTurn, FolderPassages};
use crate::features::explorer::scope::Scope;
use crate::features::explorer::tools;
use crate::features::search::engine::vector_search::USearchVectorIndex;
use crate::shared::Result;
use async_trait::async_trait;
use parking_lot::Mutex;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const DIM: usize = 16;

/// Bag-of-words vectors: texts sharing words point the same way, which is
/// all "by meaning" needs to mean in a test. Counts every passage it embeds.
struct FakeEmbedder {
    identity: String,
    embedded: Mutex<Vec<String>>,
    batches: AtomicUsize,
    delay_ms: AtomicU64,
}

impl FakeEmbedder {
    fn new(identity: &str) -> Arc<Self> {
        Arc::new(Self {
            identity: identity.to_string(),
            embedded: Mutex::new(Vec::new()),
            batches: AtomicUsize::new(0),
            delay_ms: AtomicU64::new(0),
        })
    }

    fn embedded(&self) -> Vec<String> {
        self.embedded.lock().clone()
    }

    fn forget_calls(&self) {
        self.embedded.lock().clear();
        self.batches.store(0, Ordering::SeqCst);
    }

    fn vector(text: &str) -> Vec<f32> {
        let mut vector = [0.01f32; DIM];
        for word in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.len() > 2)
        {
            let bucket = word.to_lowercase().bytes().fold(7usize, |hash, byte| {
                hash.wrapping_mul(31).wrapping_add(byte as usize)
            }) % DIM;
            vector[bucket] += 1.0;
        }
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        vector.iter().map(|x| x / norm).collect()
    }
}

#[async_trait]
impl EmbeddingPort for FakeEmbedder {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        Ok(Self::vector(text))
    }
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let delay = self.delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        self.batches.fetch_add(1, Ordering::SeqCst);
        self.embedded.lock().extend(texts.iter().cloned());
        Ok(texts.iter().map(|text| Self::vector(text)).collect())
    }
    fn dimension(&self) -> usize {
        DIM
    }
    fn model_identity(&self) -> String {
        self.identity.clone()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

struct Source(Option<Arc<FakeEmbedder>>);

#[async_trait]
impl EmbedderSource for Source {
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>> {
        self.0
            .clone()
            .map(|embedder| embedder as Arc<dyn EmbeddingPort>)
    }
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A data directory with a fake home inside it, so the refusal rules can be
/// tested without touching the real one.
struct Rig {
    _temp: tempfile::TempDir,
    base: PathBuf,
    home: PathBuf,
    events: Arc<Mutex<Vec<FolderIndexStatusDto>>>,
}

impl Rig {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(temp.path()).unwrap();
        let home = canonical.join("home").join("me");
        std::fs::create_dir_all(&home).unwrap();
        Self {
            base: canonical.join("data").join("folder-index"),
            home,
            _temp: temp,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn config(&self) -> ManagerConfig {
        ManagerConfig {
            base_dir: self.base.clone(),
            home: Some(self.home.clone()),
            limits: Limits {
                max_files: 50,
                batch_size: 4,
                save_every: 8,
            },
            max_indexes: 8,
            debounce: Duration::from_millis(100),
        }
    }

    fn manager_with(
        &self,
        config: ManagerConfig,
        embedder: Option<Arc<FakeEmbedder>>,
    ) -> FolderIndexManager {
        let events = Arc::clone(&self.events);
        FolderIndexManager::new(
            config,
            Arc::new(Source(embedder)),
            Arc::new(move |status: &FolderIndexStatusDto| events.lock().push(status.clone())),
        )
    }

    fn manager(&self, embedder: &Arc<FakeEmbedder>) -> FolderIndexManager {
        self.manager_with(self.config(), Some(Arc::clone(embedder)))
    }

    /// A project folder under the fake home.
    fn project(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = self.home.join(name);
        std::fs::create_dir_all(&root).unwrap();
        for (relative, contents) in files {
            write(&root, relative, contents);
        }
        root
    }

    fn index_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&self.base)
            .map(|entries| entries.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_default();
        dirs.sort();
        dirs
    }
}

fn text(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}

const RETRY_RS: &str = "pub fn schedule_backoff(attempt: u32) -> u32 {\n    attempt * 2\n}\n";
const PARSER_RS: &str = "pub fn parse_tokens(input: &str) -> Vec<String> {\n    input.split(' ').map(String::from).collect()\n}\n";
const NOTES_MD: &str = "# Notes\n\nThe zeppelin deployment checklist lives here.\n";

async fn open_settled(manager: &FolderIndexManager, root: &Path) -> FolderIndexStatusDto {
    manager.open(&text(root)).await.unwrap();
    manager.settled().await
}

async fn chunk_paths(dir: &Path) -> Vec<String> {
    let store = FolderStore::open(dir).await.unwrap();
    let mut paths: Vec<String> = store.files_under("").await.unwrap().into_keys().collect();
    paths.sort();
    store.close().await;
    paths
}

mod chunking {
    use super::*;

    fn rust_file(functions: usize, body_lines: usize) -> String {
        let mut out = String::from("use std::fmt;\n\n");
        for index in 0..functions {
            out.push_str(&format!(
                "/// Does thing {index}.\n#[inline]\npub fn thing_{index}() {{\n"
            ));
            for line in 0..body_lines {
                out.push_str(&format!(
                    "    let value_{line} = compute({line}, {index});\n"
                ));
            }
            out.push_str("}\n\n");
        }
        out
    }

    #[test]
    fn chunks_break_before_top_level_items_and_keep_attributes_with_them() {
        let source = rust_file(6, 20);
        let chunks = chunk_text(&source);
        assert!(chunks.len() > 2, "{chunks:#?}");
        for chunk in chunks.iter().skip(1) {
            let first = chunk.body.lines().next().unwrap();
            assert!(
                first.starts_with("/// Does thing"),
                "a chunk should start at an item's doc comment, got {first:?}"
            );
        }
    }

    #[test]
    fn line_ranges_are_one_based_inclusive_and_cover_the_file() {
        let source = rust_file(8, 25);
        let total = source.lines().count() as u32;
        let chunks = chunk_text(&source);
        assert_eq!(chunks.first().unwrap().start_line, 1);
        assert_eq!(chunks.last().unwrap().end_line, total);
        for pair in chunks.windows(2) {
            assert_eq!(pair[1].start_line, pair[0].end_line + 1);
        }
        let lines: Vec<&str> = source.lines().collect();
        for chunk in &chunks {
            let expected =
                lines[(chunk.start_line - 1) as usize..chunk.end_line as usize].join("\n");
            assert_eq!(chunk.body, expected);
            assert_eq!(
                chunk.indexed_text("src/lib.rs"),
                format!(
                    "src/lib.rs:{}-{}\n{}",
                    chunk.start_line, chunk.end_line, chunk.body
                )
            );
        }
    }

    #[test]
    fn markdown_breaks_at_headings() {
        let mut source = String::new();
        for section in 0..5 {
            source.push_str(&format!("## Section {section}\n"));
            for line in 0..30 {
                source.push_str(&format!(
                    "Sentence {line} of section {section}, long enough to count.\n"
                ));
            }
        }
        let chunks = chunk_text(&source);
        assert!(chunks.len() >= 5);
        for chunk in chunks.iter().skip(1) {
            assert!(chunk.body.starts_with("## Section"), "{}", chunk.body);
        }
    }

    #[test]
    fn caps_hold_without_boundaries_and_long_lines_are_clipped() {
        // Short indented lines: no boundary anywhere, so only the caps cut.
        let source: String = (0..1_000).map(|n| format!("  x{n}\n")).collect();
        let chunks = chunk_text(&source);
        for chunk in &chunks {
            let lines = (chunk.end_line - chunk.start_line + 1) as usize;
            assert!(lines <= MAX_CHUNK_LINES, "{lines} lines");
            assert!(
                chunk.body.len() <= MAX_CHUNK_CHARS,
                "{} chars",
                chunk.body.len()
            );
        }
        assert_eq!(chunks.last().unwrap().end_line, 1_000);

        let long = format!("short\n{}\nafter\n", "y".repeat(10_000));
        let chunks = chunk_text(&long);
        let clipped = chunks
            .iter()
            .flat_map(|chunk| chunk.body.lines())
            .find(|line| line.starts_with('y'))
            .unwrap();
        assert_eq!(clipped.chars().count(), MAX_LINE_CHARS + 1);
        assert!(clipped.ends_with('…'));
        assert_eq!(chunks.last().unwrap().end_line, 3);
    }

    #[test]
    fn blank_files_have_no_chunks() {
        assert!(chunk_text("").is_empty());
        assert!(chunk_text("\n\n   \n").is_empty());
    }

    #[test]
    fn lockfiles_and_minified_files_are_skipped() {
        for skipped in [
            "Cargo.lock",
            "web/yarn.lock",
            "package-lock.json",
            "pnpm-lock.yaml",
            "dist/app.min.js",
            "styles/site.min.css",
        ] {
            assert!(chunker::skipped_by_name(skipped), "{skipped}");
        }
        for kept in ["src/lock.rs", "src/main.rs", "README.md", "src/minimal.rs"] {
            assert!(!chunker::skipped_by_name(kept), "{kept}");
        }
        assert!(chunker::looks_minified(&"a".repeat(5_000)));
        assert!(!chunker::looks_minified(&"fn x() {}\n".repeat(100)));
    }
}

mod rules {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn the_disk_root_home_its_ancestors_and_the_data_dir_are_refused() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let manager = rig.manager(&embedder);
        let parent = rig.home.parent().unwrap().to_path_buf();
        // The data directory's parent holds the index itself.
        let data = rig.base.parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&data).unwrap();
        for root in [PathBuf::from("/"), rig.home.clone(), parent, data] {
            let status = manager.open(&text(&root)).await.unwrap();
            assert_eq!(
                status.state,
                FolderIndexState::Refused,
                "{}",
                root.display()
            );
            assert!(status.message.is_some());
            assert!(manager.search_for(&root).await.is_none());
        }
        assert!(
            rig.index_dirs().is_empty(),
            "a refused folder gets no index"
        );
        assert!(embedder.embedded().is_empty());

        let project = rig.project("project", &[("src/retry.rs", RETRY_RS)]);
        let status = open_settled(&manager, &project).await;
        assert_eq!(status.state, FolderIndexState::Ready);
        manager.close().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_folder_over_the_cap_embeds_nothing() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let mut config = rig.config();
        config.limits.max_files = 5;
        let manager = rig.manager_with(config, Some(Arc::clone(&embedder)));
        let files: Vec<(String, String)> = (0..8)
            .map(|n| (format!("src/file_{n}.rs"), format!("fn f{n}() {{}}\n")))
            .collect();
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let root = rig.project("big", &refs);
        let status = open_settled(&manager, &root).await;
        assert_eq!(status.state, FolderIndexState::TooLarge);
        assert_eq!(status.files_total, 8);
        let message = status.message.unwrap();
        assert!(
            message.contains("8 files") && message.contains("limit is 5"),
            "{message}"
        );
        assert!(embedder.embedded().is_empty());
        assert!(manager.search_for(&root).await.is_none());
        manager.close().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn no_embedding_model_leaves_the_folder_unindexed() {
        let rig = Rig::new();
        let manager = rig.manager_with(rig.config(), None);
        let root = rig.project("project", &[("src/retry.rs", RETRY_RS)]);
        let status = open_settled(&manager, &root).await;
        assert_eq!(status.state, FolderIndexState::Unavailable);
        assert!(manager.search_for(&root).await.is_none());
        manager.close().await;
    }
}

mod incremental {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn only_new_and_changed_files_are_embedded_and_deleted_ones_disappear() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let manager = rig.manager(&embedder);
        let root = rig.project(
            "project",
            &[
                ("src/retry.rs", RETRY_RS),
                ("src/parser.rs", PARSER_RS),
                ("docs/notes.md", NOTES_MD),
                ("logo.png", "\u{0}\u{0}binary"),
                ("Cargo.lock", "[[package]]\nname = \"x\"\n"),
            ],
        );
        let status = open_settled(&manager, &root).await;
        assert_eq!(status.state, FolderIndexState::Ready);
        assert_eq!(status.files_total, 4, "lockfile skipped, binary recorded");
        assert_eq!(status.files_indexed, 4);
        assert_eq!(status.chunks, 3);
        let first: Vec<String> = embedder.embedded();
        assert_eq!(first.len(), 3);
        assert!(first.iter().any(|t| t.starts_with("src/retry.rs:1-3\n")));
        let events = rig.events.lock().clone();
        assert!(events.iter().any(|e| e.state == FolderIndexState::Scanning));
        assert_eq!(events.last().unwrap().state, FolderIndexState::Ready);

        // Reopening with nothing changed embeds nothing.
        manager.close().await;
        embedder.forget_calls();
        open_settled(&manager, &root).await;
        assert!(embedder.embedded().is_empty());

        // Same bytes rewritten (a new modification time): still nothing.
        manager.close().await;
        write(&root, "src/retry.rs", RETRY_RS);
        // Changed, deleted.
        write(
            &root,
            "src/parser.rs",
            &format!("{PARSER_RS}\npub fn lex_more() {{}}\n"),
        );
        std::fs::remove_file(root.join("docs/notes.md")).unwrap();
        let status = open_settled(&manager, &root).await;
        assert_eq!(status.state, FolderIndexState::Ready);
        let second = embedder.embedded();
        assert!(!second.is_empty());
        assert!(
            second.iter().all(|t| t.starts_with("src/parser.rs:")),
            "only the changed file is embedded again: {second:?}"
        );

        // The deleted file is gone from rows, full text and vectors.
        let search = manager.search_for(&root).await.unwrap();
        let hits = search
            .search("zeppelin deployment checklist", None, 8)
            .await
            .unwrap();
        assert!(
            hits.iter().all(|hit| hit.path != "docs/notes.md"),
            "{hits:?}"
        );
        let found = search.search("lex_more", None, 8).await.unwrap();
        assert_eq!(
            found.first().map(|hit| hit.path.as_str()),
            Some("src/parser.rs")
        );
        manager.close().await;

        let dir = store::index_dir(&rig.base, &root);
        let paths = chunk_paths(&dir).await;
        assert_eq!(paths, vec!["logo.png", "src/parser.rs", "src/retry.rs"]);
        let index =
            USearchVectorIndex::open_or_create(DIM, store::vectors_path(&dir, "model-a")).unwrap();
        let store = FolderStore::open(&dir).await.unwrap();
        let counts = store.counts("").await.unwrap();
        store.close().await;
        assert_eq!(
            index.count() as u32,
            counts.chunks,
            "one vector per chunk, none stale"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn per_path_updates_follow_edits_removals_and_ignore_rules() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        write(&root, ".gitignore", "target/\n");
        write(&root, "src/retry.rs", RETRY_RS);
        write(&root, "src/parser.rs", PARSER_RS);
        let dir = root.join(".index-under-test");
        let store = Arc::new(FolderStore::open(&dir).await.unwrap());
        let embedder = FakeEmbedder::new("model-a");
        let vectors = Arc::new(
            USearchVectorIndex::open_or_create(DIM, store::vectors_path(&dir, "model-a"))
                .unwrap()
                .with_coalesced_saves(),
        );
        let status = Arc::new(StatusCell::new(
            FolderIndexStatusDto::new("", "", FolderIndexState::Scanning),
            Arc::new(|_: &FolderIndexStatusDto| {}),
        ));
        let indexer = Indexer {
            store: Arc::clone(&store),
            vectors: Arc::clone(&vectors),
            embedder: embedder.clone(),
            scope: Scope::open(&text(&root)).unwrap(),
            prefix: String::new(),
            status,
            cancel: CancellationToken::new(),
            limits: Limits::default(),
        };
        // The store's own directory is inside this folder; ignore it the way
        // a real index (which lives outside the folder) never needs to.
        write(&root, ".gitignore", "target/\n.index-under-test/\n");
        assert_eq!(indexer.run().await.unwrap(), Outcome::Done);
        // .gitignore is text too, so it is a passage of its own.
        assert_eq!(vectors.count(), 3);
        embedder.forget_calls();

        write(&root, "src/parser.rs", "pub fn parse_everything() {}\n");
        std::fs::remove_file(root.join("src/retry.rs")).unwrap();
        write(&root, "target/out.rs", "fn generated() {}\n");
        write(&root, "src/new/mod.rs", "pub fn brand_new() {}\n");
        let outcome = indexer
            .sync_paths(vec![
                "src/parser.rs".into(),
                "src/retry.rs".into(),
                "target/out.rs".into(),
                "src/new".into(),
            ])
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Done);
        let mut paths: Vec<String> = store.files_under("").await.unwrap().into_keys().collect();
        paths.sort();
        assert_eq!(paths, vec![".gitignore", "src/new/mod.rs", "src/parser.rs"]);
        let embedded = embedder.embedded();
        assert_eq!(embedded.len(), 2, "{embedded:?}");
        assert_eq!(vectors.count(), 3);
        store.close().await;
    }
}

mod cancellation {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn closing_stops_the_run_and_reopening_resumes_it() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        embedder.delay_ms.store(40, Ordering::SeqCst);
        let manager = rig.manager(&embedder);
        let files: Vec<(String, String)> = (0..40)
            .map(|n| {
                (
                    format!("src/m{n:02}.rs"),
                    format!("pub fn item_{n}() {{}}\n"),
                )
            })
            .collect();
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let root = rig.project("slow", &refs);
        manager.open(&text(&root)).await.unwrap();
        // Ten batches of four; wait for a few, then close mid-run.
        for _ in 0..500 {
            if embedder.batches.load(Ordering::SeqCst) >= 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        manager.close().await;
        let at_close = embedder.batches.load(Ordering::SeqCst);
        assert!(
            at_close < 10,
            "the run should not have finished: {at_close}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            embedder.batches.load(Ordering::SeqCst),
            at_close,
            "nothing runs after close"
        );
        let done_before = embedder.embedded().len();

        embedder.delay_ms.store(0, Ordering::SeqCst);
        embedder.forget_calls();
        let status = open_settled(&manager, &root).await;
        assert_eq!(status.state, FolderIndexState::Ready);
        assert_eq!(status.files_indexed, 40);
        let resumed = embedder.embedded().len();
        assert!(
            resumed < 40 && resumed + done_before >= 40,
            "resumed {resumed} after {done_before}"
        );
        manager.close().await;
    }
}

mod search {
    use super::*;

    #[test]
    fn reciprocal_rank_fusion_rewards_agreement() {
        let fused = fuse(&[&[1, 2, 3], &[3, 4]], RRF_K);
        let order: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
        assert_eq!(order, vec![3, 1, 2, 4]);
        let (_, top) = fused[0];
        assert!((top - (1.0 / 63.0 + 1.0 / 61.0)).abs() < 1e-6);
        assert!(fuse(&[], RRF_K).is_empty());
    }

    #[test]
    fn full_text_queries_are_quoted_literals() {
        assert_eq!(
            match_expression("where is \"retry\" NEAR(x)?").as_deref(),
            Some("\"where\"* OR \"is\" OR \"retry\"* OR \"near\"*")
        );
        assert_eq!(match_expression("a ! ?"), None);
        assert_eq!(
            match_expression("handle_retry").as_deref(),
            Some("\"handle_retry\"*")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_path_prefix_keeps_results_inside_it() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let manager = rig.manager(&embedder);
        let root = rig.project(
            "project",
            &[
                (
                    "src/net/retry.rs",
                    "pub fn retry_request() { backoff(); }\n",
                ),
                ("docs/retry.md", "# Retry\n\nHow retry backoff works.\n"),
                ("src/other.rs", "pub fn unrelated() {}\n"),
            ],
        );
        open_settled(&manager, &root).await;
        let search = manager.search_for(&root).await.unwrap();
        let all = search.search("retry backoff", None, 8).await.unwrap();
        assert!(all.iter().any(|hit| hit.path == "docs/retry.md"));
        assert!(all.iter().any(|hit| hit.path == "src/net/retry.rs"));
        for prefix in ["src", "./src/", "src/net"] {
            let inside = search
                .search("retry backoff", Some(prefix), 8)
                .await
                .unwrap();
            assert!(!inside.is_empty(), "{prefix}");
            assert!(
                inside.iter().all(|hit| hit.path.starts_with("src/")),
                "{prefix}: {inside:?}"
            );
        }
        let hit = all
            .iter()
            .find(|hit| hit.path == "src/net/retry.rs")
            .unwrap();
        assert_eq!((hit.start_line, hit.end_line), (1, 1));
        assert_eq!(hit.text, "pub fn retry_request() { backoff(); }");
        manager.close().await;
    }
}

mod registry {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_sub_folder_reuses_its_parents_index() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let manager = rig.manager(&embedder);
        let parent = rig.project(
            "repo",
            &[
                ("README.md", "# Repo\n\nThe retry story starts here.\n"),
                (
                    "crates/net/src/retry.rs",
                    "pub fn retry_with_backoff() {}\n",
                ),
            ],
        );
        open_settled(&manager, &parent).await;
        let embedded_once = embedder.embedded().len();
        manager.close().await;

        let child = parent.join("crates/net");
        let status = open_settled(&manager, &child).await;
        assert_eq!(status.root, text(&child));
        assert_eq!(status.index_root, text(&parent));
        assert_eq!(status.files_total, 1, "counts cover the sub-folder only");
        assert_eq!(rig.index_dirs().len(), 1, "no second index");
        assert_eq!(
            embedder.embedded().len(),
            embedded_once,
            "nothing embedded again"
        );

        let search = manager.search_for(&child).await.unwrap();
        let hits = search.search("retry", None, 8).await.unwrap();
        assert_eq!(
            hits.iter().map(|hit| hit.path.as_str()).collect::<Vec<_>>(),
            vec!["src/retry.rs"],
            "paths are relative to the picked folder, and the parent's README is out of scope"
        );
        manager.close().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_ninth_folder_evicts_the_least_recently_opened() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let manager = rig.manager(&embedder);
        let roots: Vec<PathBuf> = (0..10)
            .map(|n| rig.project(&format!("p{n}"), &[("main.rs", "fn main() {}\n")]))
            .collect();
        for root in roots.iter().take(8) {
            open_settled(&manager, root).await;
        }
        assert_eq!(rig.index_dirs().len(), 8);
        // p0 opened again is now the most recent; p1 is the oldest.
        open_settled(&manager, &roots[0]).await;
        open_settled(&manager, &roots[8]).await;
        let dirs = rig.index_dirs();
        assert_eq!(dirs.len(), 8);
        assert!(dirs.contains(&store::index_dir(&rig.base, &roots[0])));
        assert!(!dirs.contains(&store::index_dir(&rig.base, &roots[1])));
        assert!(dirs.contains(&store::index_dir(&rig.base, &roots[8])));
        open_settled(&manager, &roots[9]).await;
        assert!(!rig
            .index_dirs()
            .contains(&store::index_dir(&rig.base, &roots[2])));
        manager.close().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_new_embedding_model_embeds_everything_again() {
        let rig = Rig::new();
        let first = FakeEmbedder::new("model-a");
        let root = rig.project(
            "project",
            &[("src/retry.rs", RETRY_RS), ("src/parser.rs", PARSER_RS)],
        );
        let manager = rig.manager(&first);
        open_settled(&manager, &root).await;
        manager.close().await;
        assert_eq!(first.embedded().len(), 2);
        let dir = store::index_dir(&rig.base, &root);
        assert!(store::vectors_path(&dir, "model-a").exists());

        let second = FakeEmbedder::new("model-b");
        let manager = rig.manager(&second);
        let status = open_settled(&manager, &root).await;
        assert_eq!(status.state, FolderIndexState::Ready);
        assert_eq!(second.embedded().len(), 2, "every chunk embedded again");
        assert!(
            !store::vectors_path(&dir, "model-a").exists(),
            "old vectors dropped"
        );
        assert!(store::vectors_path(&dir, "model-b").exists());
        manager.close().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rebuild_starts_over_and_forget_deletes_the_index() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let manager = rig.manager(&embedder);
        let root = rig.project("project", &[("src/retry.rs", RETRY_RS)]);
        open_settled(&manager, &root).await;
        embedder.forget_calls();
        manager.rebuild(&text(&root)).await.unwrap();
        assert_eq!(manager.settled().await.state, FolderIndexState::Ready);
        assert_eq!(embedder.embedded().len(), 1);
        manager.forget(&text(&root)).await.unwrap();
        assert!(rig.index_dirs().is_empty());
        assert!(manager.search_for(&root).await.is_none());
    }
}

mod model {
    use super::*;

    fn hit(path: &str, start: u32, lines: &[&str]) -> FolderHit {
        FolderHit {
            path: path.to_string(),
            start_line: start,
            end_line: start + lines.len() as u32 - 1,
            text: lines.join("\n"),
            score: 0.1,
        }
    }

    #[test]
    fn passages_are_headed_by_line_references_and_fit_the_budget() {
        let hits = vec![
            hit("src/a.rs", 3, &["fn a() {", "    b();", "}"]),
            hit("src/b.rs", 10, &["fn b() {}"]),
        ];
        let out = tool::render_passages(&hits, 10_000);
        assert!(
            out.starts_with("`src/a.rs:3-5`\n   3│ fn a() {\n   4│     b();\n   5│ }\n"),
            "{out}"
        );
        assert!(out.contains("`src/b.rs:10-10`\n  10│ fn b() {}\n"), "{out}");
        assert!(crate::features::explorer::line_refs::contains_line_reference(&out));

        let long: Vec<String> = (0..200).map(|n| format!("let line_{n} = {n};")).collect();
        let long_refs: Vec<&str> = long.iter().map(String::as_str).collect();
        let many = vec![
            hit("src/long.rs", 1, &long_refs),
            hit("src/b.rs", 10, &["fn b() {}"]),
        ];
        let cut = tool::render_passages(&many, 600);
        assert!(cut.len() <= 600, "{}", cut.len());
        assert!(cut.contains("passage cut for length"));
        assert!(cut.contains("1 more passage not shown"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn search_folder_returns_line_referenced_passages_within_its_allowance() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let manager = rig.manager(&embedder);
        let body: String = (1..=60)
            .map(|n| format!("    retry_step_{n}();\n"))
            .collect();
        let root = rig.project(
            "project",
            &[
                (
                    "src/retry.rs",
                    &format!("pub fn retry_loop() {{\n{body}}}\n"),
                ),
                ("src/parser.rs", PARSER_RS),
            ],
        );
        open_settled(&manager, &root).await;
        let search = manager.search_for(&root).await.unwrap();
        let out = tool::run(&search, &json!({"query": "retry loop"}), 1_000)
            .await
            .unwrap();
        assert!(out.len() <= 1_000, "{}", out.len());
        assert!(out.contains("for \"retry loop\""), "{out}");
        assert!(out.contains("`src/retry.rs:1-"), "{out}");
        assert!(out.contains("   1│ pub fn retry_loop() {"), "{out}");

        // Dense search always has a nearest neighbour; an empty scope does not.
        let none = tool::run(&search, &json!({"query": "retry", "path": "docs"}), 1_000)
            .await
            .unwrap();
        assert!(
            none.starts_with("No passages for \"retry\" in docs"),
            "{none}"
        );
        assert!(tool::run(&search, &json!({}), 1_000).await.is_err());
        assert_eq!(
            tools::activity_label("search_folder", &json!({"query": "retry"})),
            "Searched the folder for \"retry\""
        );
        assert!(tools::is_explorer_tool("search_folder"));
        manager.close().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn first_step_passages_ride_in_the_block_within_half_its_budget() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let manager = rig.manager(&embedder);
        let long: String = (1..=400)
            .map(|n| format!("line {n} of the open file\n"))
            .collect();
        let root = rig.project(
            "project",
            &[
                (
                    "src/retry.rs",
                    "pub fn schedule_retry_backoff() {\n    sleep(backoff);\n}\n",
                ),
                ("open.txt", &long),
            ],
        );
        open_settled(&manager, &root).await;
        let search = manager.search_for(&root).await.unwrap();
        let scope = Scope::open(&text(&root)).unwrap();
        let focus = ExplorerFocusDto {
            open_path: Some("open.txt".into()),
            selection: None,
        };
        let turn = ExplorerTurn::with_index(scope.clone(), Some(focus.clone()), Some(search));
        assert!(turn.has_folder_index());
        let block = turn
            .context_block(4_000, true, Some("where is the retry backoff scheduled?"))
            .await;
        let closed_book = turn.context_block(4_000, false, None).await;
        assert!(!closed_book.contains("Passages from the folder"));
        assert!(block.len() <= 4_000, "{}", block.len());
        assert!(block.contains("Passages from the folder"), "{block}");
        assert!(block.contains("`src/retry.rs:1-3`"), "{block}");
        assert!(
            block.contains("search_folder"),
            "the rules mention the tool"
        );
        assert!(
            block.contains("Open file: open.txt"),
            "the open file keeps its place"
        );
        let passages_at = block.find("Passages from the folder").unwrap();
        let rules_at = block.find("How to answer about the folder").unwrap();
        assert!(
            rules_at - passages_at <= 2_000,
            "passages take at most half"
        );

        let mut tool_list = Vec::new();
        tools::add_to_turn(&mut tool_list, turn.has_folder_index());
        assert!(tool_list.iter().any(|t| t.name == "search_folder"));
        manager.close().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_empty_index_adds_no_passages_and_no_tool() {
        let rig = Rig::new();
        let embedder = FakeEmbedder::new("model-a");
        let manager = rig.manager(&embedder);
        let root = rig.project("empty", &[("logo.png", "\u{0}\u{0}binary only")]);
        let status = open_settled(&manager, &root).await;
        assert_eq!(status.state, FolderIndexState::Ready);
        let search = manager.search_for(&root).await;
        assert!(search.is_none(), "nothing indexed, nothing to search");
        let turn = ExplorerTurn::with_index(Scope::open(&text(&root)).unwrap(), None, search);
        let block = turn.context_block(4_000, true, Some("anything")).await;
        assert!(!block.contains("Passages from the folder"), "{block}");
        assert!(!block.contains("search_folder"));
        let mut tool_list = Vec::new();
        tools::add_to_turn(&mut tool_list, turn.has_folder_index());
        assert!(tool_list.iter().all(|t| t.name != "search_folder"));
        let empty = prompt::render_context(
            &Scope::open(&text(&root)).unwrap(),
            None,
            4_000,
            true,
            false,
            &FolderPassages::default(),
        );
        assert!(!empty.contains("Passages from the folder"));
        manager.close().await;
    }
}
