-- Pictures the reader attaches to a message, beside the ones an answer shows.
-- An attached picture has no web source, so source_url and image_url become
-- nullable; origin gains 'attachment'; original_name keeps the file name the
-- reader attached (file names on disk are always <id>.<ext>).
-- A draft row (request_id 'draft:<ulid>') is leased to the composer that
-- staged it until a user message claims it.
-- Design: docs/design/2026-10-17-chat-vision.md §10.4.
--
-- No table references chat_images, so the copy/drop/rename rebuild cannot
-- cascade into child rows, even with foreign keys enforced inside the
-- migration's transaction.

CREATE TABLE chat_images_new (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    request_id TEXT NOT NULL,
    message_id TEXT REFERENCES conversation_messages(id) ON DELETE CASCADE,
    ref TEXT NOT NULL,
    ext TEXT NOT NULL CHECK (ext IN ('jpg','png','gif','webp')),
    mime TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    caption TEXT NOT NULL,
    source_url TEXT,
    source_title TEXT,
    image_url TEXT,
    origin TEXT NOT NULL CHECK (origin IN ('wikimedia_commons','page','attachment')),
    license TEXT,
    attribution TEXT,
    original_name TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (request_id, ref)
);

INSERT INTO chat_images_new (id, conversation_id, request_id, message_id, ref, ext, mime,
    sha256, byte_size, width, height, caption, source_url, source_title, image_url, origin,
    license, attribution, original_name, created_at)
SELECT id, conversation_id, request_id, message_id, ref, ext, mime, sha256, byte_size, width,
    height, caption, source_url, source_title, image_url, origin, license, attribution, NULL,
    created_at
FROM chat_images;

DROP TABLE chat_images;
ALTER TABLE chat_images_new RENAME TO chat_images;

CREATE INDEX idx_chat_images_message ON chat_images(message_id);
CREATE INDEX idx_chat_images_conversation ON chat_images(conversation_id);
CREATE INDEX idx_chat_images_request ON chat_images(request_id);
