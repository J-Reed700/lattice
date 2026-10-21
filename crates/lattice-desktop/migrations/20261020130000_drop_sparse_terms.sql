-- Sparse retrieval is removed: no curated model has a sparse head and llama.cpp cannot run one; BM25 (FTS5) stays the lexical branch.
DROP INDEX IF EXISTS idx_chunk_sparse_terms_lookup;
DROP TABLE IF EXISTS chunk_sparse_terms;
