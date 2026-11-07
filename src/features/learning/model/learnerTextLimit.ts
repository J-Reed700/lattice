/**
 * The most characters of free text a learner may send a model: the bound a chat
 * message has. It mirrors `MAX_CHAT_MESSAGE_CHARS` in
 * `crates/lattice-desktop/src/infrastructure/security/input_validator.rs`, so a
 * field stops where the backend would refuse.
 */
export const LEARNER_TEXT_MAX_CHARS = 200_000;
