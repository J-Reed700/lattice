-- Code example claims saved before the ledger recorded the run each stands
-- for: the run's id is the one their statement names.
INSERT INTO learning_lesson_claim_events (claim_id, ordinal, actor, kind, payload_json, job_id, at)
SELECT e.claim_id,
       (SELECT MAX(m.ordinal) + 1 FROM learning_lesson_claim_events m WHERE m.claim_id = e.claim_id),
       'machine',
       'code_example',
       json_object(
           'event', json_object(
               'kind', 'code_example',
               'observation', substr(json_extract(e.payload_json, '$.event.statement'), 18,
                                     length(json_extract(e.payload_json, '$.event.statement')) - 62)),
           'appVersion', json_extract(e.payload_json, '$.appVersion'),
           'checkerModel', json_extract(e.payload_json, '$.checkerModel')),
       e.job_id,
       e.at
  FROM learning_lesson_claim_events e
 WHERE e.kind = 'extracted'
   AND e.ordinal = (SELECT MIN(f.ordinal) FROM learning_lesson_claim_events f WHERE f.claim_id = e.claim_id)
   AND json_extract(e.payload_json, '$.event.statement') LIKE 'The code example % runs to completion with the output it shows.'
   AND NOT EXISTS (SELECT 1 FROM learning_lesson_claim_events c
                    WHERE c.claim_id = e.claim_id AND c.kind = 'classified');
