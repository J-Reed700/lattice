-- A teaching finding knows the sentence it is about, and can be resolved.
--
-- `sentence` is the whole sentence a sentence-level quotation finding is about. A row recorded before this column
-- has none; it is read back with the sentence its note completes, when it completes one.
--
-- A waiting finding is resolved when the user takes the sentence out of the lesson ('removed') or adds a source that
-- supports it ('source_added'). `resolution` and `resolved_at` are set together, and only in the 'resolved' state.
--
-- The state's CHECK names the states, so adding 'resolved' rebuilds the table: copy, drop, rename. Nothing refers to
-- this table, so the drop cascades to no other rows.

CREATE TABLE learning_lesson_teaching_findings_resolution (
  lesson_id TEXT NOT NULL REFERENCES learning_lessons(id) ON DELETE CASCADE,
  finding_key TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('teaching','answer_key','structure','quotation')),
  site TEXT NOT NULL CHECK(site IN ('lesson','section','question')),
  unit_index INTEGER CHECK((site = 'lesson') = (unit_index IS NULL) AND (unit_index IS NULL OR unit_index >= 0)),
  unit_title TEXT NOT NULL,
  unit_sha256 TEXT NOT NULL,
  note TEXT NOT NULL,
  summary TEXT,
  passage TEXT,
  sentence TEXT,
  repairs INTEGER NOT NULL CHECK(repairs >= 0),
  state TEXT NOT NULL CHECK(state IN ('waiting','kept','resolved')),
  job_id TEXT,
  found_at INTEGER NOT NULL,
  kept_at INTEGER CHECK((state = 'kept') = (kept_at IS NOT NULL)),
  resolution TEXT CHECK(resolution IN ('removed','source_added')),
  resolved_at INTEGER,
  CHECK((state = 'resolved') = (resolution IS NOT NULL)),
  CHECK((state = 'resolved') = (resolved_at IS NOT NULL)),
  PRIMARY KEY(lesson_id, finding_key)
);

INSERT INTO learning_lesson_teaching_findings_resolution
  (lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, summary, passage, repairs, state, job_id, found_at, kept_at)
SELECT lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, summary, passage, repairs, state, job_id, found_at, kept_at
FROM learning_lesson_teaching_findings;

DROP TABLE learning_lesson_teaching_findings;

ALTER TABLE learning_lesson_teaching_findings_resolution RENAME TO learning_lesson_teaching_findings;
