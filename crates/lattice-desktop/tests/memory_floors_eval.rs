//! Measures the two relevance floors of conversation memory, which were set
//! without data: `RECALL_RELEVANCE_FLOOR` (automatic recall of a chat's older
//! messages) and `SHARED_RELEVANCE_FLOOR` (an item shared from another chat in
//! the pool, and the meaning arm of the recall gate).
//!
//! # What this measures
//!
//! For every probe turn of `evals/conversation-memory/recall-floors-v1.json`
//! and every candidate it could bring back, the signals the gates read:
//!
//! - **Recall** (`gate: "recall"`): the candidate's `lexical_share` from the
//!   repository's word index and `relevance_by_message`, its `similarity` from
//!   `search_source_messages` (the hybrid search's cosine, over the query the
//!   turn forms with its recent turns), and its `exact_identifier` flag.
//! - **Shared** (`gate: "shared"`): whether the item `shares_word` with the
//!   turn, from `prepare_memory` on a repository with no embedder, and its
//!   `similarity` from `knowledge_semantic_scores`.
//!
//! It applies no floor. `scripts/analyse_memory_floors.py` sweeps them. At the
//! shipped floors the signals must agree with the shipped code, and the run
//! stops if they do not: a shared probe's must reproduce what `prepare_memory`
//! offers with the embedder on, and every older message `recall_for_turn`
//! anchors a passage group on must clear the recall gate.
//!
//! The database is built by the shipped migrations, the messages by the
//! repository, and their vectors by the embedding model the app loads for
//! conversation memory: `open_gguf`, as the app's loader starts it, then
//! `split_text` and `embed_single` per message as the chat's indexer does, and
//! `embed_query` for the turn.
//!
//! # Running it
//!
//! The test is `#[ignore]`d, so `cargo test` never starts a model:
//!
//! ```text
//! SQLX_OFFLINE=true \
//! LATTICE_EMBED_GGUF=/path/to/Qwen3-Embedding-0.6B-f16.gguf \
//! LATTICE_EVAL_FLOORS_OUT=/path/to/new-rows.jsonl \
//!   cargo test --manifest-path crates/lattice-desktop/Cargo.toml \
//!     --test memory_floors_eval -- --ignored --nocapture
//! ```
//!
//! | Variable | Meaning | Default |
//! |---|---|---|
//! | `LATTICE_EMBED_GGUF` | the embedding model's GGUF, with `tokenizer.json` beside it | *required* |
//! | `LATTICE_EVAL_FLOORS_OUT` | the JSONL file to write; it must not exist | *required* |
//! | `LATTICE_LLAMA_SERVER` | the llama-server binary | the bundled build in `binaries/` |
//! | `LATTICE_EVAL_FLOORS_SET` | the labelled set | `evals/conversation-memory/recall-floors-v1.json` |
//! | `LATTICE_EVAL_TIME_LIMIT_SECS` | the run's time limit, after which it stops and writes nothing | `3600` |
//!
//! Nothing is written until every probe is scored, and only the rows are
//! written. They carry ids and numbers, no conversation text. No credential is
//! read.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unwrap_in_result,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::print_stdout
)]

use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use lattice::features::conversation::chat::history_tools::{
    build_recall_query, exact_identifiers, recall_for_turn, relevance_by_message,
    what_the_turn_asks, HistoryToolBudget, HistoryToolScope, RecallRequest, RECALL_RELEVANCE_FLOOR,
};
use lattice::features::conversation::repository::ConversationRepository;
use lattice::features::embedding::llama_cpp::LlamaEmbeddingService;
use lattice::interfaces::di::modules::standalone::open_gguf;
use lattice_core::application::ports::conversation_memory::{
    ConversationMemoryPort, ConversationMemoryReadPort, RecallCandidates, SHARED_RELEVANCE_FLOOR,
};
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::domain::conversation::memory::compute_digest;
use lattice_core::domain::conversation::MessageRole;
use lattice_core::shared::persistence::embedding_encoding::encode_embedding;

// ---------------------------------------------------------------------------
// The labelled set
// ---------------------------------------------------------------------------

