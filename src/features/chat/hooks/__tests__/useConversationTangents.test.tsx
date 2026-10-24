import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationKeys } from '@/shared/conversations/conversationKeys';

import { useConversationTangentsQuery, usePromoteTangentMutation } from '../useConversationTangents';

const h = vi.hoisted(() => ({ api: { listConversationTangents: vi.fn(), promoteConversationTangent: vi.fn() } }));
vi.mock('@/lib/api', () => ({ VaultAPI: h.api, default: h.api }));

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
const tangent = { conversationId: 't1', parentConversationId: 'p1' } as never;

describe('usePromoteTangentMutation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  });

  it('seeds the new conversation and refreshes the lists', async () => {
    vi.spyOn(client, 'invalidateQueries');
    h.api.promoteConversationTangent.mockResolvedValueOnce({ ok: true, data: { id: 't1', title: 'Silk Road' } });
    const { result } = renderHook(() => usePromoteTangentMutation(), { wrapper });
    await act(async () => { await result.current.mutateAsync(tangent); });
    expect(h.api.promoteConversationTangent).toHaveBeenCalledWith('t1');
    expect(client.getQueryData(conversationKeys.detail('t1'))).toEqual({ id: 't1', title: 'Silk Road' });
    expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: conversationKeys.lists });
  });

  it('throws a refused promotion', async () => {
    h.api.promoteConversationTangent.mockResolvedValueOnce({ ok: false, error: 'tangent gone' });
    const { result } = renderHook(() => usePromoteTangentMutation(), { wrapper });
    await act(async () => { await expect(result.current.mutateAsync(tangent)).rejects.toThrow('tangent gone'); });
  });

  it('lists a conversation’s tangents', async () => {
    h.api.listConversationTangents.mockResolvedValueOnce({ ok: true, data: [tangent] });
    const { result } = renderHook(() => useConversationTangentsQuery('p1'), { wrapper });
    await vi.waitFor(() => expect(result.current.data).toEqual([tangent]));
    expect(h.api.listConversationTangents).toHaveBeenCalledWith('p1');
  });
});
