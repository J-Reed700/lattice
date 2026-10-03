# Folder index: search the open folder by meaning

2026-10-02. Builds on `2026-10-02-explorer.md`. The Explorer's tools find text
the model can guess (`search_files` is a grep). This adds a semantic index of
the open folder so "where do we handle retries?" finds the code even when it
never says "retry".

## Decisions

- **Its own store, outside `lattice.db`.** One directory per indexed folder:
  `<data_dir>/folder-index/<first 16 hex of sha256(canonical root)>/` holding
  `chunks.db` (SQLite) and `vectors-<identity>.usearch`. No passage, vector
  or file row is written to `lattice.db`. Why: backups are `VACUUM INTO` of
  `lattice.db` and would carry a copy of every indexed repo; library chunk
  writes bump `vector_index_state`; and a separate store cannot leak into
  library search. Same pattern as `summaries-<identity>.usearch`. The one
  thing `lattice.db` does hold is the folders list (`explorer_folders`, see
  "Your folders" below): names and pins, not index content.
- **Indexing starts when a folder is picked.** The Explorer locks the scope
  (see the Explorer doc), so an index exists only for a folder the user chose.
  Reopening a folder resumes its index incrementally.
- **One folder open at a time.** Opening a folder closes the previous one's
  watcher, indexing task and SQLite pool. Navigating away from the Explorer
  page does not close it; "Close folder" does.
- **Refused roots.** The filesystem root, the user's home directory itself,
  and any ancestor of home (`/Users`) are never indexed (state `refused`).
  The Explorer still opens them; only the index is skipped.
- **Size cap.** If the walk finds more than 20,000 indexable files, stop
  before embedding anything (state `tooLarge`, message with the count). The
  grep tools still work. The count is recorded in the index's `meta`
  (`too_large`), so a closed folder still lists as too large; a later walk
  that fits clears it.
