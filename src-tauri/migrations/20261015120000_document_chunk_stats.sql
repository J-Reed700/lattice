-- How many indexed passages each document has and how many characters they
-- hold, kept by triggers as chunks come and go. A chat turn reads the mean
-- passage length of its scope to size how deep retrieval reads; averaging
-- LENGTH(content) over every chunk in scope on every turn grew with the
-- library and bound one variable per document.
CREATE TABLE IF NOT EXISTS document_chunk_stats (
    document_id TEXT PRIMARY KEY NOT NULL,
    chunks INTEGER NOT NULL DEFAULT 0,
    content_chars INTEGER NOT NULL DEFAULT 0
);

INSERT INTO document_chunk_stats (document_id, chunks, content_chars)
SELECT document_id, COUNT(*), SUM(LENGTH(content))
FROM text_chunks
GROUP BY document_id;

CREATE TRIGGER IF NOT EXISTS text_chunks_stats_insert
AFTER INSERT ON text_chunks
BEGIN
    INSERT INTO document_chunk_stats (document_id, chunks, content_chars)
    VALUES (NEW.document_id, 1, LENGTH(NEW.content))
    ON CONFLICT(document_id) DO UPDATE SET
        chunks = chunks + 1,
        content_chars = content_chars + LENGTH(NEW.content);
END;

CREATE TRIGGER IF NOT EXISTS text_chunks_stats_delete
AFTER DELETE ON text_chunks
BEGIN
    UPDATE document_chunk_stats
    SET chunks = chunks - 1,
        content_chars = content_chars - LENGTH(OLD.content)
    WHERE document_id = OLD.document_id;
    DELETE FROM document_chunk_stats
    WHERE document_id = OLD.document_id AND chunks <= 0;
END;

CREATE TRIGGER IF NOT EXISTS text_chunks_stats_update
AFTER UPDATE OF content, document_id ON text_chunks
BEGIN
    UPDATE document_chunk_stats
    SET chunks = chunks - 1,
        content_chars = content_chars - LENGTH(OLD.content)
    WHERE document_id = OLD.document_id;
    INSERT INTO document_chunk_stats (document_id, chunks, content_chars)
    VALUES (NEW.document_id, 1, LENGTH(NEW.content))
    ON CONFLICT(document_id) DO UPDATE SET
        chunks = chunks + 1,
        content_chars = content_chars + LENGTH(NEW.content);
    DELETE FROM document_chunk_stats
    WHERE document_id = OLD.document_id AND chunks <= 0;
END;
