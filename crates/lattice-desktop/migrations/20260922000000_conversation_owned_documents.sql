-- Files attached to a chat, rather than filed in the library.
--
-- Attaching a file used to be the same operation as adding one to the library:
-- a documents row, chunks, embeddings, FTS mirrors, a blob and a space
-- membership, permanent and vault-wide. A transcript dropped into the composer
-- to ask one question about it was indistinguishable, forever after, from a
-- document the user had deliberately filed.
--
-- An attachment is still extracted, chunked and embedded -- that is how the
-- turn reads and cites it -- but it now belongs to the conversation it arrived
-- in:
--
--   * hidden from the library listing and from the document count beside it
--   * excluded from vault-wide search and from every other chat's retrieval,
--     while its own chat can still search it on later turns
--   * deleted with that conversation
--
-- "Add to library" sets this column back to NULL and the row becomes an
-- ordinary document.
--
-- Deliberately no foreign key. Conversation-owned rows are removed through
-- DeleteDocumentUseCase, which also drops their vectors and releases their
-- library blob; a cascade would delete the row and orphan both. A dangling id
-- is collected at startup by that same use case.
ALTER TABLE documents ADD COLUMN owner_conversation_id TEXT;

CREATE INDEX IF NOT EXISTS idx_documents_owner_conversation
    ON documents(owner_conversation_id) WHERE owner_conversation_id IS NOT NULL;
