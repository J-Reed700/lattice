-- What a conversation has shown its model, under the names it gave each thing:
-- a name keeps its meaning for the whole conversation, so a later turn's
-- reply or tool call resolves it. `position` is the order the names were
-- given in. No message column: a name outlives the message that first showed it.
CREATE TABLE conversation_names (
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    prefix TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    under_prefix TEXT,
    under_ordinal INTEGER,
    reference TEXT NOT NULL,
    PRIMARY KEY (conversation_id, position)
);
