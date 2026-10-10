import type { ConversationMessage, OptimisticMessage } from '@/types/conversation';

// Reserve space before sending so failures can always keep the unsaved prompt.
// Once full, ask the reader to retry or dismiss; never silently evict their text.
export const MAX_UNSAVED_PROMPTS = 20;
export const MAX_UNSAVED_PROMPT_BYTES = 2 * 1024 * 1024;

export function prepareOptimisticMessages(
  messages: Map<string, OptimisticMessage>,
  conversationId: string,
  content: string,
  replacingFailedTempId?: string
): Map<string, OptimisticMessage> {
  const next = new Map(messages);
  if (replacingFailedTempId) {
    const failed = next.get(replacingFailedTempId);
    if (failed?.conversationId === conversationId && failed.role === 'user' && failed.status === 'failed') {
      next.delete(replacingFailedTempId);
    }
  }
  const prompts = [...next.values()].filter(message => message.role === 'user');
  const bytes = prompts.reduce((total, message) => total + message.content.length * 2, content.length * 2);
  if (prompts.length >= MAX_UNSAVED_PROMPTS || bytes > MAX_UNSAVED_PROMPT_BYTES) {
    throw new Error('Unsent messages are full. Retry or dismiss a failed message, or shorten this message before sending.');
  }
  return next;
}

/** Match the actual request, never an older question with the same wording. */
export function persistedRequestIds(messages: ConversationMessage[]): Set<string> {
  const ids = new Set<string>();
  for (const message of messages) {
    if (message.role !== 'user' || !message.metadata) continue;
    try {
      const metadata: unknown = JSON.parse(message.metadata);
      if (metadata && typeof metadata === 'object' && 'requestId' in metadata
        && typeof metadata.requestId === 'string') ids.add(metadata.requestId);
    } catch { /* Malformed historical metadata must not drop an unsaved prompt. */ }
  }
  return ids;
}

export function reconcilePersistedFailedMessages(
  optimisticMessages: Map<string, OptimisticMessage>,
  conversationId: string,
  messages: ConversationMessage[]
): Map<string, OptimisticMessage> {
  const persisted = persistedRequestIds(messages);
  const next = new Map(optimisticMessages);
  for (const [tempId, message] of next) {
    if (message.conversationId === conversationId && message.status === 'failed'
      && message.requestId && persisted.has(message.requestId)) next.delete(tempId);
  }
  return next.size === optimisticMessages.size ? optimisticMessages : next;
}
