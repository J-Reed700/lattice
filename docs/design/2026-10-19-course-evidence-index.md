# A per-course evidence index, and claim pages read from it

Date: 2026-10-19. Status: design; replaces the ring structure of `2026-10-19-claim-evidence-pages.md` (§4–7, §10 D1–D3 there are withdrawn; its part 1, PR #114, and its §13 pace budget stand). Code read on `feat/claim-evidence-pages` in `/Users/josh/Code/lattice-fit`, uncommitted tree included. `path:line` is under `crates/lattice-desktop/src/` unless prefixed `crates/` or `scripts/`.

## 1. What is being built, and why

Every source of the owner's .NET course is a captured web page (`kind = web`, 150 of 150). The library's BM25 + vector + RRF index ranks only `kind = document` snapshots (`features/learning/lessons/reference_collection.rs:236-291` maps library chunks back onto a snapshot's bytes); every other source is ranked by an in-memory BM25 that each lesson job rebuilds from `verification_sources` (`:188-234`, `features/learning/references/source_library/reads.rs:294`) and throws away. The course's pages have never been embedded. A private vector table once existed and was dropped when the library path arrived (`crates/lattice-desktop/migrations/20261009150000_learning_consolidation.sql:87-89`).

Explorer already has what a course needs: a per-folder index on disk under `<data_dir>/folder-index/<hash>/` with `chunks.db` (SQLite rows plus an FTS5 table) and a vectors file keyed by the embedding identity, built by a job on the app's job runtime, searched dense + `bm25` fused by the shared reciprocal rank fusion, kept out of `lattice.db` and backups (`features/explorer/index/mod.rs:7-11`, `store.rs:41-67`, `search.rs:144-204`).

The owner's decisions (2026-10-19): a per-course index partitioned by course id, modelled on Explorer's; the shared parts move into the infrastructure layer and both features use them, no copy; claim evidence is searched exactly the way the agents already search indexes, through one search stack and never a third implementation (§2.4); claim evidence pages are the section's own cited passages, then the next top results of one hybrid search across the course, page by page, each page sized by the measured-pace budget, then targeted web research when the results run out; no rings, no one-passage-per-source rule, no every-source-once. Whether the lesson's own sources are read first was open; the research findings are in §5.2 and the recommendation is in §9.

Hard rules kept throughout: generic for every backend and embedding model, no probing; no hand-picked counts, sizes, thresholds or timeouts (bounds come from the window, the measured pace, relevance signals or Stop); Explorer's existing limits unchanged; pre-release, so no shims and any schema change is a new dated migration; processes, evals and run history live in the repo.

## 2. Infrastructure extraction

### 2.1 Generic and Explorer-specific

| Explorer today | Generic (moves) | Explorer-specific (stays) |
|---|---|---|
| `store.rs` (646 lines): `meta`, `files(path,size,mtime,content_hash,embedded)`, `chunks(id,path,start_line,end_line,text)` with `AUTOINCREMENT`, `passages_fts` and its triggers (`:41-67`), `index_dir` (`:79`), `vectors_path` (`:90`), `replace_file`/`remove_files`/`pending_files`/`mark_embedded`/`reset_embedded`/`chunks_by_ids`/`full_text` (`:315-527`), `list_indexes`/`inspect`/`dir_bytes` (`:541-646`) | the store, generalised: `documents(key, stamp, content_hash, embedded)` and `passages(id, document, start, end, text)`; `start`/`end` are the owner's units (lines for a folder, bytes for a source); FTS5 unchanged; the directory registry (list, inspect, bytes) | the meta keys `root`, `too_large`, `complete` and their decoding; `files_under(prefix)` semantics become `keys_under(prefix)`, which a course passes `""` to |
| `run/embed.rs`: batches through the embedder, saves the vectors file every `save_every`, marks a file embedded only after a save holding all its chunks (`:67-160`) | the embedding loop, with the vectors file (`USearchVectorIndex`, already `infrastructure/vector_index`), progress reported through a small trait | `Indexer::advance` and the `FolderIndexStatusDto` it moves (`run.rs:204-215`, `dto.rs:8-22`) |
| `chunker.rs`: line-based, structure-aware, `indexed_text` heading and `body_of` (`:13-17`, `:42-47`, `:151-190`) | the chunk contract (`NewPassage { start, end, text }`) and two chunkers side by side: Explorer's line chunker as it is, and the byte-span chunker the reference collection already has (`reference_collection.rs:136-163`) | which chunker a feature calls |
| `build.rs`: `LiveIndex` (dir, store, engine, `writing` mutex, `:90-99`), `Engine` (`:82-86`), progress saved at most every second (`:31`), job config `Requeue`, `concurrency(1)`, `exclusive_per_subject` (`:57-61`), the handler (`:246-291`) | `LiveIndex`, `Engine`, the acquire/release registry of live directories, the progress saver, a `build_job_config()` both kinds use | `BuildRequest { root, index_root, wipe_prefix }` (`:36-46`), the open folder goes first (`:255-266`), `folder_missing`, `TooLarge` |
| `search.rs`: `fuse` through `ReciprocalRankFusion` at the library's weights (`:53-61`), `match_expression` (`:63-82`), dense scoped by a key set (`:207-226`), `search` taking either half's failure (`:144-204`) | `match_expression` and the FTS read as a `TextSearchPort` (`PassageTextSearch`); the vectors file already is a `VectorSearchPort`. The orchestration itself is not moved: it is the library's `HybridSearchUseCase` behind the core port (§2.4) | `offset`/`picked_relative` for a reused parent index (`:105-113`, `:229-237`); `fuse`, `dense`, `DENSE_CANDIDATES`/`LEXICAL_CANDIDATES` (`:33-34`) are deleted in favour of the library's orchestrator |
| `run/files.rs`, `run/sync.rs`, `watcher.rs`, `manager.rs`, `manager/folder.rs`, `tool.rs` | nothing | all of it: walking, ignore rules (`Scope`), size/mtime stamps, the file bound, ancestor reuse, the watcher, open/close/rebuild/delete, `search_folder` |

### 2.2 Target modules

`crates/lattice-desktop/src/infrastructure/passage_index/`:

- `mod.rs`: the module header (what an index directory holds; one `passages.db` plus `vectors-<identity>.usearch`), re-exports.
- `store.rs`: `PassageStore::open(dir)`, `documents`, `touch(key, stamp)`, `replace(key, stamp, hash, &[NewPassage]) -> Vec<i64>` (ids whose vectors the caller removes), `remove(keys) -> Vec<i64>`, `pending() -> Vec<key>`, `passages_of(key)`, `by_ids`, `mark_embedded`, `reset_embedded`, `full_text(match, scope, limit) -> Vec<(id, bm25)>`, `counts(scope)`, `keys_under(prefix)`, `meta`/`set_meta`/`delete_meta`; a `fts5vocab` view for term document counts (§4.2). Schema version in `meta`; a different version drops and recreates the tables.
- `vectors.rs`: `Vectors::open(dir, identity, dimension) -> Arc<USearchVectorIndex>` (a `VectorSearchPort`, searched through `search_scoped` with the key set), `add`, `remove`, `save`, `count`; the identity-change rule (a different identity drops the vectors file, chunks stay) as Explorer has it.
- `chunking/lines.rs` (Explorer's chunker, unchanged) and `chunking/spans.rs` (`span_chunks(text, chunk_chars, step_chars) -> Vec<NewPassage>`: the collection's 1,200-character windows every 900, cut at the nearest sentence end so a passage is whole sentences; the chunker the course index and `ReferenceCollection` both call, so an index hit is a collection span).
- `build.rs`: `Embedding { embedder, batch_size, save_every, embed_text: fn(&Passage) -> String }`, `embed_pending(store, vectors, embedding, cancel, &mut dyn Progress) -> Result<Embedded>`, `save`; `Progress { fn embedded(&mut self, count: usize); fn saved(&mut self) }`. `embed_text` lets Explorer keep its stored heading (identity) and a course prefix the source title at embed time.
- `live.rs`: `LiveIndex`, `Engine`, `LiveIndexes` (acquire/release by directory, as `manager.rs` keeps them today at `:108-160`).
- `text_search.rs`: `PassageTextSearch`, the store's FTS5 behind the core `TextSearchPort` (`match_expression` moves here). No search orchestration lives in this module: dense + lexical fusion is the one hybrid search of §2.4, and a `Scope` is the `allowed_document_ids` key set that both core ports already take.
- `registry.rs`: `list(base) -> Vec<Entry { dir, meta }>`, `inspect(dir)`, `dir_bytes`.
- `job.rs`: `build_job_config()` (`Requeue`, `concurrency(1)`, `exclusive_per_subject`), the progress saver.

No new core port is needed: features may depend on infrastructure (`scripts/rust-architecture-check/src/main.rs:588-597` forbids only domain→* and application→features/infrastructure/interfaces; `search.rs:17` already imports `infrastructure::vector_index`). The module imports only `lattice_core` and `crate::infrastructure`, so `infrastructure-imports-feature` stays at zero (`scripts/rust-architecture-check/src/ratchet.rs:484-487`). Explorer and learning each import the module and never each other, so `feature-edge` stays at zero (`:72`). No statics: `LiveIndexes` is a struct the composition root owns, as `FolderIndexManager` is (`interfaces/di/container.rs:476-485`). Every file above is well under 800 lines (`:112`); `store.rs` splits into `store.rs` and `registry.rs` on the way.

### 2.3 Explorer refit, no behaviour change

`FolderStore` becomes `PassageStore` plus Explorer's meta keys; `touch_file(path, size, mtime)` is `touch(path, "size:mtime")`; `replace_file` is `replace`; `files_under`/`paths_under`/`remove_under` are `keys_under`. `Indexer` keeps `scope`, `prefix`, `limits`, the walk and the sync (`run/sync.rs:26-123`, `run/files.rs:62-200`) and calls `passage_index::build::embed_pending` with Explorer's `Limits` as the `Embedding`'s batch and save sizes, implementing `Progress` with today's `advance`. `FolderSearch` wraps the `Arc<dyn HybridSearchTrait>` the factory builds for the index and keeps `offset`; `search_folder` is untouched. `manager.rs` uses `live::LiveIndexes` and `registry` in place of its own maps and `store::list_indexes`. `watcher.rs`, `tool.rs`, `dto.rs`, `plugin.rs` do not change.

One visible consequence: the generalised table names mean a schema version bump, so each existing folder index rebuilds once on its next open (Explorer already has `META_VERSION` for this, `store.rs:36`). Pre-release, data deletable; stated here so no one hunts for a regression.

Gates: Explorer's tests (`features/explorer/index/tests.rs`, `tests/folders.rs`) pass unchanged except for store construction; `scripts/check-rust-layer-boundaries.sh` reports no new entry under any ratchet.

### 2.4 One query path

The owner's expectation: claim evidence is searched exactly the way the agents already search indexes; one search stack in infrastructure, reused everywhere, never a third implementation.

Today there are three orchestrations over the one fusion. The library's `HybridSearchUseCase` (`features/search/use_cases/hybrid_search.rs:99-110`): `search_branches` joins the vector branch at `HYBRID_VECTOR_FLOOR` and depth `limit × BRANCH_DEPTH`, the text branch and the sparse branch; `fuse` builds the weighted rankings, records each result's best rank and score per branch (`BranchSignals`), and lets one document place `PER_DOCUMENT_CAP` passages before the rest follow in fused order (`:78-88`). The folder index's `FolderSearch::fuse` and `dense` (`explorer/index/search.rs:53-61`, `:144-226`). Memory recall's (`conversation/repository/memory_semantic.rs:125-145`). The lesson collection's interleave is a fourth ranking that is not fusion at all. The chat's agents and the retrieval planner already reach the library through a core port, `LibrarySearchTrait` (`crates/lattice-core/src/application/ports/library_search.rs:19-42`: `execute_scoped`, `search_branches`, `fuse`, `rerank`), implemented by `HybridSearchUseCase` and consumed by `function_calling/executor.rs:90,126`, `conversation/chat/retrieval/corpus_plan/library_search.rs:24-245`, `shortlist.rs:40` and `compare`'s one adapter (`compare/scoped_search.rs:31-41`).

**The single path is that port.** `HybridSearchUseCase` is already generic over its two storage ports: `VectorSearchPort::search_scoped(embedding, top_k, threshold, allowed_document_ids)` (`ports/vector_search_port.rs:105-110`), which `USearchVectorIndex` implements and the folder index already calls with a key set (`search.rs:21`, `:216-221`), and `TextSearchPort::search_scoped(query, top_k, space_id, allowed_document_ids)` (`ports/text_search_port.rs:94-99`), implemented today only by `SqliteTextSearch` over `lattice.db` (`search/engine/text_search/sqlite_text_search.rs:183`). So:

- The port is renamed `HybridSearchTrait` (`ports/hybrid_search.rs`), since it serves folders and courses too: a mechanical rename across its six call sites, pre-release, no alias.
- `PassageTextSearch` (§2.2) implements the port's reads over `passages_fts`, `allowed_document_ids` being the key set. The port's write methods (`index_document`, `index_batch`, `remove_document`, `clear`, `text_search_port.rs:135-193`) belong to the library indexer and `HybridSearchUseCase` never calls them; `PassageTextSearch` returns an error naming the store from them, and a follow-up splits the port into read and write halves so that error goes away.
- A `HybridSearchFactory` port beside it: `fn hybrid(&self, embedder: Arc<dyn EmbeddingPort>, vectors: Arc<dyn VectorSearchPort>, text: Arc<dyn TextSearchPort>) -> Arc<dyn HybridSearchTrait>`, implemented by the search feature as `HybridSearchUseCase::new(embedder, vectors, text)` with no sparse branch, reranker or chunk repository (library stages), and wired in the container beside `EmbedderSource` (`interfaces/di/container.rs:476-485`) into Explorer's manager and learning's `CourseIndexes`. Neither feature imports `features::search`; `feature-edge` stays at zero.
- `FolderSearch` keeps `offset` and `picked_relative` and calls `execute_scoped(SearchRequestDto::hybrid(query, top_k), None, Some(keys))`; its `fuse`, `dense`, `DENSE_CANDIDATES` and `LEXICAL_CANDIDATES` go, and the library's `BRANCH_DEPTH` and `HYBRID_VECTOR_FLOOR` take their place. That is the one change in what Explorer's search returns, and it is the point. `search_folder` is unchanged above it.
- `ReferenceCollection::rank` (§4.1) calls `search_branches` for the claim and for its counter query and one `fuse` over all the branches (§5.5); `Ranked` carries the per-branch rank and score `fuse` already records on each result.
- With the port come the library's rules, as the agents get them: the per-document cap reorders within a list (a source's fifth passage follows once every source has had its turn; nothing is dropped), the vector floor keeps out noise, and the weights are the shared constants. The eval's `rank_in_course` column (§7) shows the cap's effect on the owner's course; if it costs recall there, `SearchRequestDto` gains a field the checker sets to rank without it, a decision for then with numbers.

Memory recall keeps its own orchestration for now, since it ranks memory rows rather than passages; it is a follow-up (§6).

**The library's store.** Converging the query path is done above; converging the store is not. The library's index is a `USearchVectorIndex` at a path (`features/search/di.rs:141`) with compression, a rescore store and dimension metadata (`infrastructure/vector_index/`), chunk rows and FTS in `lattice.db` (`SqliteTextSearch`), a sparse term store, `document_chunk_stats`, the note index sink, and a backup that carries `lattice.db` whole. Moving it onto `passage_index` means moving chunks out of `lattice.db` into a directory, re-pointing indexing, batch, web ingestion, the sparse store and the backup, and rebuilding every library on upgrade: weeks across five features for one fewer store driver, with no change to what any caller sees. Recommendation (D4): a follow-up after the course index has run live. Two store drivers behind the same two ports and the same orchestrator is not where duplication has cost; a third orchestrator was.

## 3. The course index

### 3.1 Location and keying

`<data_dir>/course-index/<program_id>/passages.db` and `vectors-<identity>.usearch`. A program id is already an opaque UUID, so no hashing (Explorer hashes a folder path because the path says something, `store.rs:77-88`). One directory per course: two courses never share a store or a vectors file, so two courses never see each other's vectors; deleting a course is removing a directory. Documents are keyed by **source version id** (`learning_source_versions.id`, the immutable snapshot `ReferencePassage.source_id` already names, `reference_collection.rs:31-33`): a new version is a new key, so refresh and reimport need no in-place update. Passages are byte spans into `full_text`. Meta: `program`, `identity`, `schema`.

The folder index stays at `folder-index/`; moving it would rebuild Explorer for no gain.

### 3.2 What is indexed

The active, non-deleted version of every source of the program, exactly the set `verification_sources` returns (`reads.rs:294-313`), including `kind = document` snapshots. Decision (D3, recommended yes): documents are indexed again per course rather than searched through the library and fused.

- For: one ranking over one corpus on one score scale, which is what makes fusion legitimate (the fusion module's own note explains why disjoint lists must not be fused, `fusion.rs:11-13`); offsets land in the snapshot's bytes directly, with no `locate_chunks` relocation that silently falls back when the library's copy drifts (`reference_collection.rs:165-180`, `:254-266`); the course index works with the library closed, mid-import or absent; `ReferenceCollection` loses its `LibraryPassagesPort` dependency, `library_spans`, `library_documents` and `mode()`'s `hybrid`/`lexical_fallback` split (`:82-97`, `:293-307`).
- Against: a document's text is embedded once by the library and once per course that cites it. Document sources are a minority of course sources (none in the owner's course), and the cost is bounded by the text's size and paid once per version.

`LibraryPassagesPort::document_text` stays where it is used: capturing a library document into a version (`features/learning/references/sources.rs:27`).

### 3.3 Chunking aligned with the checker

The index passage is the collection's span: 1,200 characters stepping 900 (`reference_collection.rs:16-23`), now cut at sentence ends by `chunking/spans.rs`, and the checker widens a hit exactly as today: 300 bytes to paragraph edges (`merge`, `:378-392`) and 1,700 characters each side merged (`verification_context`, `:629-699`). A hit maps onto one `Span { source, start, end }` with no relocation. The embedded text is `"{title}\n{chunk}"` through `embed_text`; the stored text is the chunk alone. The existing widths are not redesigned here; the eval (§7) reports passage tokens and can motivate a change later.

### 3.4 The build, and every lifecycle event

A `CourseIndexes` manager in `features/learning/references/course_index/` (`mod.rs`, `manager.rs`, `job.rs`, `sync.rs`, `search.rs`) modelled on `FolderIndexManager`: `acquire(program) -> Arc<LiveIndex>`, `ensure_current(program, cancel, progress) -> Result<IndexMode>`, `search(program) -> CourseSearch` (the `Arc<dyn HybridSearchTrait>` the `HybridSearchFactory` builds over that course's vectors and `PassageTextSearch`, §2.4), `delete(program)`, `sweep()`. `ensure_current` is one idempotent step: read the active version set, diff it against the store's keys, remove extra keys (passages and vectors), add missing ones (chunk, insert, FTS by trigger), then `embed_pending`. It is the same code whether a background job or the lesson job runs it, and it holds `LiveIndex.writing` for its run.

| event | what happens |
|---|---|
| capture: `add_by` (`source_library/capture.rs:10`, insert `:108`), research saves (`content_verification/research.rs:528`) | the use case that called the repository marks the course stale: `CourseIndexes::refresh(program)` submits a `learning.course_index` job (`subject_id = program`, operation id `course-index:{program}:{revision digest}` so repeats coalesce, `crates/lattice-core/src/shared/runtime/jobs/repository.rs:243-280`), unless a lesson job holds the index, in which case that job's next `ensure_current` picks the change up |
| refresh produces a pending version (`capture.rs:202`, `:361`) | nothing: pending versions are not active |
| adopt, reimport (`source_library/lifecycle.rs:6`, `:206`) | stale → job: the old version's key is removed, the new one added |
| delete (`lifecycle.rs:144`, `deleted_at`) | stale → job: the key is removed |
| freshness `before_use` refresh at lesson start | the lesson job's `ensure_current` after the refresh, as `reload` runs per lesson today (`lessons/generation_jobs.rs:277-283`) |
| course deleted | the delete use case calls `CourseIndexes::delete(program)`; `sweep()` at startup removes `course-index/<id>` directories whose program no longer exists (cascades already remove the rows, `20261016100000_learning_delete_cascades.sql`) |
| embedding identity changes | the vectors file is keyed by identity; a different identity drops the old file, `reset_embedded`, re-embeds; chunks stay (Explorer's rule, `manager.rs:24-26`) |
| crash mid-build | `RecoveryPolicy::Requeue`; the store is the checkpoint: `AUTOINCREMENT` ids, a document marked embedded only after a save that holds all its passages (`run/embed.rs:63-66`) |
| index not current when a lesson job starts | the lesson job runs `ensure_current` inline under its own cancellation, in the phase that today says "Matching saved references to the library" (`generation_jobs.rs:243-253`), with stage text "Indexing n new references · p passages/s"; a background build holding `writing` is waited for, with stage text saying so; the wait is bounded by that build's remaining work, which is the same work |
| no embedding model loaded | FTS only. `IndexMode::Lexical` is returned, said in the stage text, written into the report's retrieval metadata where `mode()` goes today (`reference_collection.rs:293`), and part of the selection key (`evidence_selection.rs:236-238`) |
| embedding fails part-way | the unembedded passages stay pending and FTS still covers them; `IndexMode::Partial { embedded, total }` is reported the same three ways; the next `ensure_current` retries. Never silent, never a stop |
| pack export/import (`features/learning/portability/`) | nothing is exported: the index is rebuilt from `learning_source_versions.full_text`, which the pack carries as `SourceBody` entries when the user confirms redistribution (`pack.rs:239-254`, `:344`); an imported course is indexed lazily on first use. Without bodies there is no text, exactly as the keyword path has no text today |
| backups | excluded by construction: the archive carries `db/`, `files/`, `vault/`, `web-archive/`, `web-snapshots/`, `chat-images/` (`features/backup/archive/format.rs:46-67`) and nothing under `course-index/`, as nothing under `folder-index/` |

No migration: the index lives on disk and `learning_source_versions` already holds everything it is built from. The dropped `learning_source_retrieval_index` table is not resurrected.

### 3.5 Concurrency with Explorer

Two job kinds, `explorer.folder_index` (`build.rs:27`) and `learning.course_index`, each `concurrency(1)` and `exclusive_per_subject` (`jobs/mod.rs:229-257`), so two builds of one course never overlap and a course build never queues behind a large repository walk. The embedding service is one loaded model shared through `EmbedderSource` (`manager.rs:52-56`; the app's implementation `features/explorer/plugin.rs:131`, which moves to a neutral home, `features/embedding/`, so learning can take the same `Arc<dyn EmbedderSource>` without a feature edge); two concurrent builds share its throughput and nothing else. The lesson job's inline `ensure_current` and a background course build serialise on `LiveIndex.writing`.

## 4. Retrieval API

### 4.1 `ReferenceCollection` on the index

`ReferenceCollection` keeps `sources` (title, url, `excerpt` = full text, `acquired_at`, `dto.rs`), `cited_passages`, `verification_context`, `catalog`, `mentions_subject_of`, and loses its retrieval internals:

| today | replaced by |
|---|---|
| `load(pool, program, sources, library)` (`:236-291`) and `reload` (`:309-316`) | `open(index: &CourseSearch, sources) -> Self`; callers re-open after `ensure_current` (`generation_jobs.rs:247`, `:281`; `claim_reviews/attach.rs:243`; `recall/recall_repository/selectors.rs:148`; `research.rs:511`) |
| `lexical(sources)` (`:183`) | kept, same signature: an ephemeral in-memory `PassageStore` (`sqlite::memory:`, FTS5) built from `sources`, no vectors. Its twelve production callers (`planning/outline_repair.rs:196,297,574`, `outline_review.rs:446`, `outline_linking.rs:496`, `outline_references.rs:130`, `diagnostic_generation.rs:57`, `assessment/assessment_generation.rs:220`, `lessons/teaching.rs:329`) do not change |
| `keyword`, `Chunk`, `frequencies`, `average_length`, `keyword_ranking` (`:188-234`, `:514-548`) | FTS5 `bm25()` in the store |
| `retrieve(query, limit)` and `merge` (`:322-420`): two disjoint lists interleaved by rank, `score = 1/(60+rank+1)` | `rank(queries, scope, depth) -> Vec<Ranked>`: `HybridSearchTrait::search_branches` per query and one `fuse` (§2.4), each hit widened to paragraph edges and overlap-deduped as `merge` does today; `Ranked = ReferencePassage` with `score` = the fused score and the per-branch rank and score the fusion recorded. No lesson-specific merge remains |
| `Branch { Library, Keyword }`, `branch_score` (`:55-60`) | `Branch { Dense, Lexical }` kept as diagnostics only (`branch_ranks` by query and branch); `ClaimChecks::key`, the selection digest and the receipt exclude them as they exclude `score` (`content_verification/mod.rs:213-224`, `evidence_selection.rs:206-222`) |
| `library_spans`, `library_documents`, `locate_chunks`, `ranks_through_library`, `embedding_model` | gone; `mode()` returns the `IndexMode` (§3.4); the embedding identity comes from the index |
| `retrieval_kind` = `hybrid` / `lexical_fallback` per list | `hybrid`, `lexical` or `partial` per the index mode, one value per run |

`Scope` is `All` or a set of version ids, so a caller can search the lesson's cited sources, the sources a research round added (`evidence_selection.rs:300-333` uses this), or the course.

### 4.2 Subject terms from the index

`mentions_subject_of` (`reference_collection/pages.rs:51-100`) needs each term's passage count across the collection. The store exposes it from a `fts5vocab(passages_fts, 'row')` table (`doc` per term) instead of the in-memory `subject_frequencies`; the query side keeps `content_terms`. The rule itself (terms no commoner than the median of the claim's own, dropping terms in no passage or in every passage) does not change.

### 4.3 Deterministic retrieval for tests and the model-free eval

Tests keep `ReferenceCollection::lexical(sources)` (`evidence_paging_tests.rs:113-135` fixtures and every `related(i)` source): FTS5 `bm25()` is deterministic and needs no model. A test that wants vectors uses Explorer's `FakeEmbedder` pattern, hashed-word vectors (`explorer/index/tests.rs:80-115`), moved to `infrastructure/passage_index/testing.rs` behind `cfg(test)` so both features share it. The eval runs BM25-only by default and with the configured embedding model when told where it is (§7).

## 5. Claim pages

### 5.1 Two passes, not one page

Research findings (forwarded 2026-10-19, sources in §10): every attribution benchmark checks a claim against the cited source alone (ALCE, AttributionBench, AttrScore, TRUE/AlignScore/MiniCheck; the 2025–26 report evaluators do the same); support found in another source means the claim stands and the citation should be repaired (SIDE: a better citation preferred 70% of the time); writers' citations often do not support their claims (SourceCheckup 50–90% not fully supported; up to 57% post-hoc); and evidence selection is where confirmation bias acts, so the attribution check must not be mixed with retrieved passages.

**Pass A, attribution.** The claim against the passages its own section cites (`Citations::of`, `citations.rs:83`; `take_cited` while a check of them alone fits), nothing else. Supported means the citation is right. A section that cites nothing has no pass A. The comparison key is `ClaimChecks::key(unit, claim, cited)` and is stable across runs for as long as the citation is, so A reuses its receipt without `evidence_selection::select`.

**Pass B, corroboration.** The claim against the course by relevance: one hybrid search with the claim itself as the query (the AVeriTeC 2025 winner's practice; decomposition did not help in MiniCheck) plus its counter-framed query (§5.5), fused into one list, read page by page, each page the next slice of the list sized by the budget (§5.3), excluding anything A showed. Support found here means the claim is supported *and* its citation is wrong: the finding carries `supported_by` (source, offsets), the ledger records `SupportedElsewhere { cited, found }`, and the section repair (`content_verification/repair.rs`, which already groups failed claims with the evidence their check used) rewrites that block's `quote`/`sourceIndex` to the found passage; the next pass's `section_citations` (`citations.rs:93-124`) then makes it pass A's passage. A contradiction found in B is flagged even when A passed: it enters the provenance gate as a contrary passage, where the overturn rule already decides (`provenance_gate.rs:66-68`); the user sees both passages.

How much of B runs for a claim A supported is decision D1. Recommended: B's first page for every claim, packed into the same request as its A comparison (one prompt, two answers; `render_batch_evidence` interns shared texts and keeps each item's own evidence ids, `crates/lattice-core/src/application/services/claim_verification/batch.rs:26-56`). The added cost is one answer per claim, about 130 output tokens or 11 s on the logged run; what it buys is the contradiction check and the citation repair. B continues past page 1 only while the claim is unsupported: the judge's verdict after each page is the sufficiency signal (Sufficient Context; FIRE's verify-or-search interleaving), and that is the only escalation rule.

### 5.2 Order inside pass B: the open question

The ordering of B's list is one isolated choice, a `Reach` sequence the walk steps through:

```
enum Reach { LessonSources, Course }          // B's scopes, in order
const REACHES: &[Reach]                        // the one line that holds the decision
```

- (a) pure relevance: `[Course]`. One fused list over the whole course.
- (b) lesson's sources first: `[LessonSources, Course]`. The same search scoped to the versions the lesson cites (`Citations::cites`, `citations.rs:88`) until that list runs out (§5.4), then the course excluding what was shown.
- (c) tie-break only: `[Course]` with lesson-cited sources ahead at equal fused score.

The research recommends (a): no study shows that boosting cited or provenance documents improves verification; SciFact-Open found evidence the original annotation missed once the corpus was opened, and small-corpus systems lost at least 15 F1; and the 62-of-86 figure that favoured the boost was measured with the boost on and keyword-only ranking, so it cannot separate "best evidence" from "ranked first". (c) is harmless and has no evidence behind it. §7 measures all three on the owner's course before the switch is set for good; recommendation in §9 (D2).

### 5.3 The page budget, and before it is measured

A B page holds the reading that costs as long as one more answer: `tokens per page = answer tokens per claim × prompt tokens/s ÷ output tokens/s` (`evidence_checks/page_budget.rs:54-63`), both rates the backend's own as `LLMPort::pace` reports them from the app's requests (`crates/lattice-core/src/application/ports/llm_port.rs:259`, `llm_port/pace.rs:25-28`), the answer size this job's running mean (`AnswerSize`, `page_budget.rs:12-40`). A page is the longest prefix of the list whose passages, widened, fit the budget; a passage is never split; the window stays the hard bound and part 1's halving and shrinking still answer a backend that counts past it (`judge.rs:50-129`). Pass A never needs a budget.

Before an answer is measured, derived not picked, in this order:

1. Pass A requests go first. They need no budget, and their answers measure the job's answer size, so by the time any B page is built the mean exists in every lesson with at least one cited claim.
2. If no A comparison ran (a lesson whose sections cite nothing), the answer size is the checker's output cap, the largest an answer can be (`ClaimChecker::new(.., 512, ..)` as `pack` constructs it, `evidence_checks.rs:455-460`). The first measured answer replaces it.
3. The pace is known once the backend has served one request with timing, which every lesson job has done long before claim checking. A backend that reports no timing and streams nothing to time (`pace.rs:1-3`) leaves pace `None`; then a B page is bounded by the window alone, as a request is today, and the per-round log and the report say "page budget unavailable: no measured pace". That is the one case without a budget, and it is visible.

### 5.4 Results run out

The list for a reach is the fused ranking, in order, minus passages overlapping what the claim was shown. Its end is set by content, not by a count or a score: the last position whose passage mentions the claim's subject (`mentions_subject_of`, `pages.rs:51-100`). Retrieval fetches a depth and doubles it while the fetched tail still ends on-subject or until the scope has no more passages, so the depth is found, not chosen; raw branch scores and fused scores are logged at the end of each list and never used to stop (RRF scores have no absolute meaning; the gap rule of the pages design is withdrawn). Pages are consecutive budget slices of the list; passages before the end are shown in fused order whether or not they mention the subject, so a passage the dense branch ranks high without sharing a term (3 of 90 in the eval) is reachable, which settles the pages design's D5. When the cursor passes the end, the reach is exhausted; when `REACHES` is exhausted, the claim goes to the targeted web research of the pass that follows (`repair_loop.rs:118-175`, `research::expand`, `research.rs:196`), with nothing more read from the course. A claim with no distinctive term has an empty list and goes there at once, as its walk ends today.

The group stop of deepening (another round only while the last settled a claim, `deepening.rs:264-266`) is removed: the list's end bounds the walk, and each page's verdict is the interleaving the evidence favours. The cost this leaves open is a long on-subject list; §7 reports its distribution on the owner's course, and if a tail turns out long the owner decides with those numbers rather than this design picking a cut. Conformal calibration to a target miss rate is the principled cut if one is ever needed; it needs a labelled claim→evidence set and a target miss rate, both owner decisions.

### 5.5 The counter query

Support-, refute- and neutral-framed queries retrieve about half different evidence, and HyDE-style query generation biases stance unless both stances are generated. The counter query stays, model-free and as the pages design builds it (`counter_query`, `pages.rs`: the claim's subject terms without its value terms, or the terms without a negation marker), but it is no longer a separate page: its branches from `search_branches` join the claim's in the same `fuse` call at the same weights, the multi-query fusion the library already does for planned queries. A passage both framings find rises; one only the counter framing finds enters the list where fusion places it. The value-term re-ordering (`ValueTerms`, `pages.rs:314-360`) goes, since one fused order is the point. A claim with no value terms has no counter query.

### 5.6 What carries over

- Receipts and reuse: `ClaimChecks::key`, `claim_receipt_key`, `ClaimReceipt.shown` unchanged (`mod.rs:213-224`, `:285-298`); each B page keeps its own receipt as deeper pages do (`deepening.rs:186-195`); `claim_reuse::legacy_receipt`, `identities`, `settled_finding` untouched. `RANKING` is bumped once to `two-pass-hybrid-v1` (`evidence_selection.rs:17`); `select` restores B's first page across runs while the sources are unchanged, and research-led pages search the added versions through `Scope::Keys(added)`.
- Part 1: `PromptExceedsWindow`, halving, shrink-and-bind unchanged.
- Reading order: `reading_order` (`mod.rs:534-545`) stays for B pages; an A page is short enough that order does not matter and keeps the canonical order.
- Packing: `pack` by section and `batch_fits`, no item cap (`evidence_checks.rs:451-481`); A and B1 items of one claim go in the same request.
- Progress: `begin_checks`, `plan_model_checks`, `checked`, `deeper_page` (`lesson_progress.rs:213-275`) unchanged; stage texts name the pass ("Checking n claims against their citations", "Corroborating n claims across the course, page p").
- Logging: one `tracing::info!` per round, per claim: pass, reach, page, passages, exact tokens, budget, fused score of the page's first and last passage, list end reached, counter branches present.

### 5.7 The part 2 branch's code

| piece | fate |
|---|---|
| `Stage`, `RingPage`, `ring_page`, `best_per_source`, `had_turn` (`citations.rs:128-338`) | deleted |
| `next_by_source`, `above_cut`, `gap_cut`, `gap_is_clear`, `Gaps`, `largest_gap` (`pages.rs:100-300`) | deleted; replaced by `ReferenceCollection::rank` and the list end rule |
| `counter_query`, `ValueTerms::of_passage` | query construction kept; re-ordering deleted |
| `Walk { stage }`, `next_page`, `deepen`'s group stop (`deepening.rs`) | `Walk { reach, list, cursor, shown }`; `next_page` takes the next budget slice; no group stop |
| `page_one` = cited + ring 1 (`mod.rs:493-531`) | pass A (cited only) and B page 1 as separate comparisons |
| `Branch`/`branch_score` on `ReferencePassage` | kept as `Branch { Dense, Lexical }` diagnostics with `branch_ranks`, `similarity`, `bm25` |
| `reading_order`, `pack`, `AnswerSize`, `page_budget`, `PageNote`-style round log, `walk_pages`, `live_page_recall_eval.rs` | kept and reshaped |
| `keyword_ranking`, `merge`'s interleave, `load`'s library mapping | deleted (§4.1) |

## 6. Other consumers of course sources

The index serves any caller that has a program id and a query; `Scope` serves the ones that want a subset. Follow-ups, one line each, not designed here:

- Lesson writer's evidence (`lessons/generation/evidence.rs:164-172`, `:206`): `rank` over the course instead of `retrieve`; its sizing needs breadth, so its rule differs.
- Outline evidence (`planning/outline_evidence.rs:75`, `window / 4`): `rank` scoped to the course; replace the window fraction.
- Teaching review (`lessons/review_evidence.rs:180`, `:194`): `rank` within the pace budget per issue.
- Prior-lesson recall (`lessons/generation/recall.rs:273`): `Scope::Keys(prior lessons' sources)`.
- Claim review attach and recall selectors (`claim_reviews/attach.rs:243`, `recall_repository/selectors.rs:148`): `open` in place of `load` (done in PR 3, mechanical).
- The references workspace search (`source_library/reads.rs:206`): could rank through the index instead of SQL `LIKE`.
- Memory recall (`conversation/repository/memory_semantic.rs:125-145`): route through `HybridSearchFactory` with its own two ports and delete its fusion wrapper.
- The library's store onto `passage_index` (§2.4, D4): after the course index has run live.
- `TextSearchPort` split into read and write halves, so `PassageTextSearch` implements only what it has.

## 7. Evals

All in `crates/lattice-desktop/src/features/learning/lessons/live_page_recall_eval.rs` (`#[ignore]`, no model request), documented in `evals/grounding/README.md` (§"Page recall", `:150-215`, rewritten for the index), results in `evals/grounding/results/<date>-page-recall-<order>-<retrieval>.jsonl` with a `results/README.md` run-history table (date, database, program, order, retrieval, embedding identity, pace, summary numbers, commit).

1. **Page recall on the index.** Reads the stored run as today (`learning_lesson_verifications.report_json`, the program's versions; database opened read-only), builds a temporary course index in a scratch directory from those versions, and for every approved finding reports: whether pass A would have settled it (the judge-cited passage lies in its section's citations); the rank of the judge-cited passage in B's fused list; the page that holds it, tokens read to it, and modelled seconds with `LATTICE_PAGE_RECALL_PACE`; the reach it was found in; the list's length and tokens (the run-out cost of §5.4); and `null` when no page holds it (the claim would have gone to web). `LATTICE_PAGE_RECALL_ORDER=relevance|lesson-sources|tie-break` runs the three orders of §5.2 in one invocation and prints them side by side.
2. **Hybrid against BM25-only.** `LATTICE_PAGE_RECALL_EMBEDDING=<model directory>` loads the configured embedding model through the app's own loader and builds vectors; absent, the index is FTS only. The same per-finding rank and page fields under both give recall at page 1, mean reciprocal rank and the count never reached. The 3 of 90 passages sharing no distinctive term are the first thing to look for in the hybrid column.
3. **Live run** on the owner's course once PR 4 lands: the report's tallies (supported by citation, supported elsewhere with citation repaired, contradicted, needs review, web-researched), per-round log and wall time against the 2026-10-19 run; every newly supported claim cites a passage on its page (the receipt invariant); every newly unsupported claim is checked against the page-recall report. `live_claim_pairs_eval.rs` runs once to show the judge did not move; `live_small_lesson.rs` stays the end-to-end control.

## 8. Implementation plan

Part 1 (#114) merges first. Each PR is shippable with its own tests and ends with the full suite (lib, bins, tests) after its last change. Builders work in separate worktrees, at most three at once.

| PR | owns | depends on | contract it fixes | gates |
|---|---|---|---|---|
| **1 Measured pace** (carved out of this worktree's uncommitted work) | `crates/lattice-core/.../ports/llm_port.rs`, `llm_port/pace.rs`, `shared/text`; `features/llm/{cloud,engine,instances}/*` timing capture; `claim_verification.rs` `Answered<T>` (`:20-25`) and `check_passages_or_size_refusal` (`:242-262`); `content_verification/evidence_checks/page_budget.rs`; the `AnswerSize` accumulation in `group` | #114 | `LLMPort::pace() -> Option<Pace>`, `CompletionResponse.timing`, `page_budget(pace, answer) -> Option<usize>` | adapter tests show timing captured per backend; `page_budget` tests; the pages design's `live_page_recall_eval` pace modelling still runs |
| **2 Passage index in infrastructure, Explorer refit** | new `infrastructure/passage_index/*`; core: `LibrarySearchTrait` → `HybridSearchTrait`, new `HybridSearchFactory` port, both implemented in `features/search`; `features/explorer/index/*` refit with `FolderSearch::fuse`/`dense` deleted; `features/embedding/` home for `EmbedderSource`; container wiring | #114 | the API of §2.2 and the ports of §2.4 | Explorer's tests green (its search now answers through the library's orchestrator; a test pins that `search_folder` results come from `HybridSearchTrait`); layer check with no new ratchet entry; a folder index rebuilds once and reports as before |
| **3 Course index** | `features/learning/references/course_index/*`; `reference_collection.rs` (`open`, `rank`, `lexical` ephemeral, library path removed); callers (`generation_jobs.rs`, `claim_reviews/attach.rs`, `recall_repository/selectors.rs`, `research.rs:511`); stale marks at the capture, adopt, delete, reimport call sites; `interfaces/di/container.rs` wiring beside `folder_index`; startup sweep | 2 | `ReferenceCollection::open/rank/lexical/mode`, `Ranked`, `Scope`, `IndexMode` | FakeEmbedder tests: build, resume after a cut, identity change re-embeds, delete removes, sync removes a replaced version, FTS-only mode reported; `reference_collection/tests.rs` fixtures unchanged; `live_small_lesson` control |
| **4 Two-pass claim pages** | `content_verification/*` (`evidence_checks.rs`, `evidence_checks/deepening.rs`, `citations.rs`, `mod.rs`, `evidence_selection.rs`, ledger outcome `SupportedElsewhere`, `repair.rs` reason kind); `reference_collection/pages.rs`; `live_page_recall_eval.rs`; `evals/grounding/README.md`, `results/README.md` | 3 (starts against 3's contract over `lexical`) | the report's new fields (`supported_by`, pass, reach, index mode) | tests: A is cited-only and keyed without `select`; B1 excludes A's passages; A and B1 pack into one request; a page never exceeds the budget while fitting the window; the list ends at the last on-subject passage and the claim then reaches research; a supported-elsewhere claim repairs its citation; a B contradiction after an A support reaches the gate; A's receipt survives a ranking change; `RANKING` bump rebuilds B1 once; the model-free eval written to `results/` |

Order: PR 1 and PR 2 in parallel (two builders), then PR 3 and PR 4 in parallel (PR 4 against the contract, finishing after 3 merges), with the live run after 4. The uncommitted rings code on `feat/claim-evidence-pages` is not merged as it stands: PR 1 takes the pace pieces, PR 4 takes `reading_order`, `pack`, the diagnostics fields, `counter_query`, `walk_pages` and the eval file, and the rest is deleted with the branch once PR 4 lands. The pages design document stays as history with this document's note at its top.

## 9. Owner decisions

Taken by the owner, 2026-10-19:

- **D1 = every claim.** Pass B's first page runs for claims pass A supported too, packed with A, to catch a contradiction anywhere in the course and repair weak citations.
- **D2 = (a) pure relevance**, per the research findings (§5.2). The three-way eval of §7 confirms it before the constant is final, with the result recorded in `results/README.md`.
- **D3 = per course.** Document-kind sources are indexed again in each course's index (§3.2).
- **D4 = later.** The library's store converges onto `passage_index` as a follow-up, after the course index has run live. The query path converges now.
- **D5 = break-even ("rent or buy"), decided from the eval.** How far pass B reads before an unsupported claim goes to web research is one switch (`B_DEPTH = Depth::BreakEven`, `evidence_checks/walk.rs`). Every claim reads page 1 (D1). A claim page 1 leaves unsupported reads page k+1 only while what pass B has spent on it past page 1, plus the cost of page k+1, stays at or below W, the measured cost of researching one claim on the web; otherwise it goes to the targeted web research. A page costs its tokens ÷ the measured prompt rate + the job's answer tokens per claim ÷ the measured output rate (`page_seconds`, the measurements `page_budget` uses). W is measured, never chosen: each claim research round (`research::expand`) times itself from its start to its result (saved, unchanged or exhausted) and records elapsed ÷ the claims it researched on every claim's `Researched` ledger event (`round_ms_per_claim`). A job's walks weigh its own latest round, else the ledger's latest from any program, since W describes the backend and the network, not the course. While W or the pace is unmeasured (a fresh install, a backend that reports no timing), a claim reads page 1 only, and its research round is measured for next time. The page-recall eval on the owner's lesson (hybrid; pace 850 prompt and 11.3 output tokens/s, 130 answer tokens a claim; `LATTICE_PAGE_RECALL_RESEARCH_SECONDS`) chose it: to the list's end, 64 claims found in the course and 7 sent to the web, the worst claim 27 min in pass B and about 138–145 min in all at W 60–120 s; break-even at W 60 s, 50 found and 21 sent, worst 1.4 min, 70 min; at W 120 s, 60 found and 11 sent, worst 2.3 min, 87 min; the first page only, 32 found and 39 sent, worst 0.4 min. The first-page and list-end rules stay as the eval's floor and ceiling.

## 10. Research sources (forwarded 2026-10-19)

Attribution checks the cited source alone: ALCE https://arxiv.org/abs/2305.14627; AttributionBench https://arxiv.org/abs/2402.15089; DeepResearch-ReportEval https://arxiv.org/abs/2510.07861; ReportBench https://arxiv.org/abs/2508.15804. Better citations: SIDE https://arxiv.org/abs/2207.06220. Confirmation bias in evidence selection: "Failing to Falsify" https://arxiv.org/abs/2604.02485; stance-framed retrieval https://arxiv.org/html/2506.21745v1. Open corpus: SciFact-Open. Citation quality: SourceCheckup; Wallat. Claim as query, no decomposition: MiniCheck https://arxiv.org/abs/2404.10774. Fusion: Bruch et al. https://arxiv.org/abs/2210.11934 (convex combination needs a labelled weight; RRF until one exists). Sufficiency as the stop: Sufficient Context https://arxiv.org/abs/2411.06037; FIRE https://aclanthology.org/2025.findings-naacl.158. More retrieval is not better: "Cited but Not Verified" https://arxiv.org/abs/2605.06635. Calibrated cuts need a labelled set: https://arxiv.org/abs/2511.17908. Hand-set thresholds not copied: CRAG, Self-RAG, Adaptive-k's buffer and bottom-10% cut.
