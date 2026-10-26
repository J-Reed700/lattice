-- When a deck's cards must be remembered by, such as an interview. Reviews
-- before then are spaced to fit the time left. The date is the learner's own,
-- so it sits beside the deck rather than on it and never travels in a pack.
CREATE TABLE study_deck_targets (
    deck_id TEXT PRIMARY KEY REFERENCES study_decks(id) ON DELETE CASCADE,
    target_at INTEGER NOT NULL
) STRICT;
