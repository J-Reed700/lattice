-- Pictures an assistant answer shows inline. The file lives at
-- ~/.lattice/chat-images/<id>.<ext>; sha256 is checked on every read.
-- message_id is NULL while the turn that fetched the picture is still running;
-- the row is then leased by request_id and attached when the answer persists.
-- Design: docs/design/2026-10-17-chat-images-and-diagrams.md §7.5–7.6.
CREATE TABLE chat_images (
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
    source_url TEXT NOT NULL,
    source_title TEXT,
    image_url TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('wikimedia_commons','page')),
    license TEXT,
    attribution TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (request_id, ref)
);
CREATE INDEX idx_chat_images_message ON chat_images(message_id);
CREATE INDEX idx_chat_images_conversation ON chat_images(conversation_id);
CREATE INDEX idx_chat_images_request ON chat_images(request_id);
