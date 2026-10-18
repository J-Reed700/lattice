-- Logical program-scoped sources own immutable, addressable captured versions.
CREATE TABLE learning_source_library (
    id TEXT NOT NULL,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('web','document','pasted')),
    origin TEXT NOT NULL,
    requested_url TEXT,
    freshness_policy TEXT NOT NULL CHECK(freshness_policy IN ('fixed','manual','before_use')),
    active_version_id TEXT,
    pending_version_id TEXT,
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(program_id,id)
);

CREATE TABLE learning_source_versions (
    id TEXT NOT NULL,
    program_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    version_number INTEGER NOT NULL CHECK(version_number >= 1),
    title TEXT NOT NULL,
    publisher TEXT,
    requested_url TEXT,
    resolved_url TEXT,
    full_text TEXT NOT NULL,
    excerpt TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    word_count INTEGER NOT NULL CHECK(word_count >= 0),
    truncated INTEGER NOT NULL CHECK(truncated IN (0,1)),
    extraction_version TEXT NOT NULL,
    acquired_at INTEGER NOT NULL,
    PRIMARY KEY(program_id,id),
    UNIQUE(program_id,source_id,version_number),
    UNIQUE(program_id,source_id,content_sha256),
    FOREIGN KEY(program_id,source_id)
        REFERENCES learning_source_library(program_id,id) ON DELETE CASCADE
);

CREATE TABLE learning_source_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('add_web','add_document','add_text','refresh','adopt','policy')),
    payload_hash TEXT NOT NULL,
    result_version_id TEXT,
    result_revision INTEGER NOT NULL CHECK(result_revision >= 0),
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id,source_id)
        REFERENCES learning_source_library(program_id,id) ON DELETE CASCADE
);

CREATE TABLE learning_source_refresh_checks (
    operation_id TEXT PRIMARY KEY REFERENCES learning_source_operations(operation_id) ON DELETE CASCADE,
    program_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('unchanged','update_available','failed')),
    checked_at INTEGER NOT NULL,
    active_digest TEXT,
    pending_version_id TEXT,
    message TEXT,
    FOREIGN KEY(program_id,source_id)
        REFERENCES learning_source_library(program_id,id) ON DELETE CASCADE
);
CREATE INDEX learning_source_checks_history
    ON learning_source_refresh_checks(program_id,source_id,checked_at DESC);

-- Preserve all source IDs already embedded in prepared lessons/questions. Legacy
-- excerpts were bounded at acquisition, so mark their extraction accordingly.
INSERT INTO learning_source_library(
    id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,
    pending_version_id,revision,created_at,updated_at
)
SELECT id,program_id,
       CASE WHEN url IS NOT NULL THEN 'web' ELSE 'document' END,
       CASE WHEN url IS NOT NULL THEN url
            WHEN instr(id, ':') > 0 THEN substr(id, 1, instr(id, ':') - 1)
            ELSE id END,
       url,
       CASE WHEN url IS NOT NULL THEN 'manual' ELSE 'fixed' END,
       id,NULL,0,acquired_at,acquired_at
FROM learning_sources;

INSERT INTO learning_source_versions(
    id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,
    full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at
)
SELECT id,program_id,id,1,title,NULL,url,url,excerpt,excerpt,'',0,1,
       'legacy_bounded_extraction_v1',acquired_at
FROM learning_sources;
