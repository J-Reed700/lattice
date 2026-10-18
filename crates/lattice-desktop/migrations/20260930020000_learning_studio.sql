CREATE TABLE learning_programs (
  id TEXT PRIMARY KEY, title TEXT NOT NULL, goal TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('draft','active')),
  revision INTEGER NOT NULL, prior_knowledge TEXT NOT NULL, minutes_per_session INTEGER NOT NULL,
  model_name TEXT NOT NULL, current_lesson_id TEXT, created_at INTEGER NOT NULL
);
CREATE TABLE learning_modules (
  id TEXT PRIMARY KEY, program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL, title TEXT NOT NULL, summary TEXT NOT NULL, outcomes_json TEXT NOT NULL,
  UNIQUE(program_id, ordinal), UNIQUE(program_id, id)
);
CREATE TABLE learning_lessons (
  id TEXT PRIMARY KEY, program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
  module_id TEXT NOT NULL REFERENCES learning_modules(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL,
  title TEXT NOT NULL, objective TEXT NOT NULL, estimated_minutes INTEGER NOT NULL,
  preparation TEXT NOT NULL CHECK(preparation IN ('outline','ready')), completed INTEGER NOT NULL DEFAULT 0,
  UNIQUE(module_id, ordinal), UNIQUE(program_id, id)
);
CREATE TABLE learning_sources (
  id TEXT NOT NULL, program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
  title TEXT NOT NULL, url TEXT, excerpt TEXT NOT NULL, acquired_at INTEGER NOT NULL,
  PRIMARY KEY(program_id, id)
);
CREATE TABLE learning_blocks (
  lesson_id TEXT NOT NULL REFERENCES learning_lessons(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL,
  kind TEXT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL, source_ids_json TEXT NOT NULL,
  PRIMARY KEY(lesson_id, ordinal)
);
CREATE TABLE learning_questions (
  id TEXT PRIMARY KEY, lesson_id TEXT NOT NULL REFERENCES learning_lessons(id) ON DELETE CASCADE,
  module_id TEXT NOT NULL REFERENCES learning_modules(id) ON DELETE CASCADE, kind TEXT NOT NULL,
  prompt TEXT NOT NULL, options_json TEXT NOT NULL, source_ids_json TEXT NOT NULL, ordinal INTEGER NOT NULL,
  UNIQUE(lesson_id, kind, ordinal)
);
CREATE TABLE learning_answer_keys (
  question_id TEXT PRIMARY KEY REFERENCES learning_questions(id) ON DELETE CASCADE,
  correct_index INTEGER NOT NULL, explanation TEXT NOT NULL
);
CREATE TABLE learning_attempts (
  id TEXT PRIMARY KEY, program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
  module_id TEXT NOT NULL, lesson_id TEXT, kind TEXT NOT NULL, correct INTEGER NOT NULL, total INTEGER NOT NULL,
  answers_json TEXT NOT NULL, results_json TEXT NOT NULL, submitted_at INTEGER NOT NULL, payload_hash TEXT NOT NULL,
  FOREIGN KEY(program_id, module_id) REFERENCES learning_modules(program_id, id) ON DELETE CASCADE,
  FOREIGN KEY(program_id, lesson_id) REFERENCES learning_lessons(program_id, id) ON DELETE CASCADE
);
CREATE INDEX learning_attempts_program ON learning_attempts(program_id, submitted_at);
