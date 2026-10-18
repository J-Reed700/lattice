-- Lesson claim review: the per-lesson review row, every claim with its
-- replayed state, and the append-only audit trail of what the machine and
-- the user did about it. Also the source provenance and frozen snapshot of
-- user-supplied evidence, the per-course Verifier audit opt-in and its
-- audits, and the labelled examples that measure the local checker.
-- Plain CREATE TABLE and ADD COLUMN only: no existing table is rebuilt and
-- no existing CHECK changes ("needs your review" is a completed job plus a
-- review row). The learning_programs triggers name their columns, so the
-- added columns do not touch content_revision.
-- Design: docs/design/2026-10-16-lesson-claim-review.md §7.1, §14.7, §15.

CREATE TABLE learning_lesson_reviews (
    lesson_id TEXT PRIMARY KEY REFERENCES learning_lessons(id) ON DELETE CASCADE,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK(state IN ('verifying','needs_review','stopped','ready')),
    revision INTEGER NOT NULL DEFAULT 0,
    draft_sha256 TEXT NOT NULL,
    checker_model TEXT NOT NULL,
    stop_reason_json TEXT CHECK(stop_reason_json IS NULL OR json_valid(stop_reason_json)),
    opened_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE learning_lesson_claims (
    claim_id TEXT PRIMARY KEY,
    lesson_id TEXT NOT NULL REFERENCES learning_lesson_reviews(lesson_id) ON DELETE CASCADE,
    unit_index INTEGER NOT NULL,
    unit_kind TEXT NOT NULL CHECK(unit_kind IN ('block','question','answer_key')),
    field TEXT NOT NULL,
    quote TEXT NOT NULL,
    span_start INTEGER,
    span_end INTEGER,
    statement TEXT NOT NULL,
    -- ClaimState, replayed from the events on load; stored for listing.
    state_json TEXT NOT NULL CHECK(json_valid(state_json)),
    evidence_fingerprint TEXT,
    verdict_kind TEXT,
    reason TEXT,
    -- The passages compared (EvidenceSeen) and the texts shown.
    evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)),
    updated_at INTEGER NOT NULL
);
CREATE INDEX learning_lesson_claims_lesson ON learning_lesson_claims(lesson_id, unit_index);

CREATE TABLE learning_lesson_claim_events (
    claim_id TEXT NOT NULL REFERENCES learning_lesson_claims(claim_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    actor TEXT NOT NULL CHECK(actor IN ('machine','user')),
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
    job_id TEXT,
    at INTEGER NOT NULL,
    PRIMARY KEY(claim_id, ordinal)
);

-- Rows that predate provenance read 'research', which is wrong for sources
-- the user added before this change; pre-release data may be deleted.
ALTER TABLE learning_source_library ADD COLUMN added_by TEXT NOT NULL DEFAULT 'research'
    CHECK(added_by IN ('user','research','outline','import','verifier'));
ALTER TABLE learning_source_versions ADD COLUMN snapshot_hash TEXT;

-- Verifier audit, opt-in per course (§14.2), with the consent's provider.
ALTER TABLE learning_programs ADD COLUMN verifier_audit TEXT NOT NULL DEFAULT 'off'
    CHECK(verifier_audit IN ('off','on'));
ALTER TABLE learning_programs ADD COLUMN verifier_web_search INTEGER NOT NULL DEFAULT 0
    CHECK(verifier_web_search IN (0,1));
ALTER TABLE learning_programs ADD COLUMN verifier_consent_at INTEGER;
ALTER TABLE learning_programs ADD COLUMN verifier_consent_provider TEXT;

CREATE TABLE learning_lesson_audits (
    lesson_id TEXT NOT NULL REFERENCES learning_lesson_reviews(lesson_id) ON DELETE CASCADE,
    draft_sha256 TEXT NOT NULL,
    verifier_model TEXT NOT NULL,
    policy TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('running','completed','stopped')),
    batches_json TEXT NOT NULL CHECK(json_valid(batches_json)),
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    PRIMARY KEY(lesson_id, draft_sha256, verifier_model)
);

-- Every human verdict and audit disagreement, as a labelled example of the
-- local checker (§15). Kept when the lesson or claim goes; only the course
-- takes its examples with it.
CREATE TABLE learning_checker_examples (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    claim_id TEXT NOT NULL,
    checker_model TEXT NOT NULL,
    context_window INTEGER NOT NULL,
    policy TEXT NOT NULL,
    claim TEXT NOT NULL,
    passages_json TEXT NOT NULL CHECK(json_valid(passages_json)),
    local_verdict TEXT NOT NULL,
    label TEXT NOT NULL CHECK(label IN ('true','false','retrieval_miss')),
    label_source TEXT NOT NULL CHECK(label_source IN (
        'user_confirm','user_remove','user_edit','evidence_added','verifier_agree','verifier_disagree'
    )),
    at INTEGER NOT NULL
);
CREATE INDEX learning_checker_examples_model ON learning_checker_examples(checker_model, at);
