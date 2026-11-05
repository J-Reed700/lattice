-- Code review drills (docs/design/2026-11-04-code-review-drills.md, section 2): the bank of built drills.
--
-- A drill is code with planted, proven defects and the key that lists them. The suite and the killing tests inside
-- the key's proofs are what a learner's fix is later run against; the learner never reads them.

CREATE TABLE learning_drills (
    id TEXT PRIMARY KEY,
    language TEXT NOT NULL,
    title TEXT NOT NULL,
    -- What the code is about.
    domain TEXT NOT NULL,
    -- The course whose topic gave the domain, when one did. Deleting the course leaves the drill.
    program_id TEXT REFERENCES learning_programs(id) ON DELETE SET NULL,
    spec_json TEXT NOT NULL CHECK(json_valid(spec_json)),
    -- The code before any issue was planted.
    clean_files_json TEXT NOT NULL CHECK(json_valid(clean_files_json)),
    -- The code the learner reviews.
    files_json TEXT NOT NULL CHECK(json_valid(files_json)),
    key_json TEXT NOT NULL CHECK(json_valid(key_json)),
    -- How the drill was made: the repairs of the clean review, each proof, the mutations dropped and why, the blind
    -- review and its assessment, and the model of every call.
    evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)),
    -- The hidden suite, and what it and the proofs ran on (`builtin-python`). Both are null without a runner.
    suite TEXT,
    runner TEXT,
    -- The model that built the drill, as the server reports it.
    model TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('ready','taken')),
    created_at INTEGER NOT NULL,
    taken_at INTEGER,
    CHECK((status = 'ready' AND taken_at IS NULL) OR (status = 'taken' AND taken_at IS NOT NULL)),
    CHECK((suite IS NULL) = (runner IS NULL))
) STRICT;

-- The oldest ready drill of a language.
CREATE INDEX learning_drills_bank ON learning_drills(language, status, created_at);
-- The domain of a language used least recently.
CREATE INDEX learning_drills_domains ON learning_drills(language, domain, created_at);
-- Deleting a course finds its drills.
CREATE INDEX learning_drills_program ON learning_drills(program_id) WHERE program_id IS NOT NULL;

-- The patterns a drill's key holds, for the bank's lookups.
CREATE TABLE learning_drill_patterns (
    drill_id TEXT NOT NULL REFERENCES learning_drills(id) ON DELETE CASCADE,
    pattern_id TEXT NOT NULL,
    PRIMARY KEY(drill_id, pattern_id)
) STRICT;

CREATE INDEX learning_drill_patterns_pattern ON learning_drill_patterns(pattern_id, drill_id);
