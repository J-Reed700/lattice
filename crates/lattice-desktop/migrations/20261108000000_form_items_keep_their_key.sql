-- A form keeps the answer key, explanation and wrong-choice reasons it was drawn with, so editing a
-- question in the bank does not regrade or contradict a quiz that is already open. Candidates never
-- changed before this, so each existing form item takes what its candidate has now. The three are
-- written together: a NULL key_json means the form has no copy and reads the bank.
ALTER TABLE learning_assessment_form_items ADD COLUMN key_json TEXT;
ALTER TABLE learning_assessment_form_items ADD COLUMN explanation TEXT;
ALTER TABLE learning_assessment_form_items ADD COLUMN distractor_rationales_json TEXT;

UPDATE learning_assessment_form_items
SET key_json = (
        SELECT k.key_json
        FROM learning_assessment_answer_keys k
        WHERE k.candidate_id = learning_assessment_form_items.candidate_id
    ),
    explanation = (
        SELECT k.explanation
        FROM learning_assessment_answer_keys k
        WHERE k.candidate_id = learning_assessment_form_items.candidate_id
    ),
    distractor_rationales_json = (
        SELECT k.distractor_rationales_json
        FROM learning_assessment_answer_keys k
        WHERE k.candidate_id = learning_assessment_form_items.candidate_id
    );
