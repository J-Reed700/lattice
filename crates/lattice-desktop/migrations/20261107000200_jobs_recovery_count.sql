-- A job that keeps ending the process stops being resumed (docs: never-stuck design, item 2).
--
-- `saved_work` is bumped whenever a running job saves something: a checkpoint, advanced progress, a staged result.
-- Each recovery that resumes a job leaves `recovery_mark` at the `saved_work` it saw and sets `unsaved_starts`. A
-- recovery that finds the job running with `saved_work` still at its mark, and `unsaved_starts` set, fails it. An
-- orderly end (suspend, deferral) clears `unsaved_starts`.

ALTER TABLE jobs ADD COLUMN saved_work INTEGER NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN recovery_mark INTEGER NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN unsaved_starts INTEGER NOT NULL DEFAULT 0;
