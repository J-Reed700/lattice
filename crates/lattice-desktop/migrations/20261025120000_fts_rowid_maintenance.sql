-- FTS5 does not index UNINDEXED columns. Keep durable, indexed identities in
-- ordinary tables and address FTS entries by their INTEGER rowid.

CREATE TABLE chunk_fts_rowids (
    fts_rowid INTEGER PRIMARY KEY,
    chunk_id TEXT NOT NULL UNIQUE
);

INSERT INTO chunk_fts_rowids(chunk_id) SELECT id FROM text_chunks;

DROP TRIGGER IF EXISTS chunks_fts_insert;
DROP TRIGGER IF EXISTS chunks_fts_update;
DROP TRIGGER IF EXISTS chunks_fts_delete;
DELETE FROM chunks_fts;
DELETE FROM chunks_trigram;
INSERT INTO chunks_fts(rowid, chunk_id, content, header)
SELECT m.fts_rowid, c.id, c.content, c.context_prefix
FROM text_chunks c JOIN chunk_fts_rowids m ON m.chunk_id=c.id;
INSERT INTO chunks_trigram(rowid, chunk_id, content, header)
SELECT m.fts_rowid, c.id, c.content, c.context_prefix
FROM text_chunks c JOIN chunk_fts_rowids m ON m.chunk_id=c.id;

CREATE TRIGGER chunks_fts_insert AFTER INSERT ON text_chunks BEGIN
  INSERT INTO chunk_fts_rowids(chunk_id) VALUES(new.id);
  INSERT INTO chunks_fts(rowid,chunk_id,content,header)
    SELECT fts_rowid,new.id,new.content,new.context_prefix FROM chunk_fts_rowids WHERE chunk_id=new.id;
  INSERT INTO chunks_trigram(rowid,chunk_id,content,header)
    SELECT fts_rowid,new.id,new.content,new.context_prefix FROM chunk_fts_rowids WHERE chunk_id=new.id;
END;

CREATE TRIGGER chunks_fts_update AFTER UPDATE OF id,content,context_prefix ON text_chunks BEGIN
  DELETE FROM chunks_fts WHERE rowid=(SELECT fts_rowid FROM chunk_fts_rowids WHERE chunk_id=old.id);
  DELETE FROM chunks_trigram WHERE rowid=(SELECT fts_rowid FROM chunk_fts_rowids WHERE chunk_id=old.id);
  UPDATE chunk_fts_rowids SET chunk_id=new.id WHERE chunk_id=old.id;
  INSERT INTO chunks_fts(rowid,chunk_id,content,header)
    SELECT fts_rowid,new.id,new.content,new.context_prefix FROM chunk_fts_rowids WHERE chunk_id=new.id;
  INSERT INTO chunks_trigram(rowid,chunk_id,content,header)
    SELECT fts_rowid,new.id,new.content,new.context_prefix FROM chunk_fts_rowids WHERE chunk_id=new.id;
END;

CREATE TRIGGER chunks_fts_delete AFTER DELETE ON text_chunks BEGIN
  DELETE FROM chunks_fts WHERE rowid=(SELECT fts_rowid FROM chunk_fts_rowids WHERE chunk_id=old.id);
  DELETE FROM chunks_trigram WHERE rowid=(SELECT fts_rowid FROM chunk_fts_rowids WHERE chunk_id=old.id);
  DELETE FROM chunk_fts_rowids WHERE chunk_id=old.id;
END;

-- Source primary keys are unique within each source kind. Keep the identity
-- normalized so trigger lookups use a compact indexed pair. This intentionally
-- has no FK: AFTER DELETE triggers must still find the rowid after cascades.
CREATE TABLE conversation_search_fts_rowids (
    fts_rowid INTEGER PRIMARY KEY,
    source TEXT NOT NULL CHECK(source IN ('title','message','bookmark')),
    source_id TEXT NOT NULL,
    UNIQUE(source, source_id)
);

DROP TRIGGER IF EXISTS trg_conversation_search_fts_conversation_ai;
DROP TRIGGER IF EXISTS trg_conversation_search_fts_conversation_au;
DROP TRIGGER IF EXISTS trg_conversation_search_fts_conversation_ad;
DROP TRIGGER IF EXISTS trg_conversation_search_fts_message_ai;
DROP TRIGGER IF EXISTS trg_conversation_search_fts_message_au;
DROP TRIGGER IF EXISTS trg_conversation_search_fts_message_ad;
DROP TRIGGER IF EXISTS trg_conversation_search_fts_bookmark_ai;
DROP TRIGGER IF EXISTS trg_conversation_search_fts_bookmark_au;
DROP TRIGGER IF EXISTS trg_conversation_search_fts_bookmark_ad;

DELETE FROM conversation_search_fts;
INSERT INTO conversation_search_fts_rowids(source,source_id)
 SELECT 'title',id FROM conversations;
INSERT INTO conversation_search_fts_rowids(source,source_id)
 SELECT 'message',id FROM conversation_messages;
INSERT INTO conversation_search_fts_rowids(source,source_id)
 SELECT 'bookmark',id FROM conversation_message_bookmarks;
INSERT INTO conversation_search_fts(rowid,conversation_id,message_id,source,content)
 SELECT r.fts_rowid,c.id,NULL,'title',COALESCE(c.title,'') FROM conversations c
 JOIN conversation_search_fts_rowids r ON r.source='title' AND r.source_id=c.id;
INSERT INTO conversation_search_fts(rowid,conversation_id,message_id,source,content)
 SELECT r.fts_rowid,m.conversation_id,m.id,'message',COALESCE(m.content,'') FROM conversation_messages m
 JOIN conversation_search_fts_rowids r ON r.source='message' AND r.source_id=m.id;
