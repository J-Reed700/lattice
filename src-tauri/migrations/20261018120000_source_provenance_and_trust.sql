-- Source provenance and trust: what capture read about each source version,
-- the three cached model judgements, the provenance graph between versions,
-- the user's trust overrides, and the labelled outcomes that calibrate the
-- sufficiency bars. New tables only: no existing table is rebuilt and no
-- existing CHECK changes. A source version is keyed by (program_id, id), so
-- every reference to one is a composite foreign key.
-- Column vocabularies are the serde names of the lattice-core enums
-- (domain::provenance); a test checks each list against `ALL`.
-- Design: docs/design/2026-10-18-source-provenance-and-trust.md §16 and its
-- P3 as-built notes.

-- What capture recorded about one source version, in the version's own
-- transaction; re-extractable from the snapshot when one exists.
CREATE TABLE source_provenance (
    program_id TEXT NOT NULL,
    source_version_id TEXT NOT NULL,
    origin_key TEXT NOT NULL,
    -- Origin::site(): the registrable domain, else the host; NULL for files
    -- and pasted text. Site overrides are keyed by it.
    site TEXT,
    origin_json TEXT NOT NULL CHECK(json_valid(origin_json)),
    canonical_url TEXT,
    signals_json TEXT NOT NULL CHECK(json_valid(signals_json)),
    signals_sha256 TEXT NOT NULL,
    -- The MinHash sketch: 128 values, each a little-endian u32 (4 bytes).
    sketch BLOB NOT NULL CHECK(length(sketch) = 128 * 4),
    links_json TEXT NOT NULL CHECK(json_valid(links_json)),
    extracted_from TEXT NOT NULL
        CHECK(extracted_from IN ('fetch','browser_reader','snapshot','library','text')),
    extraction_version TEXT NOT NULL,
    extracted_at INTEGER NOT NULL,
    PRIMARY KEY(source_version_id, program_id),
    FOREIGN KEY(program_id, source_version_id)
        REFERENCES learning_source_versions(program_id, id) ON DELETE CASCADE
);
CREATE INDEX source_provenance_origin ON source_provenance(program_id, origin_key);
CREATE INDEX source_provenance_site ON source_provenance(site);
CREATE INDEX source_provenance_signals ON source_provenance(signals_sha256);

-- Judgement #1, keyed by what it judged, so identical signals anywhere share
-- it.
CREATE TABLE source_standing_assessments (
    signals_sha256 TEXT NOT NULL,
    policy TEXT NOT NULL,
    model TEXT NOT NULL,
    standing TEXT NOT NULL CHECK(standing IN ('institutional','identified','community','unknown')),
    judgement_json TEXT NOT NULL CHECK(json_valid(judgement_json)),
    input_tokens INTEGER NOT NULL CHECK(input_tokens >= 0),
    output_tokens INTEGER NOT NULL CHECK(output_tokens >= 0),
    assessed_at INTEGER NOT NULL,
    PRIMARY KEY(signals_sha256, policy, model)
);

-- Judgement #2, per course, origin and subject. `input_sha256` is the rest
-- of the judgement's cache key (what the call was shown).
CREATE TABLE source_subject_authority (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    origin_key TEXT NOT NULL,
    subject_key TEXT NOT NULL,
    policy TEXT NOT NULL,
    model TEXT NOT NULL,
    input_sha256 TEXT NOT NULL,
    relation TEXT NOT NULL CHECK(relation IN ('owner','affiliated','independent','unknown')),
    judgement_json TEXT NOT NULL CHECK(json_valid(judgement_json)),
    input_tokens INTEGER NOT NULL CHECK(input_tokens >= 0),
    output_tokens INTEGER NOT NULL CHECK(output_tokens >= 0),
    assessed_at INTEGER NOT NULL,
    PRIMARY KEY(program_id, origin_key, subject_key, policy, model)
);

-- Judgement #3, per claim. The key hashes the statement with the subject
-- classes the call was offered (claim_profile_key), so it is not the
-- statement's own hash.
CREATE TABLE claim_profiles (
    input_sha256 TEXT NOT NULL,
    policy TEXT NOT NULL,
    model TEXT NOT NULL,
    profile_json TEXT NOT NULL CHECK(json_valid(profile_json)),
    input_tokens INTEGER NOT NULL CHECK(input_tokens >= 0),
    output_tokens INTEGER NOT NULL CHECK(output_tokens >= 0),
    assessed_at INTEGER NOT NULL,
    PRIMARY KEY(input_sha256, policy, model)
);

