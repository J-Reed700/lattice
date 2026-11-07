-- What the app did to a file it keeps under the lattice home, kept until the
-- file is dealt with. A sweep deletes a file only when a row here names it and
-- nothing else in this database claims it; a file no row names is kept,
-- whatever the other tables hold. 'import' is written before a library blob's
-- bytes and removed once its document commits; 'release' is written with the
-- delete that let a file go and removed once the file is gone or claimed
-- again. run_id is the process that wrote the row, so a sweep leaves an import
-- that is still running alone.
CREATE TABLE file_intents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    store TEXT NOT NULL
        CHECK (store IN ('library', 'web_snapshot', 'chat_image', 'attachment')),
    kind TEXT NOT NULL CHECK (kind IN ('import', 'release')),
    name TEXT NOT NULL,
    run_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX idx_file_intents_store_name ON file_intents (store, name);
