-- Permanent per-conversation snapshots of cited web pages.
--
-- The global page cache (web-cache/) expires after 24 hours by design, and the
-- live page can change or vanish entirely. A citation in a conversation is a
-- record of what the user was *shown*, so the text of every page a
-- conversation actually read is archived beside the citation itself: months
-- later the conversation can still quote and discuss exactly what it cited,
-- whether or not the page still exists.
--
-- First snapshot wins: later fetches of the same URL do not overwrite the
-- archived text. A page that genuinely changed is new information, and new
-- information enters as a new cited source or an established_fact correction —
-- never by silently rewriting what an earlier answer was based on.
ALTER TABLE conversation_web_sources ADD COLUMN content TEXT;
ALTER TABLE conversation_web_sources ADD COLUMN content_fetched_at TEXT;
ALTER TABLE conversation_web_sources ADD COLUMN content_truncated INTEGER NOT NULL DEFAULT 0;
