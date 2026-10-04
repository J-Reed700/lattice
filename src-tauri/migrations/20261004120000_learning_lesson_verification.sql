-- Reports contain private answer-key claims: backend-only, never learner DTOs.
-- Existing lessons remain readable; all new preparation paths require a report.
CREATE TABLE learning_lesson_verifications (
    lesson_id TEXT PRIMARY KEY REFERENCES learning_lessons(id) ON DELETE CASCADE,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    policy TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    report_json TEXT NOT NULL CHECK(json_valid(report_json)),
    checked_at INTEGER NOT NULL
);
