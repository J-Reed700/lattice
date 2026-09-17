import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';
import { conversationUiStore } from '@/stores/conversationUiStore';

import { useConversationsController } from '../useConversationsController';

const compact = VaultAPI.compactConversation as unknown as ReturnType<typeof vi.fn>;

const wrapper = ({ children }: { children: ReactNode }) => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
};

const CONVERSATION_ID = 'conv-compact';

describe('compactConversation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    conversationUiStore.setState({ error: null });
  });

  afterEach(() => {
    conversationUiStore.setState({ error: null });
  });

  it('returns the applied compaction record on success', async () => {
    compact.mockResolvedValue({
      ok: true,
      data: {
        compaction: {
          id: 'c1',
          conversationId: CONVERSATION_ID,
          summaryText: 'The user settled on a pricing model.',
          upToMessageId: 'm3',
          originalMessageCount: 6,
          originalTokens: 2400,
          summaryTokens: 200,
          compressionRatio: 0.083,
          createdAt: '2026-01-01T00:00:00.000Z',
        },
      },
    });
    const { result } = renderHook(() => useConversationsController(), { wrapper });

    const record = await result.current.compactConversation(CONVERSATION_ID);

    expect(compact).toHaveBeenCalledWith(CONVERSATION_ID, undefined);
    expect(record?.id).toBe('c1');
    expect(record?.upToMessageId).toBe('m3');
    expect(conversationUiStore.getState().error).toBeNull();
  });

  it('returns null and surfaces the error when the backend refuses', async () => {
    compact.mockResolvedValue({ ok: false, error: 'not enough history to compact' });
    const { result } = renderHook(() => useConversationsController(), { wrapper });

    const record = await result.current.compactConversation(CONVERSATION_ID);

    expect(record).toBeNull();
    expect(conversationUiStore.getState().error).toBe('not enough history to compact');
  });
});
