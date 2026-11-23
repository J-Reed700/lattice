-- The digest of the note's title and text as of its last sync with its file,
-- whichever side made it.
ALTER TABLE vault_mirror_state ADD COLUMN mirrored_text_sha256 TEXT;