const RECALL_KINDS: [&str; 5] = [
    "exact_restatement",
    "paraphrase",
    "follow_up",
    "instruction_only",
    "distractor",
];
const SHARED_PROBE_KINDS: [&str; 2] = ["shared_bearing", "shared_unrelated"];
/// The kinds of item the app shares with a chat's pool without being asked.
const SHARED_ITEM_KINDS: [&str; 3] = ["user_fact", "preference", "goal"];
/// Candidates a recall probe needs at least, so a floor has something to reject.
const MIN_CANDIDATES: usize = 10;
/// Attempts at a search whose meaning branch did not run, before the run stops.
const SEARCH_ATTEMPTS: usize = 3;

#[derive(Debug, Deserialize)]
struct Message {
    id: String,
    role: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct Conversation {
    id: String,
    messages: Vec<Message>,
}

#[derive(Debug, Deserialize)]
struct RecallProbe {
    id: String,
    kind: String,
    conversation: String,
    /// The assistant message the probe turn follows.
    after: String,
    turn: String,
    relevant: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Item {
    id: String,
    kind: String,
    /// The user message the item was extracted from.
    message: String,
    label: String,
    /// The exact span of that message the item quotes.
    quote: String,
}

#[derive(Debug, Deserialize)]
struct Chat {
    id: String,
    messages: Vec<Message>,
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Pool {
    id: String,
    chats: Vec<Chat>,
}

#[derive(Debug, Deserialize)]
struct SharedProbe {
    id: String,
    kind: String,
    pool: String,
    turn: String,
    relevant: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FloorsSet {
    name: String,
    /// Messages ahead of a probe that the prompt already carries.
    recent_turns: usize,
    conversations: Vec<Conversation>,
    recall_probes: Vec<RecallProbe>,
    pools: Vec<Pool>,
    shared_probes: Vec<SharedProbe>,
}

impl FloorsSet {
    fn load(path: &Path) -> Self {
        let raw = std::fs::read_to_string(path).unwrap_or_else(|error| {
            panic!("the labelled set {} is unreadable: {error}", path.display())
        });
        let set: Self = serde_json::from_str(&raw).unwrap_or_else(|error| {
            panic!("the labelled set {} is malformed: {error}", path.display())
        });
        let problems = set.problems();
        assert!(
            problems.is_empty(),
            "the labelled set {} is inconsistent:\n{}",
            path.display(),
            problems.join("\n")
        );
        set
    }

    /// Everything wrong with the set, so one run reports all of it.
    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut unique = |problems: &mut Vec<String>, what: &str, id: &str| {
            if !seen.insert(id.to_owned()) {
                problems.push(format!("{what} id {id} is not unique"));
            }
        };

        if self.recent_turns == 0 {
            problems.push("recent_turns must be at least 1".into());
        }
        for conversation in &self.conversations {
            unique(&mut problems, "conversation", &conversation.id);
            for (index, message) in conversation.messages.iter().enumerate() {
                unique(&mut problems, "message", &message.id);
                let expected = if index % 2 == 0 { "user" } else { "assistant" };
                if message.role != expected {
                    problems.push(format!("message {} should be {expected}", message.id));
                }
                if message.text.trim().is_empty() {
                    problems.push(format!("message {} is empty", message.id));
                }
            }
        }
        let mut pool_items: HashMap<&str, HashSet<&str>> = HashMap::new();
        for pool in &self.pools {
            unique(&mut problems, "pool", &pool.id);
            for chat in &pool.chats {
                unique(&mut problems, "chat", &chat.id);
                for message in &chat.messages {
                    unique(&mut problems, "message", &message.id);
                    if message.text.trim().is_empty() {
                        problems.push(format!("message {} is empty", message.id));
                    }
                }
                for item in &chat.items {
                    unique(&mut problems, "item", &item.id);
                    pool_items.entry(&pool.id).or_default().insert(&item.id);
                    if !SHARED_ITEM_KINDS.contains(&item.kind.as_str()) {
                        problems.push(format!("item {} has kind {}", item.id, item.kind));
                    }
                    if item.label.trim().is_empty() {
                        problems.push(format!("item {} has no label", item.id));
                    }
                    match chat.messages.iter().find(|m| m.id == item.message) {
                        Some(message) if message.role == "user" => {
                            if !message.text.contains(&item.quote) || item.quote.is_empty() {
                                problems.push(format!(
                                    "item {} quotes text that {} does not contain",
                                    item.id, item.message
                                ));
                            }
                        }
                        _ => problems.push(format!(
                            "item {} names {}, which is not a user message of chat {}",
                            item.id, item.message, chat.id
                        )),
                    }
                }
            }
        }

