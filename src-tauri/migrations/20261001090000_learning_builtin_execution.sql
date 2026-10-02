-- Add embedded execution without rewriting existing immutable container runs.
-- runtime_kind and its frozen container columns retain their original contract;
-- builtin_runtime identifies an alternative provider only when those are absent.
ALTER TABLE learning_practical_activities ADD COLUMN builtin_runtime TEXT
    CHECK(builtin_runtime IS NULL OR (builtin_runtime IN ('javascript','python') AND runtime_kind='none'));
ALTER TABLE learning_practical_runs ADD COLUMN builtin_runtime TEXT
    CHECK(builtin_runtime IS NULL OR (builtin_runtime IN ('javascript','python') AND engine IS NULL AND image_id IS NULL));
ALTER TABLE learning_practical_activities ADD COLUMN builtin_runtime_version TEXT
    CHECK((builtin_runtime IS NULL AND builtin_runtime_version IS NULL)
       OR (builtin_runtime IS NOT NULL AND builtin_runtime_version IS NOT NULL AND length(builtin_runtime_version) BETWEEN 1 AND 128));
ALTER TABLE learning_practical_runs ADD COLUMN builtin_runtime_version TEXT
    CHECK((builtin_runtime IS NULL AND builtin_runtime_version IS NULL)
       OR (builtin_runtime IS NOT NULL AND builtin_runtime_version IS NOT NULL AND length(builtin_runtime_version) BETWEEN 1 AND 128));
