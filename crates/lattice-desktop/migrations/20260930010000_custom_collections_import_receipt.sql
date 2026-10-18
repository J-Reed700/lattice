CREATE TABLE IF NOT EXISTS custom_collection_import_receipts (
    payload_sha256 TEXT PRIMARY KEY NOT NULL,
    imported_at TEXT NOT NULL
);