        for probe in &self.recall_probes {
            unique(&mut problems, "probe", &probe.id);
            if !RECALL_KINDS.contains(&probe.kind.as_str()) {
                problems.push(format!("probe {} has kind {}", probe.id, probe.kind));
            }
            if probe.turn.trim().is_empty() {
                problems.push(format!("probe {} has no turn", probe.id));
            }
            let Some(conversation) = self
                .conversations
                .iter()
                .find(|c| c.id == probe.conversation)
            else {
                problems.push(format!(
                    "probe {} names conversation {}",
                    probe.id, probe.conversation
                ));
                continue;
            };
            let Some(index) = conversation
                .messages
                .iter()
                .position(|m| m.id == probe.after)
            else {
                problems.push(format!(
                    "probe {} follows {}, which is not in its conversation",
                    probe.id, probe.after
                ));
                continue;
            };
            if index % 2 == 0 {
                problems.push(format!(
                    "probe {} follows {}, a user message",
                    probe.id, probe.after
                ));
            }
            let at = index + 1;
            if at < self.recent_turns + MIN_CANDIDATES {
                problems.push(format!(
                    "probe {} follows {}, too early to leave {MIN_CANDIDATES} candidates",
                    probe.id, probe.after
                ));
                continue;
            }
            let older: HashSet<&str> = conversation.messages[..at - self.recent_turns]
                .iter()
                .map(|m| m.id.as_str())
                .collect();
            for relevant in &probe.relevant {
                if !older.contains(relevant.as_str()) {
                    problems.push(format!(
                        "probe {} labels {relevant}, which is not an older message",
                        probe.id
                    ));
                }
            }
            let bears = !matches!(probe.kind.as_str(), "instruction_only" | "distractor");
            if bears == probe.relevant.is_empty() {
                problems.push(format!(
                    "probe {} of kind {} has relevant {:?}",
                    probe.id, probe.kind, probe.relevant
                ));
            }
        }
        for probe in &self.shared_probes {
            unique(&mut problems, "probe", &probe.id);
            if !SHARED_PROBE_KINDS.contains(&probe.kind.as_str()) {
                problems.push(format!("probe {} has kind {}", probe.id, probe.kind));
            }
            if probe.turn.trim().is_empty() {
                problems.push(format!("probe {} has no turn", probe.id));
            }
            let Some(items) = pool_items.get(probe.pool.as_str()) else {
                problems.push(format!("probe {} names pool {}", probe.id, probe.pool));
                continue;
            };
            for relevant in &probe.relevant {
                if !items.contains(relevant.as_str()) {
                    problems.push(format!(
                        "probe {} labels {relevant}, which is not in its pool",
                        probe.id
                    ));
                }
            }
            if (probe.kind == "shared_bearing") == probe.relevant.is_empty() {
                problems.push(format!(
                    "probe {} of kind {} has relevant {:?}",
                    probe.id, probe.kind, probe.relevant
                ));
            }
        }
        for kind in RECALL_KINDS {
            if !self.recall_probes.iter().any(|p| p.kind == kind) {
                problems.push(format!("no probe of kind {kind}"));
            }
        }
        for kind in SHARED_PROBE_KINDS {
            if !self.shared_probes.iter().any(|p| p.kind == kind) {
                problems.push(format!("no probe of kind {kind}"));
            }
        }
        problems
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Printed instead of a silent skip: a run that measured nothing must not read
/// as a pass.
const MISSING_CONFIG: &str = "\
LATTICE_EMBED_GGUF and LATTICE_EVAL_FLOORS_OUT must both be set, so no model was started and \
nothing was measured. See the module comment at the top of tests/memory_floors_eval.rs for \
the full command.";

#[derive(Debug)]
struct Config {
    gguf: PathBuf,
    set: PathBuf,
    out: PathBuf,
    time_limit: Duration,
}

impl Config {
    fn from_env() -> Self {
        let gguf = PathBuf::from(std::env::var_os("LATTICE_EMBED_GGUF").expect(MISSING_CONFIG));
        let out = PathBuf::from(std::env::var_os("LATTICE_EVAL_FLOORS_OUT").expect(MISSING_CONFIG));
        let set = std::env::var_os("LATTICE_EVAL_FLOORS_SET").map_or_else(
            || {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../evals/conversation-memory/recall-floors-v1.json")
            },
            PathBuf::from,
        );
        let seconds = match std::env::var("LATTICE_EVAL_TIME_LIMIT_SECS") {
            Ok(raw) => raw.parse::<u64>().unwrap_or_else(|_| {
                panic!("LATTICE_EVAL_TIME_LIMIT_SECS={raw} is not a whole number of seconds")
            }),
            Err(std::env::VarError::NotPresent) => 3600,
            Err(error) => panic!("LATTICE_EVAL_TIME_LIMIT_SECS is unreadable: {error}"),
        };
        assert!(
            seconds > 0,
            "LATTICE_EVAL_TIME_LIMIT_SECS must be above zero"
        );
        let config = Self {
            gguf,
            set,
            out,
            time_limit: Duration::from_secs(seconds),
        };
        config.check();
        config
    }

