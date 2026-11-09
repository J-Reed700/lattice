CREATE TABLE IF NOT EXISTS learning_scorecards (
    session_id TEXT NOT NULL,
    criterion_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('observed', 'insufficient_evidence')),
    points INTEGER,
    max_points INTEGER NOT NULL,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    feedback TEXT NOT NULL,
    next_step TEXT NOT NULL,
    PRIMARY KEY (session_id, criterion_id)
);

CREATE TABLE learning_drill_pattern_schedules_new (
    material TEXT NOT NULL CHECK(material = 'passage' OR material GLOB 'code:?*' OR material GLOB 'problem:?*'),
    pattern_id TEXT NOT NULL,
    stability REAL NOT NULL,
    difficulty REAL NOT NULL,
    last_reviewed_at INTEGER NOT NULL,
    due_at INTEGER NOT NULL,
    interval_days INTEGER NOT NULL CHECK(interval_days >= 0),
    review_count INTEGER NOT NULL CHECK(review_count >= 1),
    lapses INTEGER NOT NULL CHECK(lapses >= 0),
    PRIMARY KEY(material, pattern_id)
) STRICT;

INSERT INTO learning_drill_pattern_schedules_new
SELECT material, pattern_id, stability, difficulty, last_reviewed_at, due_at, interval_days, review_count, lapses
FROM learning_drill_pattern_schedules;

DROP TABLE learning_drill_pattern_schedules;
ALTER TABLE learning_drill_pattern_schedules_new RENAME TO learning_drill_pattern_schedules;
