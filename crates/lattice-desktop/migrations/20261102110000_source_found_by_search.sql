-- Whether the course's outline research found a page, as against a document or
-- an address the user chose when making the course: both are saved as the
-- outline's (`added_by`). The admission sweep (claim classes design 3.5(4))
-- reads what the machine found and never what the user chose. Rows made before
-- this column keep 0: nothing says who chose them, and a page the user may have
-- chosen is not refused.
ALTER TABLE learning_source_library ADD COLUMN found_by_search INTEGER NOT NULL DEFAULT 0 CHECK(found_by_search IN (0,1));
