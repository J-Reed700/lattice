import type { Conversation } from '@/types/conversation';

/**
 * A conversation that lives beside what it is bound to: a folder's thread in
 * the Explorer, a course's tutor chat in the course. Chat leaves them out.
 */
export const isBoundChat = (conversation: Pick<Conversation, 'explorerRoot' | 'programId'>): boolean =>
  Boolean(conversation.explorerRoot || conversation.programId);
