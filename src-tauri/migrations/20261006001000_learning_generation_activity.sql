ALTER TABLE learning_generation_jobs ADD COLUMN activity_json TEXT
    CHECK (activity_json IS NULL OR json_valid(activity_json));
