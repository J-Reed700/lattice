-- Teaching-review findings the repairs did not settle. The lesson publishes,
-- and each finding waits for the user in the lesson's review panel, with the
-- reviewer's note and the part of the lesson it is about, until the user
-- keeps the text as written.
--
-- A finding is identified by its kind and place (`finding_key`, such as
-- `teaching:section-3`), never by its wording, which the reviewer rewrites
-- every pass. `unit_sha256` binds it to the text of that part: while the text
-- is unchanged the machine does not repair it again, and a "keep as written"
-- stands.
CREATE TABLE learning_lesson_teaching_findings (
  lesson_id TEXT NOT NULL REFERENCES learning_lessons(id) ON DELETE CASCADE,
  finding_key TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('teaching','answer_key','structure','quotation')),
  site TEXT NOT NULL CHECK(site IN ('lesson','section','question')),
  unit_index INTEGER CHECK((site = 'lesson') = (unit_index IS NULL) AND (unit_index IS NULL OR unit_index >= 0)),
  unit_title TEXT NOT NULL,
  unit_sha256 TEXT NOT NULL,
  note TEXT NOT NULL,
  passage TEXT,
  repairs INTEGER NOT NULL CHECK(repairs >= 0),
  state TEXT NOT NULL CHECK(state IN ('waiting','kept')),
  job_id TEXT,
  found_at INTEGER NOT NULL,
  kept_at INTEGER CHECK((state = 'kept') = (kept_at IS NOT NULL)),
  PRIMARY KEY(lesson_id, finding_key)
);