    /// Everything that can be wrong before a model starts.
    fn check(&self) {
        assert!(
            self.gguf.is_file(),
            "LATTICE_EMBED_GGUF={} is not a file",
            self.gguf.display()
        );
        let tokenizer = self.gguf.parent().map(|dir| dir.join("tokenizer.json"));
        assert!(
            tokenizer.as_ref().is_some_and(|path| path.is_file()),
            "tokenizer.json must sit beside {}",
            self.gguf.display()
        );
        assert!(
            !self.out.exists(),
            "LATTICE_EVAL_FLOORS_OUT={} exists; a result is never overwritten, so name a new file",
            self.out.display()
        );
        let parent = self
            .out
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        assert!(
            parent.is_dir(),
            "the folder of LATTICE_EVAL_FLOORS_OUT={} does not exist",
            self.out.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

/// One probe against one candidate. The embedding model is on every row so a
/// result read back later says what produced it.
#[derive(Debug, Serialize)]
struct Row<'a> {
    probe: &'a str,
    kind: &'a str,
    gate: &'a str,
    candidate: &'a str,
    relevant: bool,
    /// Whether the shipped search returned the candidate; recall only.
    found: Option<bool>,
    /// The candidate's share of the turn's weight; recall only, absent when it holds none of the turn's words.
    lexical_share: Option<f32>,
    /// Whether the item shares a word with the turn; shared only.
    shares_word: Option<bool>,
    similarity: Option<f32>,
    exact_identifier: bool,
    embedding_model: &'a str,
    embedding_name: &'a str,
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

struct Scorer {
    pool: SqlitePool,
    /// With the embedder, as the chat turn has it.
    repo: ConversationRepository,
    /// Without it, so a pool item is offered by shared words alone.
    lexical_repo: ConversationRepository,
    service: Arc<LlamaEmbeddingService>,
    identity: String,
    name: String,
    /// Each distinct message text is embedded once.
    vectors: HashMap<String, Vec<u8>>,
}

impl Scorer {
    async fn open(service: Arc<LlamaEmbeddingService>) -> Self {
        let pool = SqlitePoolOptions::new()
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect(":memory:")
            .await
            .expect("an in-memory database");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("the shipped migrations");
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .expect("foreign keys");
        let embedder: Arc<dyn EmbeddingPort> = service.clone();
        Self {
            repo: ConversationRepository::new(pool.clone()).with_memory_embedding(Some(embedder)),
            lexical_repo: ConversationRepository::new(pool.clone()),
            identity: service.model_identity(),
            name: service.facts().display_name.clone(),
            pool,
            service,
            vectors: HashMap::new(),
        }
    }

    /// A message's stored vector, made as the chat's indexer makes it: split
    /// with the model's tokenizer, then one `embed_single` per chunk. A message
    /// that needs more than one chunk is outside this set.
    async fn vector_of(&mut self, text: &str) -> Vec<u8> {
        if let Some(blob) = self.vectors.get(text) {
            return blob.clone();
        }
        let chunks = self
            .service
            .split_text(text, "")
            .expect("the embedder splits the message");
        assert_eq!(
            chunks.len(),
            1,
            "a message longer than the embedder's window is not in this set: {}",
            text.chars().take(60).collect::<String>()
        );
        let vector = self
            .service
            .embed_single(&chunks[0].text)
            .await
            .expect("the embedder embeds the message");
        assert_eq!(
            vector.len(),
            self.service.dimension(),
            "a vector of the wrong width"
        );
        let blob = encode_embedding(&vector);
        self.vectors.insert(text.to_owned(), blob.clone());
        blob
    }

    /// Warms the embedder's query cache with `text` as the shipped searches
    /// will send it, so their five-second wait never decides a row.
    async fn warm_query(&self, text: &str) {
        let sent: String = text.chars().take(2048).collect();
        self.service
            .embed_query(&sent)
            .await
            .expect("the embedder embeds the query");
    }

    async fn new_conversation(&self, title: &str) -> String {
        self.repo
            .create_conversation(title, None)
            .await
            .expect("a conversation")
            .id
            .to_string()
    }

    /// Saves `messages` to `conversation` and a vector for each. Returns the
    /// stored id of every message by its id in the set.
    async fn seed(&mut self, conversation: &str, messages: &[&Message]) -> HashMap<String, String> {
        let mut stored = HashMap::new();
        for message in messages {
            let role = match message.role.as_str() {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                other => panic!("message {} has role {other}", message.id),
            };
            let saved = self
                .repo
                .add_message_with_status_repo(
                    conversation,
                    role,
                    &message.text,
                    0,
                    None,
                    "completed",
                )
                .await
                .expect("the message is saved");
            let blob = self.vector_of(&message.text).await;
            let written = self
                .repo
                .persist_memory_vector(
                    conversation,
                    &uuid::Uuid::new_v4().to_string(),
                    &saved.id,
                    &message.role,
                    &message.text,
                    blob,
                    self.service.dimension() as i64,
                    &self.identity,
                    &chrono::Utc::now().to_rfc3339(),
                )
                .await
                .expect("the vector is saved");
            assert_eq!(
                written, 1,
                "the vector of message {} was not stored",
                message.id
            );
            stored.insert(message.id.clone(), saved.id);
        }
        stored
    }

    /// A recall probe's rows. The conversation is rebuilt as it stood: the
    /// messages up to `after`, then the turn saved as the chat saves it before
    /// recall runs, so the word index counts it as it does in the app.
    async fn recall(&mut self, set: &FloorsSet, probe: &RecallProbe) -> Vec<String> {
        let conversation = set
            .conversations
            .iter()
            .find(|c| c.id == probe.conversation)
            .expect("the probe's conversation");
        let at = conversation
            .messages
            .iter()
            .position(|m| m.id == probe.after)
            .expect("the probe's message")
            + 1;
        let prefix: Vec<&Message> = conversation.messages[..at].iter().collect();
        let (older, recent) = prefix.split_at(at - set.recent_turns);
        let recent_text: Vec<String> = recent.iter().map(|m| m.text.clone()).collect();

        let id = self
            .new_conversation(&format!("{} after {}", conversation.id, probe.after))
            .await;
        let stored = self.seed(&id, &prefix).await;
        self.repo
            .add_message_with_status_repo(&id, MessageRole::User, &probe.turn, 0, None, "pending")
            .await
            .expect("the turn is saved");

        let query = build_recall_query(&probe.turn, &recent_text);
        let (terms, context) = what_the_turn_asks(&probe.turn, &recent_text);
        let exact_terms = exact_identifiers(&probe.turn, context);
        assert!(
            !(query.trim().is_empty() && exact_terms.is_empty()),
            "probe {} asks recall nothing",
            probe.id
        );
        self.warm_query(&query).await;

        let held = self
            .repo
            .messages_holding(&id, &terms)
            .await
            .expect("the turn's words are looked up");
        let relevance = relevance_by_message(&held);
        let found = self
            .search(&id, &query, &exact_terms, at + MIN_CANDIDATES, &probe.id)
            .await;
        self.check_against_recall_for_turn(
            probe,
            &id,
            &recent_text,
            older,
            &stored,
            &found,
            &relevance,
        )
        .await;

        older
            .iter()
            .map(|message| {
                let stored_id = &stored[&message.id];
                let hit = found.candidates.iter().find(|c| &c.message_id == stored_id);
                self.line(&Row {
                    probe: &probe.id,
                    kind: &probe.kind,
                    gate: "recall",
                    candidate: &message.id,
                    relevant: probe.relevant.contains(&message.id),
                    found: Some(hit.is_some()),
                    lexical_share: relevance.get(stored_id.as_str()).copied(),
                    shares_word: None,
                    similarity: hit.and_then(|c| c.similarity),
                    exact_identifier: hit.is_some_and(|c| c.exact_identifier),
                    embedding_model: &self.identity,
                    embedding_name: &self.name,
                })
            })
            .collect()
    }

    /// Runs the shipped `recall_for_turn` on the turn and requires that every
    /// older message it anchored a group on clears the recall gate by these
    /// signals, at the shipped floors. A group on a recent turn is not a
    /// candidate here and is skipped.
    async fn check_against_recall_for_turn(
        &self,
        probe: &RecallProbe,
        conversation: &str,
        recent_text: &[String],
        older: &[&Message],
        stored: &HashMap<String, String>,
        found: &RecallCandidates,
        relevance: &HashMap<&str, f32>,
    ) {
        let recalled = recall_for_turn(
            &self.repo,
            &HistoryToolScope::new(conversation, HistoryToolBudget::default()),
            &RecallRequest {
                user_input: &probe.turn,
                recent_turns: recent_text,
                active_memory: &[],
                max_bytes: 0,
            },
        )
        .await
        .expect("recall_for_turn runs");
        for group in &recalled.groups {
            let anchor: String = sqlx::query_scalar(
                "SELECT id FROM conversation_messages WHERE conversation_id=? AND sequence=?",
            )
            .bind(conversation)
            .bind(group.anchor_sequence)
            .fetch_one(&self.pool)
            .await
            .expect("a group is anchored on a message of the conversation");
            let Some(message) = older.iter().find(|m| stored[&m.id] == anchor) else {
                continue;
            };
            let hit = found.candidates.iter().find(|c| c.message_id == anchor);
            let clears = hit.is_some_and(|c| {
                c.exact_identifier
                    || relevance
                        .get(c.message_id.as_str())
                        .is_some_and(|share| *share >= RECALL_RELEVANCE_FLOOR)
                    || c.similarity
                        .is_some_and(|similarity| similarity >= SHARED_RELEVANCE_FLOOR)
            });
            assert!(
                clears,
                "probe {}: recall_for_turn anchored a group on {}, which these signals do not let through at the shipped floors",
                probe.id, message.id
            );
            assert_eq!(
                group.from_exact_identifier,
                hit.is_some_and(|c| c.exact_identifier),
                "probe {}: recall_for_turn and these signals disagree whether {} is an exact identifier",
                probe.id,
                message.id
            );
        }
    }

    /// The shipped search over every message, repeated until its meaning
    /// branch has run: a row without a similarity must mean no similarity, not
    /// a search that did not look.
    async fn search(
        &self,
        conversation: &str,
        query: &str,
        exact_terms: &[String],
        limit: usize,
        probe: &str,
    ) -> RecallCandidates {
        let mut last_error = None;
        for _ in 0..SEARCH_ATTEMPTS {
            let found = self
                .repo
                .search_source_messages(conversation, query, exact_terms, limit)
                .await
                .expect("recall searches");
            if found.semantic_ran && found.index_error.is_none() {
                return found;
            }
            last_error = found.index_error;
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        panic!(
            "probe {probe}: the meaning branch of recall did not run in {SEARCH_ATTEMPTS} attempts (index_error {last_error:?})"
        );
    }

    /// Builds the pool: a space, its chats with their messages, vectors and
    /// items, and the empty chat the probes are asked in. Returns the home chat
    /// and each item's stored id by its id in the set.
    async fn build_pool(&mut self, pool: &Pool) -> (String, HashMap<String, String>) {
        sqlx::query("INSERT INTO conversation_spaces(id,name) VALUES (?,?)")
            .bind(&pool.id)
            .bind(&pool.id)
            .execute(&self.pool)
            .await
            .expect("a space");
        let home = self.new_conversation(&format!("home of {}", pool.id)).await;
        self.put_in_space(&home, &pool.id).await;
        let mut items = HashMap::new();
        for chat in &pool.chats {
            let id = self.new_conversation(&chat.id).await;
            self.put_in_space(&id, &pool.id).await;
            let messages: Vec<&Message> = chat.messages.iter().collect();
            let stored = self.seed(&id, &messages).await;
            sqlx::query("INSERT INTO conversation_memory_state(conversation_id,memory_revision) VALUES (?,1) ON CONFLICT(conversation_id) DO NOTHING")
                .bind(&id)
                .execute(&self.pool)
                .await
                .expect("a memory state");
            for item in &chat.items {
                let message = chat
                    .messages
                    .iter()
                    .find(|m| m.id == item.message)
                    .expect("the item's message");
                let message_id = &stored[&message.id];
                let sequence: i64 =
                    sqlx::query_scalar("SELECT sequence FROM conversation_messages WHERE id=?")
                        .bind(message_id)
                        .fetch_one(&self.pool)
                        .await
                        .expect("the message's sequence");
                let start = message
                    .text
                    .find(&item.quote)
                    .expect("the quote is in its message");
                let item_id = uuid::Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO conversation_memory_items(id,conversation_id,kind,label,created_at_sequence,changed_at_sequence,created_at,updated_at) VALUES (?,?,?,?,?,?,'2026-11-01T00:00:00Z','2026-11-01T00:00:00Z')")
                    .bind(&item_id)
                    .bind(&id)
                    .bind(&item.kind)
                    .bind(&item.label)
                    .bind(sequence)
                    .bind(sequence)
                    .execute(&self.pool)
                    .await
                    .expect("an item");
                sqlx::query("INSERT INTO conversation_memory_evidence(item_id,ordinal,message_id,sequence,role,start_byte,end_byte,content_digest,purpose) VALUES (?,0,?,?,'user',?,?,?,'assertion')")
                    .bind(&item_id)
                    .bind(message_id)
                    .bind(sequence)
                    .bind(start as i64)
                    .bind((start + item.quote.len()) as i64)
                    .bind(compute_digest(&message.text))
                    .execute(&self.pool)
                    .await
                    .expect("an item's evidence");
                items.insert(item.id.clone(), item_id);
            }
        }
        (home, items)
    }

    async fn put_in_space(&self, conversation: &str, space: &str) {
        sqlx::query("UPDATE conversations SET space_id=? WHERE id=?")
            .bind(space)
            .bind(conversation)
            .execute(&self.pool)
            .await
            .expect("a conversation joins its space");
    }

    /// The items `prepare_memory` offers a chat for `turn`.
    async fn offered(
        repo: &ConversationRepository,
        conversation: &str,
        turn: &str,
    ) -> HashSet<String> {
        repo.prepare_memory(conversation, turn)
            .await
            .expect("memory is prepared")
            .expect("the repository prepares memory")
            .optional
            .iter()
            .map(|text| {
                serde_json::from_str::<serde_json::Value>(text).expect("an offered item is JSON")
                    ["memory_id"]
                    .as_str()
                    .expect("an offered item names itself")
                    .to_owned()
            })
            .collect()
    }

    /// A shared probe's rows: every item of its pool. The two signals are
    /// checked against what the shipped path offers at the shipped floor.
    async fn shared(
        &self,
        probe: &SharedProbe,
        home: &str,
        items: &HashMap<String, String>,
        pool_items: &[&Item],
    ) -> Vec<String> {
        self.warm_query(&probe.turn).await;
        let scores = self.repo.knowledge_semantic_scores(home, &probe.turn).await;
        assert_eq!(
            scores.len(),
            pool_items.len(),
            "probe {}: {} of {} pool items were scored; the meaning arm did not look at them all",
            probe.id,
            scores.len(),
            pool_items.len()
        );
        let by_word = Self::offered(&self.lexical_repo, home, &probe.turn).await;
        let offered = Self::offered(&self.repo, home, &probe.turn).await;

        let expected: HashSet<String> = pool_items
            .iter()
            .map(|item| &items[&item.id])
            .filter(|stored| {
                by_word.contains(*stored)
                    || scores.get(*stored).copied().unwrap_or(0.0) >= SHARED_RELEVANCE_FLOOR
            })
            .cloned()
            .collect();
        assert_eq!(
            offered, expected,
            "probe {}: the signals do not reproduce what the shipped path offers at SHARED_RELEVANCE_FLOOR",
            probe.id
        );

        pool_items
            .iter()
            .map(|item| {
                let stored = &items[&item.id];
                self.line(&Row {
                    probe: &probe.id,
                    kind: &probe.kind,
                    gate: "shared",
                    candidate: &item.id,
                    relevant: probe.relevant.contains(&item.id),
                    found: None,
                    lexical_share: None,
                    shares_word: Some(by_word.contains(stored)),
                    similarity: scores.get(stored).copied(),
                    exact_identifier: false,
                    embedding_model: &self.identity,
                    embedding_name: &self.name,
                })
            })
            .collect()
    }

    fn line(&self, row: &Row<'_>) -> String {
        serde_json::to_string(row).expect("a row serialises")
    }
}

/// Scores every probe of `set` and returns the rows, as JSON lines.
async fn run(config: &Config, set: &FloorsSet) -> Vec<String> {
    let service = Arc::new(
        open_gguf(&config.gguf)
            .await
            .expect("the embedding model starts on a llama-server"),
    );
    let mut scorer = Scorer::open(service).await;
    let mut rows = Vec::new();

    for probe in &set.recall_probes {
        rows.extend(scorer.recall(set, probe).await);
    }

    let mut built: HashMap<&str, (String, HashMap<String, String>)> = HashMap::new();
    for pool in &set.pools {
        built.insert(&pool.id, scorer.build_pool(pool).await);
    }
    for probe in &set.shared_probes {
        let pool = set
            .pools
            .iter()
            .find(|p| p.id == probe.pool)
            .expect("the probe's pool");
        let (home, items) = &built[probe.pool.as_str()];
        let pool_items: Vec<&Item> = pool.chats.iter().flat_map(|c| c.items.iter()).collect();
        rows.extend(scorer.shared(probe, home, items, &pool_items).await);
    }
    rows
}

/// What the rows must number, from the set alone.
fn expected_rows(set: &FloorsSet) -> usize {
    let recall: usize = set
        .recall_probes
        .iter()
        .map(|probe| {
            let conversation = set
                .conversations
                .iter()
                .find(|c| c.id == probe.conversation)
                .expect("the probe's conversation");
            let at = conversation
                .messages
                .iter()
                .position(|m| m.id == probe.after)
                .expect("the probe's message")
                + 1;
            at - set.recent_turns
        })
        .sum();
    let shared: usize = set
        .shared_probes
        .iter()
        .map(|probe| {
            set.pools
                .iter()
                .filter(|p| p.id == probe.pool)
                .flat_map(|p| p.chats.iter())
                .map(|c| c.items.len())
                .sum::<usize>()
        })
        .sum();
    recall + shared
}

#[tokio::test]
#[ignore = "needs LATTICE_EMBED_GGUF (tokenizer.json beside it), a runnable llama-server and LATTICE_EVAL_FLOORS_OUT; see the module comment"]
async fn the_signals_behind_the_recall_and_shared_floors() {
    let config = Config::from_env();
    let set = FloorsSet::load(&config.set);

    let rows = match tokio::time::timeout(config.time_limit, run(&config, &set)).await {
        Ok(rows) => rows,
        Err(_) => panic!(
            "the time limit of {} s ran out before every probe was scored; nothing was written",
            config.time_limit.as_secs()
        ),
    };
    assert_eq!(
        rows.len(),
        expected_rows(&set),
        "the rows do not number what the set implies"
    );

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config.out)
        .unwrap_or_else(|error| panic!("{} cannot be created: {error}", config.out.display()));
    for row in &rows {
        writeln!(file, "{row}").expect("a row is written");
    }
    file.sync_all().expect("the rows reach the disk");
    println!(
        "{}: wrote {} rows to {} (shipped floors: recall {RECALL_RELEVANCE_FLOOR}, shared {SHARED_RELEVANCE_FLOOR})",
        set.name,
        rows.len(),
        config.out.display()
    );
}
