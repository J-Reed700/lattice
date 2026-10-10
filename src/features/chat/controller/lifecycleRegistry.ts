export interface ConversationLifecycleRegistry {
  register: (conversationId: string, cleanup: () => void) => () => void;
  cleanupConversation: (conversationId: string) => void;
  cleanupAll: () => void;
}

export function createConversationLifecycleRegistry(): ConversationLifecycleRegistry {
  const cleanupsByConversation = new Map<string, Set<() => void>>();

  const register = (conversationId: string, cleanup: () => void): (() => void) => {
    const cleanups = cleanupsByConversation.get(conversationId) ?? new Set<() => void>();
    cleanups.add(cleanup);
    cleanupsByConversation.set(conversationId, cleanups);

    return () => {
      const current = cleanupsByConversation.get(conversationId);
      current?.delete(cleanup);
      if (current?.size === 0) cleanupsByConversation.delete(conversationId);
    };
  };

  const cleanupConversation = (conversationId: string): void => {
    const cleanups = cleanupsByConversation.get(conversationId);
    cleanupsByConversation.delete(conversationId);
    cleanups?.forEach(cleanup => cleanup());
  };

  const cleanupAll = (): void => {
    [...cleanupsByConversation.keys()].forEach(cleanupConversation);
  };

  return { register, cleanupConversation, cleanupAll };
}
