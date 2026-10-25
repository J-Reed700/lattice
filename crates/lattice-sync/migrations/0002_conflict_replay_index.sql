-- A retry of a rejected operation must find its original conflict even after
-- resolution. Index the exact replay identity rather than scanning all of a
-- user's conflicts through the status-list index.
CREATE INDEX idx_conflicts_replay
    ON conflicts (user_id, server_op_seq, incoming_device_id, incoming_client_op_id);
