import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, LLMHealthStatus } from '@/types';
import type { ChatStarters } from '@/types/api/chatStarters';

export const qaApi = {
  /**
   * Checks if the LLM (Large Language Model) service is healthy and accessible.
   * Verifies API key, endpoint connectivity, and model availability.
   *
   * @returns Health status object with connectivity and model info
   */
  checkLLMHealth: async (): Promise<ApiResult<LLMHealthStatus>> =>
    apiCall<LLMHealthStatus>('check_llm_health'),

  /**
   * Three corpus-derived questions for the Chat empty state, drawn from the
   * documents `spaceId` can see. A null space means General.
   * Returns an empty `starters` array when no model could produce them —
   * the empty state renders no questions rather than inventing any.
   */
  generateChatStarters: async (
    spaceId?: string | null,
  ): Promise<ApiResult<ChatStarters>> =>
    apiCall<ChatStarters>('generate_chat_starters', {
      spaceId: spaceId ?? null,
    }),
};
