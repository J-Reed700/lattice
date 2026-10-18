-- Calibration learns from evidence the user attaches: an attach that
-- verifies a claim the gate had weighed is an outcome labelled
-- 'evidence_added' (lattice-core `LabelSource::EvidenceAdded`), so the
-- label vocabulary of source_trust_outcomes gains it. SQLite cannot alter a
-- CHECK, so the table is rebuilt with its rows; no table references it, so
-- the copy, drop and rename lose nothing.
-- Design: docs/design/2026-10-18-source-provenance-and-trust.md §13 and its
-- P5 as-built notes.
CREATE TABLE source_trust_outcomes_next (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    claim_id TEXT NOT NULL,
    source_version_id TEXT NOT NULL,
    origin_key TEXT NOT NULL,
    level TEXT NOT NULL CHECK(level IN (
        'primary','authoritative','established','community','unknown','distrusted'
    )),
    claim_kind TEXT NOT NULL CHECK(claim_kind IN (
        'specification','measurement','version_bound','definition','practice','opinion',
        'historical','general'
    )),
    decision_sufficient INTEGER NOT NULL CHECK(decision_sufficient IN (0,1)),
    decision_right INTEGER NOT NULL CHECK(decision_right IN (0,1)),
    label_source TEXT NOT NULL CHECK(label_source IN (
        'user_accept_source','user_confirm','user_remove','user_edit','verifier_agree',
        'verifier_disagree','contrary_evidence','evidence_added'
    )),
    policy TEXT NOT NULL,
    at INTEGER NOT NULL
);
INSERT INTO source_trust_outcomes_next(
    id, program_id, lesson_id, claim_id, source_version_id, origin_key, level, claim_kind,
    decision_sufficient, decision_right, label_source, policy, at
)
SELECT
    id, program_id, lesson_id, claim_id, source_version_id, origin_key, level, claim_kind,
    decision_sufficient, decision_right, label_source, policy, at
FROM source_trust_outcomes
ORDER BY rowid;
DROP TABLE source_trust_outcomes;
ALTER TABLE source_trust_outcomes_next RENAME TO source_trust_outcomes;
CREATE INDEX source_trust_outcomes_cell ON source_trust_outcomes(level, claim_kind, at);
CREATE INDEX source_trust_outcomes_origin ON source_trust_outcomes(origin_key, at);
CREATE INDEX source_trust_outcomes_claim ON source_trust_outcomes(claim_id, label_source);