- **A folder's own index first; else the parent's.** A folder whose own
  index directory exists always opens it. Only a folder without one reuses
  the deepest enclosing folder's index: open that index, walk and watch only
  the sub-tree, and filter search to that path prefix. An enclosing index
  marked too large holds nothing, so it is never reused. (Before 2026-10-02
  the deepest enclosing index always won, so `~/Code/MusicVST` embedded
  itself again into `~/Code`'s index once `~/Code` had been opened.)
- **Nothing deletes an index unless the user asks.** No cap, no eviction:
  "Delete index" and "Remove…" in the folders list, and "Rebuild index" in
  the pill, are the only deletes. Each `chunks.db` records its root,
  identity, last-opened time, `too_large` and `complete` (when a full run
  last finished) in `meta`, so a closed index can be described without
  opening it.
- **Embedding model switch = rebuild.** The vectors file is keyed by the
  embedding identity. On open, if the stored identity differs, drop the old
  vectors file and re-embed (chunks and hashes stay; only vectors are redone).
  No active embedding model: state `unavailable`, Explorer works without.

## Storage (`chunks.db`)

```sql
CREATE TABLE meta  (key TEXT PRIMARY KEY, value TEXT NOT NULL);   -- root, identity, last_opened, version,
                                                                   -- too_large ("<n>" or "<n>+"), complete (ms)
CREATE TABLE files (path TEXT PRIMARY KEY, size INTEGER NOT NULL, mtime INTEGER NOT NULL,
                    content_hash TEXT NOT NULL, embedded INTEGER NOT NULL DEFAULT 0);
CREATE TABLE chunks(id INTEGER PRIMARY KEY, path TEXT NOT NULL, start_line INTEGER NOT NULL,
                    end_line INTEGER NOT NULL, text TEXT NOT NULL);
CREATE INDEX chunks_path ON chunks(path);
CREATE VIRTUAL TABLE passages_fts USING fts5(text, path UNINDEXED, content='chunks',
                    content_rowid='id', tokenize="unicode61 tokenchars '_'");
```

Its own `SqlitePool` (WAL, max 2 connections), schema created on open.
Vector ids are the chunk `id` as a string. `embedded = 1` once all of a file's
chunks are in the vectors file, so an interrupted run resumes per file.

## Chunking

No new parser dependency. Line-based, structure-aware:

- Walk with the `ignore` crate exactly as `fs.rs` search does (honour
  `.gitignore`, skip `.git`, ignored entries, binaries, files over 1 MiB).
  Also skip lockfiles and minified files (`*.lock`, `package-lock.json`,
  `*.min.*`, average line length over 300).
- Split a file into chunks of about 40 to 80 lines and at most ~1,500 chars,
  preferring to break before a line at indentation 0 that opens a top-level
  item (fn, pub, impl, struct, enum, trait, class, def, function, export,
  const, interface, type, a Markdown heading) or after a blank line. Hard cap
  120 lines; a single overlong line is clipped.
- Line numbers are 1-based and inclusive, matching the line-reference grammar.
- The embedded text is `"{path}:{start}-{end}\n{text}"`; the path is a strong
  signal for code search. FTS gets the same text.

## Indexing run

- Background tokio task per open folder, with a cancellation token.
- Walk, compare each file's `(size, mtime)` to `files`; on a mismatch hash
  the content (sha256) and re-chunk only if the hash changed. Delete chunks,
  FTS rows and vectors for files that are gone.
- Embed with `EmbeddingPort::embed_batch` in batches of 16, checking for
  cancellation between batches. Small batches keep a chat turn's query
  embedding from waiting behind a long run.
- Save the usearch file every ~500 chunks and at the end.
- Watcher: `notify-debouncer-full` (pattern: `features/vault/watcher.rs`),
  recursive on the walked root, ~1.5 s debounce; changed or removed paths go
  through the same per-file update. Ignored paths are dropped.
- Status events throttled to at most ~4 per second (a change of state always
  goes out at once).

### Progress (as built)

- **Passages, not files.** Files only move when a save marks them (every
  ~500 passages), so counting files ran ahead of the work and then sat still.
  The status carries `passagesTotal` / `passagesEmbedded` (chunks of files
  marked embedded, plus every batch since); passages move after every
  batch of 16, files at saves as before. Percent is
  `floor(embedded / total × 100)`, so it never says 100 early.
- **Time left.** `run::Pace` folds each batch's rate into an average
  weighted by how long the batch took (weight `1 − e^(−dt/30 s)`), so a
  pause (a chat turn's query embedding, a save) moves the estimate without
  throwing it. Nothing is said for the first 10 s of a run (the first
  batches include loading the model): `passagesPerSecond` and `etaSeconds`
  are `null` until then, and again once the run settles.
- **Scanning counts up.** The walk reports the files found every 200, as
  `filesTotal`, but only once it passes what the status already says, so a
  rescan does not count down from the last run's number to zero and back.
- **Logs** (INFO, one line each, full runs only):
  `Folder index scan finished` (files, new_or_changed, removed,
  passages_to_embed, elapsed_ms); `Folder index progress` at each tenth
  (percent, embedded, total, per_second, eta_seconds); `Folder index ready`
  (files, passages, embedded this run, elapsed_ms, average per_second); and
  `Folder index paused` when a run is cancelled (embedded, total).
- **Closing** waits for the batch in flight and a save, which can take a
  moment on a slow model; "Close folder" says "Closing…" and is disabled
  until it returns.

## Search

`FolderSearch::search(query, path_prefix: Option<&str>, top_k)`:
dense (`embed_query`, top 40) and FTS5 `bm25` (top 40), fused with reciprocal
rank fusion (k = 60), filtered to `path_prefix` (the sub-folder case and the
tool's `path` argument), top `top_k` returned as
`{ path, start_line, end_line, text, score }`. While a run is still going it
searches what is indexed and the caller says how far along it is.

## Model integration

- **Tool `search_folder { query, path? }`.** "Find code or text in the folder
  by meaning. Use it for where/how questions when you don't know the exact
  words; use search_files for exact text." Offered with the other explorer
  tools when the folder's index has at least one chunk. Result: up to 8 hits,
  each headed `` `path:start-end` `` then numbered lines (reuse
  `number_line`), capped by the tool output budget. Activity label:
  `Searched the folder for "…"`.
- **First-step retrieval.** On every explorer turn whose index has chunks,
  run one search with the user's message before generation and put the top 5
  passages in the explorer block, headed as "Passages from the folder", each
  labelled with its line reference so the answer cites them the same way.
  Budget: passages get up to half of the explorer block's existing share; the
  open file and selection keep the rest. This is what makes the index useful
  to models without tool calling.
- Passages from the folder are not library sources: they do not become `[n]`
  citations and are not judged by grounding verification (sentences with line
  references are already skipped).

## Contracts

```rust
// features/explorer/index/dto.rs (specta, camelCase)
enum FolderIndexState { Scanning, Indexing, Ready, TooLarge, Refused, Unavailable, Error }
FolderIndexStatusDto { root: String, indexRoot: String, state: FolderIndexState,
                       filesTotal: u32, filesIndexed: u32,       // files move at saves
                       passagesTotal: u32, passagesEmbedded: u32, // passages after every batch
                       passagesPerSecond: Option<f32>,            // None for the first ~10 s
                       etaSeconds: Option<u32>,
                       message: Option<String> }
// indexRoot != root when a parent folder's index is reused.

enum FolderIndexSummaryState { Indexed, Partial, Indexing, NotIndexed, TooLarge, Refused, Error }
FolderIndexSummaryDto { state, filesTotal, filesIndexed, passagesTotal, passagesEmbedded,
                        bytes: u64,                  // its own index directory; 0 when none
                        indexRoot: Option<String>,   // the enclosing folder it reuses
                        etaSeconds: Option<u32>, message: Option<String> }

// features/explorer/dto.rs
ExplorerFolderDto     { root, name, pinned, addedAt, lastOpenedAt,   // RFC 3339
                        exists: bool, threadCount: u32, index: FolderIndexSummaryDto }
ExplorerFolderListDto { home: Option<String>, folders: Vec<ExplorerFolderDto> }
```

| command | args | returns |
|---|---|---|
| `explorer_index_open` | `root` | `FolderIndexStatusDto` (starts or resumes; closes any other open folder; adds or touches the folder's row) |
| `explorer_index_close` | | `()` (waits for the batch in flight and a save) |
| `explorer_index_status` | `root` | `FolderIndexStatusDto` |
| `explorer_index_rebuild` | `root` | `FolderIndexStatusDto` (wipes this folder's index, starts over) |
| `explorer_folders_list` | | `ExplorerFolderListDto` (pinned first, then last opened; adopts index directories with no row) |
| `explorer_folder_rename` | `root, name` | `()` (an empty name goes back to the folder's own) |
| `explorer_folder_set_pinned` | `root, pinned` | `()` |
| `explorer_folder_delete_index` | `root` | `()` (its own index only, closed first; the row stays, so the next open rebuilds) |
| `explorer_folder_remove` | `root, deleteThreads` | `u32` threads deleted (its own index and the row; threads only when asked) |

`explorer_index_forget` is gone; `explorer_folder_delete_index` replaces it.
Event `explorer-index://status`, payload `FolderIndexStatusDto`.

## Your folders (as built, 2026-10-02)

The start screen's Recent list (localStorage, capped at 8) and the
manager's eviction past 8 indexes were replaced by one managed list.

- **Table.** `src-tauri/migrations/20261002110000_explorer_folders.sql`:
  `explorer_folders(root TEXT PK, name, pinned 0/1, added_at, last_opened_at)`,
  times RFC 3339 UTC to the millisecond so they sort as text.
  `20261003100000_explorer_folder_settings.sql` adds `instructions` (the
  folder's system prompt) and `space_id` (NULL is General; a deleted space
  sets it back to NULL). `explorer_folder_set_settings` writes both and moves
  the folder's threads into the space in the same transaction, and binding a
  thread to a folder (`set_conversation_explorer_root`) files it in the
  folder's space, so a thread never takes the Chat sidebar's space. The
  chat's prompt precedence is the conversation's own prompt, then the
  folder's instructions, then the space's prompt, then the global one
  (`infrastructure/conversation_context.rs`). All SQL is in
  `features/explorer/repository.rs`; `features/explorer/folders.rs` composes
  it with the manager and the conversation delete.
- **Rows.** `explorer_index_open` upserts the row and bumps
  `last_opened_at` (the persisted root reopened on launch counts). A list
  read adopts any index directory whose `meta.root` has no row (indexes made
  before the table), with `last_opened` from its meta.
- **Index summary.** For the open folder it is the live status; for the
  rest it is read through a short-lived connection to the index's
  `chunks.db` (the same as `list_indexes`), without opening it for writing:
  `partial` when files or passages are left to embed, `notIndexed` when
  there is nothing and no full run ever finished (`meta.complete`),
  `indexed` otherwise; `tooLarge` from `meta.too_large`; `refused` from the
  same rules as opening. `bytes` is the size of the folder's own index
  directory; a reused sub-folder reports the enclosing root instead.
- **Delete index** removes only the folder's own directory (closing it
  first when open). A row that reuses an enclosing index has none, and the
  enclosing index is left alone.
- **Remove** deletes the folder's own index and its row. With
  `deleteThreads`, each of its conversations goes through the app's
  conversation delete (`conversation::commands::delete_conversation_unmetered`:
  cancels a running turn, deletes the conversation's attachments, deletes
  the row so messages cascade, audits), after one rate-limit check for the
  whole removal. Without it, the threads keep `explorer_root` and come back
  when the folder is added again.

## Frontend

- `ExplorerPage` calls `explorerIndexOpen(root)` whenever a root is set
  (including the persisted one on launch); "Close folder" calls
  `explorerIndexClose()` and then clears the root, showing "Closing…" and
  disabled while it waits.
- `explorerStore` holds `indexStatus` for the open root, fed by the command
  results and the event. It keeps the open root and the thread per folder;
  the folders list is the backend's, not localStorage.
- Scope bar pill (`IndexStatusPill`): `Scanning · 1,379 files` (counting up);
  `Indexing 28% · ~14 min left` with a thin bar driven by passages;
  `Indexed · 1,379 files`; `Too large to index · search uses text matching`;
  `Not indexed (home folder)`; `No embedding model`; `Index failed` with
  Retry. Its panel adds `3,067 of 10,958 passages · 489 of 1,379 files`, the
  rate (`12 passages/s`), the message, and "Rebuild index". Time left reads
  `<1 min`, `~3 min`, `~1 h 10 min` (`indexProgress.ts`).
- Chat column (`ExplorerIndexNotice`, above the composer beside the model
  notice, only on the open folder's own thread): `Indexing this folder · 28% —
  search covers what's indexed so far` (or `Scanning this folder · …`). It
  fades in after ~0.8 s so a quick rescan does not flash it, and goes when
  the index is ready.
- Start screen (`ExplorerStart`, `ExplorerFolders`, `useExplorerFolders`):
  with no folders, the large "Choose a folder…" choice and the explainer;
  with folders, the choice steps down to a slimmer bar and "Your folders"
  follows: a header with totals (`4 folders · 182 MB of indexes`), a filter
  from 7 folders, and one row per folder (name, path with home as `~`, a
  status chip — `Indexed · 1,379 files`, `Paused at 42%`,
  `Indexing 28% · ~14 min`, `Not indexed`, `Too large`, `Folder missing`,
  `Index error` — then threads, index size or `in ~/Code’s index`, and when it
  was opened). Click opens; a pin toggle; an overflow menu with Rename
  (inline), Delete index and Remove… (a dialog: `Remove MusicVST from
  Lattice?`, what is deleted and that the folder on disk is not touched,
  and an unchecked "Also delete its N chat threads" when N > 0). Missing
  folders are muted and cannot be opened. Arrow keys move between rows. The
  list follows `explorer-index://status`, so an indexing row moves live.
