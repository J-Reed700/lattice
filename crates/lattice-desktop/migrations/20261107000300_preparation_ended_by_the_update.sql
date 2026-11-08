-- Lesson preparation that failed, was paused or was interrupted under the
-- pipeline the clean slate replaced lost its unready lessons' drafts and
-- checkpoints to it. It ends the way the clean slate ended unfinished
-- preparation, and says an update ended it.

INSERT INTO job_events (job_id, ordinal, status, progress_current, message, created_at)
SELECT j.id,
       (SELECT coalesce(max(ordinal), -1) + 1 FROM job_events WHERE job_id = j.id),
       'cancelled',
       j.progress_current,
       'Preparation starts again under the new pipeline',
       CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
  FROM jobs j
 WHERE j.kind = 'learning.lesson_preparation'
   AND j.status IN ('failed', 'cancelled', 'interrupted')
   AND j.progress_message <> 'Preparation starts again under the new pipeline'
   AND j.finished_at < (SELECT CAST((julianday(installed_on) - 2440587.5) * 86400000 AS INTEGER)
                          FROM _sqlx_migrations WHERE version = 20261102100000);

UPDATE jobs
   SET status = 'cancelled',
       error_code = 'ended_by_update',
       error_message = NULL,
       cause_code = NULL,
       retry_not_before = NULL,
       result_ref = NULL,
       progress_message = 'Preparation starts again under the new pipeline'
 WHERE kind = 'learning.lesson_preparation'
   AND ((status = 'cancelled'
         AND progress_message = 'Preparation starts again under the new pipeline')
        OR (status IN ('failed', 'cancelled', 'interrupted')
            AND finished_at < (SELECT CAST((julianday(installed_on) - 2440587.5) * 86400000 AS INTEGER)
                                 FROM _sqlx_migrations WHERE version = 20261102100000)));
