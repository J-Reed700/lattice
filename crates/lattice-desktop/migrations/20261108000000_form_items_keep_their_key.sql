-- A form keeps the answer key it was drawn with, so editing a question in the bank does not regrade
-- a quiz that is already open. Candidates never changed before this, so each existing form item
-- takes the key its candidate has now.
ALTER TABLE learning_assessment_form_items ADD COLUMN key_json TEXT;

UPDATE learning_assessment_form_items
SET key_json = (
    SELECT k.key_json
    FROM learning_assessment_answer_keys k
    WHERE k.candidate_id = learning_assessment_form_items.candidate_id
);
