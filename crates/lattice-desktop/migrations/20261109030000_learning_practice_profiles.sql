-- Course-level practice profiles: target details, language and interview schedule.
CREATE TABLE learning_practice_profiles (
    program_id TEXT PRIMARY KEY REFERENCES learning_programs(id) ON DELETE CASCADE,
    details TEXT NOT NULL DEFAULT '',
    language TEXT,
    interview_at INTEGER,
    company TEXT,
    role TEXT,
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
    updated_at INTEGER NOT NULL
);
