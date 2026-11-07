-- The clean slate (claim classes design 3.9). Lesson preparation starts again
-- under the new pipeline: no old work resumes, and none of the old pipeline's
-- unpublished state is read. Programs, modules, lessons and outlines, the
-- source library and its provenance, the labelled examples of the checker,
-- and everything of a lesson that is ready are kept.

-- Unfinished preparation (pending, running, or waiting to retry) ends, with
-- the checkpoints it would have resumed from.
DELETE FROM job_checkpoints
 WHERE job_id IN (SELECT id FROM jobs
                   WHERE kind = 'learning.lesson_preparation'
                     AND status IN ('pending', 'running'));

UPDATE jobs
   SET status = 'cancelled',
       finished_at = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER),
       retry_not_before = NULL,
       result_ref = NULL,
       error_code = NULL,
       error_message = NULL,
       progress_message = 'Preparation starts again under the new pipeline'
 WHERE kind = 'learning.lesson_preparation'
   AND status IN ('pending', 'running');

INSERT INTO job_events (job_id, ordinal, status, progress_current, message, created_at)
SELECT j.id,
       (SELECT coalesce(max(ordinal), -1) + 1 FROM job_events WHERE job_id = j.id),
       'cancelled',
       j.progress_current,
       j.progress_message,
       j.finished_at
  FROM jobs j
 WHERE j.kind = 'learning.lesson_preparation'
   AND j.status = 'cancelled'
   AND j.progress_message = 'Preparation starts again under the new pipeline';

-- The drafts, pending repairs and claim inventories a preparation job staged
-- for a lesson that is not ready, and the checkpoints saved for it.
UPDATE jobs
   SET staged_result_json = json_set(
         staged_result_json,
         '$.drafts',
         json(coalesce((SELECT json_group_object(draft.key, json(draft.value))
                          FROM json_each(jobs.staged_result_json, '$.drafts') AS draft
                         WHERE draft.key IN (SELECT id FROM learning_lessons
                                              WHERE preparation = 'ready')),
                       '{}')))
 WHERE kind = 'learning.lesson_preparation'
   AND json_type(staged_result_json, '$.drafts') = 'object';

DELETE FROM job_checkpoints
 WHERE job_id IN (SELECT id FROM jobs WHERE kind = 'learning.lesson_preparation')
   AND instr(key, '/') > 0
   AND substr(key, 1, instr(key, '/') - 1)
       NOT IN (SELECT id FROM learning_lessons WHERE preparation = 'ready');

-- The claims, their events, the audits, the review row and the teaching
-- findings of every lesson that is not ready.
DELETE FROM learning_lesson_claim_events
 WHERE claim_id IN (SELECT claim_id FROM learning_lesson_claims
                     WHERE lesson_id IN (SELECT id FROM learning_lessons
                                          WHERE preparation <> 'ready'));
DELETE FROM learning_lesson_claims
 WHERE lesson_id IN (SELECT id FROM learning_lessons WHERE preparation <> 'ready');
DELETE FROM learning_lesson_audits
 WHERE lesson_id IN (SELECT id FROM learning_lessons WHERE preparation <> 'ready');
DELETE FROM learning_lesson_reviews
 WHERE lesson_id IN (SELECT id FROM learning_lessons WHERE preparation <> 'ready');
DELETE FROM learning_lesson_teaching_findings
 WHERE lesson_id IN (SELECT id FROM learning_lessons WHERE preparation <> 'ready');
