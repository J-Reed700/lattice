# FTS5 Chunk-Level Trigger Edge Cases

Lexical search indexes **chunks**, not documents. The schema lives in
`src-tauri/migrations/20260916000000_init_schema.sql`:

- `chunks_fts` (`porter unicode61 remove_diacritics 2`): the index every BM25
  query reads.
- `chunks_trigram` (`trigram`): the same text as 3-character windows, consulted
  only for queries containing CJK (`features/search/engine/fts_query.rs`).
- Triggers on `text_chunks` keep both in step, indexing
  `COALESCE(contextualized_content, content)`:
  `chunks_fts_insert`, `chunks_fts_update` (delete + re-insert the one row),
  `chunks_fts_delete`.
- `documents_fts` is a legacy document-level table with **no triggers**. No
  search path reads it; it stays empty in normal operation.

An earlier design kept one `documents_fts` row per document and rebuilt it with
`GROUP_CONCAT` on every chunk insert. Re-tokenizing a growing document after each
chunk held SQLite's single writer for minutes, so it was dropped. The old
`fts5_migration.rs` (`migrate_to_document_level_fts`) sits in this directory but
is not declared in `mod.rs` and is never compiled.

## Edge cases

| Case | Behavior |
|------|----------|
| Document with no chunks | Nothing is indexed; BM25 cannot return it. |
| Single chunk | `chunks_fts_insert` adds one row to each index. |
| Many chunks (100+) | One row per chunk, each insert O(1); no whole-document rebuild. |
| Chunk content updated | `chunks_fts_update` replaces only that chunk's rows. |
| Contextualized text added later | The update trigger re-indexes with `contextualized_content`. |
| Chunk deleted | `chunks_fts_delete` removes only that chunk's rows; siblings stay searchable. |
| Document deleted | `text_chunks.document_id` is `ON DELETE CASCADE`; each cascaded chunk delete fires `chunks_fts_delete`. |
| CJK query | Routed to `chunks_trigram`; a one- or two-character CJK query skips the lexical branch. |

BM25 rows are per chunk (`features/search/engine/bm25.rs` joins `chunks_fts` to
`text_chunks`) and carry the chunk's `document_id` for callers that need one hit
per document.

## Tests

```bash
cd src-tauri
cargo test --lib infrastructure::persistence::database::schema_validation
cargo test --lib features::search::engine::fts_query
```

`schema_validation::chunk_search_stays_current_without_legacy_document_rewrites`
covers insert, update and delete against `chunks_fts` and asserts
`documents_fts` stays empty; `test_triggers_created` checks the three triggers
exist.
