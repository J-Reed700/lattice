-- The admission gate's verdict on a saved page (claim classes design 3.5(4)):
-- the reason it is not a source, and the gate version that read it. A page the
-- gate refused stays in the library, for the user to delete, but leaves
-- retrieval and citation: nothing is compared with it again.
ALTER TABLE learning_source_versions ADD COLUMN not_a_source TEXT NULL;
ALTER TABLE learning_source_versions ADD COLUMN admission_policy TEXT NULL;
