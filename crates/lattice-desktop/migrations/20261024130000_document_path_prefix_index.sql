-- The UNIQUE file_path constraint already supplies the binary equality index.
-- Use the redundant named index for ASCII case-insensitive directory ranges,
-- matching SQLite LIKE's existing path semantics without a scan per folder.
DROP INDEX IF EXISTS idx_documents_file_path;
CREATE INDEX idx_documents_file_path ON documents(file_path COLLATE NOCASE);