-- The provenance graph (§7, §8): an edge points at a captured version or at
-- a URL nobody captured, never both.
CREATE TABLE source_relations (
    program_id TEXT NOT NULL,
    from_version_id TEXT NOT NULL,
    to_version_id TEXT,
    to_url TEXT,
    kind TEXT NOT NULL CHECK(kind IN (
        'copy_of','contains_copy_of','archive_of','canonical_of','cites','cited_for_claim'
    )),
    evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)),
    detected_at INTEGER NOT NULL,
    CHECK((to_version_id IS NULL) <> (to_url IS NULL)),
    FOREIGN KEY(program_id, from_version_id)
        REFERENCES learning_source_versions(program_id, id) ON DELETE CASCADE,
    FOREIGN KEY(program_id, to_version_id)
        REFERENCES learning_source_versions(program_id, id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX source_relations_edge ON source_relations(
    program_id, from_version_id, kind, COALESCE(to_version_id, ''), COALESCE(to_url, '')
);
CREATE INDEX source_relations_target ON source_relations(program_id, to_version_id);

-- The user's overrides, one shape per scope:
--   source        one source in one course: any level, Primary included;
--   site          a registrable domain everywhere: a standing, never a level;
--   site_subject  a registrable domain for one subject class everywhere: any
--                 level, Primary included (Primary stays a relation to a
--                 subject).
CREATE TABLE source_trust_overrides (
    scope TEXT NOT NULL CHECK(scope IN ('source','site','site_subject')),
    program_id TEXT,
    source_id TEXT,
    registrable_domain TEXT,
    -- site_subject: SubjectKey of the class, and the class itself.
    class_key TEXT,
    class_json TEXT CHECK(class_json IS NULL OR json_valid(class_json)),
    level TEXT CHECK(level IS NULL OR level IN (
        'primary','authoritative','established','community','unknown','distrusted'
    )),
    standing TEXT CHECK(standing IS NULL OR standing IN (
        'institutional','identified','community','distrusted'
    )),
    note TEXT,
    set_at INTEGER NOT NULL,
    CHECK(CASE scope
        WHEN 'source' THEN program_id IS NOT NULL AND source_id IS NOT NULL
            AND registrable_domain IS NULL AND class_key IS NULL AND class_json IS NULL
            AND level IS NOT NULL AND standing IS NULL
        WHEN 'site' THEN program_id IS NULL AND source_id IS NULL
            AND registrable_domain IS NOT NULL AND class_key IS NULL AND class_json IS NULL
            AND level IS NULL AND standing IS NOT NULL
        WHEN 'site_subject' THEN program_id IS NULL AND source_id IS NULL
            AND registrable_domain IS NOT NULL AND class_key IS NOT NULL
            AND class_json IS NOT NULL AND json_extract(class_json, '$.key') = class_key
            AND level IS NOT NULL AND standing IS NULL
        ELSE 0
    END),
    -- Primary only for a source, or for a site within a subject class.
    CHECK(level IS NOT 'primary' OR scope IN ('source','site_subject')),
    FOREIGN KEY(program_id, source_id)
        REFERENCES learning_source_library(program_id, id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX source_trust_overrides_source
    ON source_trust_overrides(program_id, source_id) WHERE scope = 'source';
CREATE UNIQUE INDEX source_trust_overrides_site
    ON source_trust_overrides(registrable_domain) WHERE scope = 'site';
CREATE UNIQUE INDEX source_trust_overrides_site_subject
    ON source_trust_overrides(registrable_domain, class_key) WHERE scope = 'site_subject';

-- Labelled outcomes for calibration (§13). Kept when the lesson or claim
-- goes; the course takes them with it. `decision_right` is whether the gate's
-- decision was right.
CREATE TABLE source_trust_outcomes (
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
        'verifier_disagree','contrary_evidence'
    )),
    policy TEXT NOT NULL,
    at INTEGER NOT NULL
);
CREATE INDEX source_trust_outcomes_cell ON source_trust_outcomes(level, claim_kind, at);
CREATE INDEX source_trust_outcomes_origin ON source_trust_outcomes(origin_key, at);
