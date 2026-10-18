-- Teaching criteria travel with their immutable lesson and attempt snapshots.
ALTER TABLE learning_blocks ADD COLUMN rubric_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(rubric_json));
ALTER TABLE learning_practice_sessions ADD COLUMN task_kind TEXT NOT NULL DEFAULT 'independent' CHECK (task_kind IN ('independent','guided'));
ALTER TABLE learning_practice_sessions ADD COLUMN revises_session_id TEXT REFERENCES learning_practice_sessions(id);
-- Private diagnostic keys never cross the renderer boundary.
ALTER TABLE learning_diagnostic_attempts ADD COLUMN evaluation_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(evaluation_json));
ALTER TABLE learning_diagnostic_attempts ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE learning_modules ADD COLUMN prerequisite_ids_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(prerequisite_ids_json));
ALTER TABLE learning_modules ADD COLUMN project_json TEXT NOT NULL DEFAULT 'null' CHECK (json_valid(project_json));
