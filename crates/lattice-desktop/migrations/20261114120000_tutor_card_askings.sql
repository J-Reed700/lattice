-- A card the tutor asked through ask_card, at the review count it had then.
-- A recall is recorded only for a card asked since its last review.
CREATE TABLE tutor_card_askings (
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    card_id TEXT NOT NULL REFERENCES study_cards(id) ON DELETE CASCADE,
    review_count INTEGER NOT NULL,
    asked_at INTEGER NOT NULL,
    PRIMARY KEY (conversation_id, card_id, review_count)
);
