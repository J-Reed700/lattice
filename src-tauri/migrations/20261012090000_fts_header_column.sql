-- The chunk's context prefix (heading tail, title, file name …) gets its own
-- column in both chunk indexes instead of being concatenated into `content`.
--
-- The prefix was already indexed: the triggers wrote
-- `COALESCE(contextualized_content, content)`, which is prefix + body. As part
-- of the body it carried the body's weight and counted towards the body's
-- length, so a title word could not be weighted apart from a passage word. A
-- separate column lets `bm25()` weigh it on its own (see
-- `search::engine::fts_query::HEADER_BM25_WEIGHT`). It is declared after
-- `content` so a weight list written for this layout leaves a two-column table
-- scoring exactly as before: FTS5 ignores weights past the last column.
--
-- FTS5 cannot add a column in place, so both tables are dropped, recreated
-- and refilled from `text_chunks` here. The refill is the rebuild; nothing
-- else has to notice the change.

DROP TRIGGER IF EXISTS chunks_fts_insert;
DROP TRIGGER IF EXISTS chunks_fts_update;
DROP TRIGGER IF EXISTS chunks_fts_delete;
DROP TABLE IF EXISTS chunks_fts;
DROP TABLE IF EXISTS chunks_trigram;

CREATE VIRTUAL TABLE chunks_fts USING fts5(
    chunk_id UNINDEXED,
    content,
    header,
    tokenize='porter unicode61 remove_diacritics 2'
);

CREATE VIRTUAL TABLE chunks_trigram USING fts5(
    chunk_id UNINDEXED,
    content,
    header,
    tokenize='trigram'
);

-- Both chunk indexes are written together: a query that reaches only one of
-- them must not see a different corpus.
CREATE TRIGGER chunks_fts_insert AFTER INSERT ON text_chunks BEGIN
  INSERT INTO chunks_fts(chunk_id, content, header) VALUES(new.id, new.content, new.context_prefix);
  INSERT INTO chunks_trigram(chunk_id, content, header) VALUES(new.id, new.content, new.context_prefix);
END;

CREATE TRIGGER chunks_fts_update AFTER UPDATE ON text_chunks BEGIN
  DELETE FROM chunks_fts WHERE chunk_id = old.id;
  DELETE FROM chunks_trigram WHERE chunk_id = old.id;
  INSERT INTO chunks_fts(chunk_id, content, header) VALUES(new.id, new.content, new.context_prefix);
  INSERT INTO chunks_trigram(chunk_id, content, header) VALUES(new.id, new.content, new.context_prefix);
END;

CREATE TRIGGER chunks_fts_delete
AFTER DELETE ON text_chunks
BEGIN
    DELETE FROM chunks_fts WHERE chunk_id = old.id;
    DELETE FROM chunks_trigram WHERE chunk_id = old.id;
END;

INSERT INTO chunks_fts(chunk_id, content, header)
SELECT id, content, context_prefix FROM text_chunks;

INSERT INTO chunks_trigram(chunk_id, content, header)
SELECT id, content, context_prefix FROM text_chunks;
