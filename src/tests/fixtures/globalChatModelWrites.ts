import { expect, vi } from 'vitest';

/**
 * The app-wide chat model is set in Settings and nowhere else. Chat tests
 * stub every global write through here and assert none of them happened, so
 * the names of those writes stay out of chat code entirely.
 */
const GLOBAL_CHAT_MODEL_WRITES = ['updateSettings', 'setActiveChatModel', 'clearActiveChatModel'] as const;

export function stubGlobalChatModelWrites(api: Record<string, unknown>): () => void {
  for (const name of GLOBAL_CHAT_MODEL_WRITES) api[name] = vi.fn();
  return () => {
    for (const name of GLOBAL_CHAT_MODEL_WRITES) expect(api[name], name).not.toHaveBeenCalled();
  };
}
