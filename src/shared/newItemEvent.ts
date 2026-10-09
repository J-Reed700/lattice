/**
 * Fired on ⌘N. Surfaces that can create something (Chat, Journal) listen for
 * it and create in place; everywhere else ⌘N opens Chat with `?new=1`, which
 * the Chat surface turns into a fresh conversation.
 */
export const NEW_ITEM_EVENT = 'lattice:new';
