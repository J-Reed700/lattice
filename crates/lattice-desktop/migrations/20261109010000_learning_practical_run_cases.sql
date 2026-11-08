-- Learning practical run cases and coding session associations.

ALTER TABLE learning_practical_runs ADD COLUMN coding_session_id TEXT;
ALTER TABLE learning_practical_runs ADD COLUMN scope TEXT CHECK(scope IS NULL OR scope IN ('examples','all','input'));
ALTER TABLE learning_practical_run_checks ADD COLUMN detail_json TEXT;
