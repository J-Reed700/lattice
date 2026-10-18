-- Sourcing owner cache: subject owner origins resolved via Wikidata,
-- registries or code forge (GitHub). Design 2026-10-18 §3.7.
CREATE TABLE IF NOT EXISTS sourcing_owner_cache (
    cache_key TEXT PRIMARY KEY NOT NULL,
    subject_name TEXT NOT NULL,
    subject_kind TEXT NOT NULL,
    origins_json TEXT NOT NULL,
    resolved_by TEXT,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sourcing_owner_cache_updated_at
    ON sourcing_owner_cache(updated_at);