INSERT INTO conversation_search_fts(rowid,conversation_id,message_id,source,content)
 SELECT r.fts_rowid,b.conversation_id,b.message_id,'bookmark',TRIM(COALESCE(b.title,'')||' '||COALESCE(b.note,'')) FROM conversation_message_bookmarks b
 JOIN conversation_search_fts_rowids r ON r.source='bookmark' AND r.source_id=b.id;

CREATE TRIGGER trg_conversation_search_fts_conversation_ai AFTER INSERT ON conversations BEGIN
 INSERT INTO conversation_search_fts_rowids(source,source_id) VALUES('title',new.id);
 INSERT INTO conversation_search_fts(rowid,conversation_id,message_id,source,content)
 SELECT fts_rowid,new.id,NULL,'title',COALESCE(new.title,'') FROM conversation_search_fts_rowids WHERE source='title' AND source_id=new.id;
END;
CREATE TRIGGER trg_conversation_search_fts_conversation_au AFTER UPDATE OF title,id ON conversations BEGIN
 DELETE FROM conversation_search_fts WHERE rowid=(SELECT fts_rowid FROM conversation_search_fts_rowids WHERE source='title' AND source_id=old.id);
 UPDATE conversation_search_fts_rowids SET source_id=new.id WHERE source='title' AND source_id=old.id;
 INSERT INTO conversation_search_fts(rowid,conversation_id,message_id,source,content)
 SELECT fts_rowid,new.id,NULL,'title',COALESCE(new.title,'') FROM conversation_search_fts_rowids WHERE source='title' AND source_id=new.id;
END;
CREATE TRIGGER trg_conversation_search_fts_conversation_ad AFTER DELETE ON conversations BEGIN
 DELETE FROM conversation_search_fts WHERE rowid=(SELECT fts_rowid FROM conversation_search_fts_rowids WHERE source='title' AND source_id=old.id);
 DELETE FROM conversation_search_fts_rowids WHERE source='title' AND source_id=old.id;
END;
CREATE TRIGGER trg_conversation_search_fts_message_ai AFTER INSERT ON conversation_messages BEGIN
 INSERT INTO conversation_search_fts_rowids(source,source_id) VALUES('message',new.id);
 INSERT INTO conversation_search_fts(rowid,conversation_id,message_id,source,content)
 SELECT fts_rowid,new.conversation_id,new.id,'message',COALESCE(new.content,'') FROM conversation_search_fts_rowids WHERE source='message' AND source_id=new.id;
END;
CREATE TRIGGER trg_conversation_search_fts_message_au AFTER UPDATE OF content,id,conversation_id ON conversation_messages BEGIN
 DELETE FROM conversation_search_fts WHERE rowid=(SELECT fts_rowid FROM conversation_search_fts_rowids WHERE source='message' AND source_id=old.id);
 UPDATE conversation_search_fts_rowids SET source_id=new.id WHERE source='message' AND source_id=old.id;
 INSERT INTO conversation_search_fts(rowid,conversation_id,message_id,source,content)
 SELECT fts_rowid,new.conversation_id,new.id,'message',COALESCE(new.content,'') FROM conversation_search_fts_rowids WHERE source='message' AND source_id=new.id;
END;
CREATE TRIGGER trg_conversation_search_fts_message_ad AFTER DELETE ON conversation_messages BEGIN
 DELETE FROM conversation_search_fts WHERE rowid=(SELECT fts_rowid FROM conversation_search_fts_rowids WHERE source='message' AND source_id=old.id);
 DELETE FROM conversation_search_fts_rowids WHERE source='message' AND source_id=old.id;
END;
CREATE TRIGGER trg_conversation_search_fts_bookmark_ai AFTER INSERT ON conversation_message_bookmarks BEGIN
 INSERT INTO conversation_search_fts_rowids(source,source_id) VALUES('bookmark',new.id);
 INSERT INTO conversation_search_fts(rowid,conversation_id,message_id,source,content)
 SELECT fts_rowid,new.conversation_id,new.message_id,'bookmark',TRIM(COALESCE(new.title,'')||' '||COALESCE(new.note,'')) FROM conversation_search_fts_rowids WHERE source='bookmark' AND source_id=new.id;
END;
CREATE TRIGGER trg_conversation_search_fts_bookmark_au AFTER UPDATE OF id,title,note,conversation_id,message_id ON conversation_message_bookmarks BEGIN
 DELETE FROM conversation_search_fts WHERE rowid=(SELECT fts_rowid FROM conversation_search_fts_rowids WHERE source='bookmark' AND source_id=old.id);
 UPDATE conversation_search_fts_rowids SET source_id=new.id WHERE source='bookmark' AND source_id=old.id;
 INSERT INTO conversation_search_fts(rowid,conversation_id,message_id,source,content)
 SELECT fts_rowid,new.conversation_id,new.message_id,'bookmark',TRIM(COALESCE(new.title,'')||' '||COALESCE(new.note,'')) FROM conversation_search_fts_rowids WHERE source='bookmark' AND source_id=new.id;
END;
CREATE TRIGGER trg_conversation_search_fts_bookmark_ad AFTER DELETE ON conversation_message_bookmarks BEGIN
 DELETE FROM conversation_search_fts WHERE rowid=(SELECT fts_rowid FROM conversation_search_fts_rowids WHERE source='bookmark' AND source_id=old.id);
 DELETE FROM conversation_search_fts_rowids WHERE source='bookmark' AND source_id=old.id;
END;
