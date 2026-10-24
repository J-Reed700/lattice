import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, LLMHealthStatus } from '@/types';
import type { ChatStarters } from '@/types/api/chatStarters';

export const qaApi = {
  checkLLMHealth: async (): Promise<ApiResult<LLMHealthStatus>> =>
    apiCall<LLMHealthStatus>('check_llm_health'),

  /** Generates starter questions for the empty chat state from visible documents. */
  generateChatStarters: async (
    spaceId?: string | null,
  ): Promise<ApiResult<ChatStarters>> =>
    apiCall<ChatStarters>('generate_chat_starters', {
      spaceId: spaceId ?? null,
    }),
};
