-- Where a job stands in its course's backlog, so the inference scheduler serves a course's lessons in course order
-- (docs: course preparation, piece 4). Set when a ranked kind's job is admitted, copied to its retries; lower runs
-- first. Jobs saved before this carry none and are served as unranked work.

ALTER TABLE jobs ADD COLUMN rank INTEGER;
