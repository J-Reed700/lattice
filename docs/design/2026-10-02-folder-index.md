# Folder index: search the open folder by meaning

2026-10-02. Builds on `2026-10-02-explorer.md`. The Explorer's tools find text
the model can guess (`search_files` is a grep). This adds a semantic index of
the open folder so "where do we handle retries?" finds the code even when it
never says "retry".

## Decisions

- **Its own store, outside `lattice.db`.** One directory per indexed folder:
  `<data_dir>/folder-index/<first 16 hex of sha256(canonical root)>/` holding
  `chunks.db` (SQLite) and `vectors-<identity>.usearch`. Nothing is written to
  `lattice.db`, so no migration. Why: backups are `VACUUM INTO` of
  `lattice.db` and would carry a copy of every indexed repo; library chunk
  writes bump `vector_index_state`; and a separate store cannot leak into
  library search. Same pattern as `summaries-<identity>.usearch`.
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
  grep tools still work.
- **Sub-folders reuse the parent's index.** If the picked root sits inside a
  folder that already has an index, open that index, walk and watch only the
  sub-tree, and filter search to that path prefix. A parent picked after a
  child builds its own index; eviction cleans up the rest.
- **Bounded on disk.** At most 8 folder indexes (same as the recent-folders
  list); opening a 9th deletes the least recently opened directory. Each
  `chunks.db` records its root, identity and last-opened time in a `meta`
  table, so the registry is the directories themselves.
- **Embedding model switch = rebuild.** The vectors file is keyed by the
  embedding identity. On open, if the stored identity differs, drop the old
  vectors file and re-embed (chunks and hashes stay; only vectors are redone).
  No active embedding model: state `unavailable`, Explorer works without.
- **Forgettable.** The start screen's recent rows get "Forget"; it deletes
  that folder's index directory and removes the row.

## Storage (`chunks.db`)

```sql
CREATE TABLE meta  (key TEXT PRIMARY KEY, value TEXT NOT NULL);   -- root, identity, last_opened, version
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
- Status events throttled to at most ~4 per second.

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
                       filesTotal: u32, filesIndexed: u32, chunks: u32,
                       message: Option<String> }
// indexRoot != root when a parent folder's index is reused.
```

| command | args | returns |
|---|---|---|
| `explorer_index_open` | `root` | `FolderIndexStatusDto` (starts or resumes; closes any other open folder) |
| `explorer_index_close` | | `()` |
| `explorer_index_status` | `root` | `FolderIndexStatusDto` |
| `explorer_index_rebuild` | `root` | `FolderIndexStatusDto` (wipes this folder's index, starts over) |
| `explorer_index_forget` | `root` | `()` (closes it if open, deletes its directory) |

Event `explorer-index://status`, payload `FolderIndexStatusDto`.

## Frontend

- `ExplorerPage` calls `explorerIndexOpen(root)` whenever a root is set
  (including the persisted one on launch); "Close folder" calls
  `explorerIndexClose()` and then clears the root.
- `explorerStore` holds `indexStatus` for the open root, fed by the command
  results and the event.
- Scope bar: a status pill after the path. `Indexing 1,240 / 3,100 files` with
  a thin progress bar; `Indexed · 3,100 files`; `Too large to index · search
  uses text matching`; `Not indexed (home folder)`; `No embedding model`;
  `Index failed` with Retry. The pill opens a small menu with "Rebuild index".
- Start screen: copy says Lattice indexes the folder for search and the index
  lives in Lattice's data, not in the folder; recent rows get "Forget".
