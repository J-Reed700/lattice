-- The wire code of the error that made a job fail or defer (`ErrorCode`, e.g. NETWORK_ERROR), beside `error_code`,
-- which is the job runtime's own outcome vocabulary. NULL for rows written before this and for outcomes with no error.
ALTER TABLE jobs ADD COLUMN cause_code TEXT;
