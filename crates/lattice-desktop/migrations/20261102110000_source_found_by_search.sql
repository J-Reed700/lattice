-- Whether the outline's research found a page, against a document or an address
-- the user chose when making the course: the sweep reads only what it found.
-- A web page saved before this column reads as found, as the sweep read every
-- outline page; a document can only be the user's choice.
ALTER TABLE learning_source_library ADD COLUMN found_by_search INTEGER NOT NULL DEFAULT 0 CHECK(found_by_search IN (0,1));
UPDATE learning_source_library SET found_by_search = 1 WHERE added_by = 'outline' AND kind = 'web';
