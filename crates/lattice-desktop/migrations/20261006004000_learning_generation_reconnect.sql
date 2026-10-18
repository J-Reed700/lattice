-- A disconnected model request is deferred work, not a failed lesson review.
-- Keep the next delivery time with the outbox so sleep/restart preserves it.
ALTER TABLE learning_generation_jobs ADD COLUMN retry_not_before INTEGER;
ALTER TABLE learning_generation_jobs ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0
    CHECK (retry_count >= 0);
