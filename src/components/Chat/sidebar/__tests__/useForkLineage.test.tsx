import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Conversation } from '@/types/conversation';

import { useForkLineage } from '../useForkLineage';

const getConversation = vi.hoisted(() => vi.fn());

vi.mock('@/lib/api', () => ({
  VaultAPI: { getConversation },
  default: { getConversation },
}));

const conversation = (id: string, title: string, extra: Partial<Conversation> = {}): Conversation => ({
  id,
  title,
  updatedAt: '2026-09-19T14:00:00.000Z',
  ...extra,
});

function renderLineage(conversations: Conversation[]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return renderHook(() => useForkLineage(conversations), {
    wrapper: ({ children }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
  });
}

describe('useForkLineage', () => {
  beforeEach(() => {
    getConversation.mockReset().mockResolvedValue({ ok: true, data: { conversation: null } });
  });

  it('names the parent a branch came from, and the turn it forked at', () => {
    const { result } = renderLineage([
      conversation('conv-parent', 'How much cooling does canopy buy?'),
      conversation('conv-branch', 'Narrow streets', {
        forkedFromConversationId: 'conv-parent',
        forkedFromMessageId: 'conv-parent-m4',
      }),
    ]);

    expect(result.current.get('conv-branch')).toEqual({
      id: 'conv-parent',
      title: 'How much cooling does canopy buy?',
      messageId: 'conv-parent-m4',
    });
    // The parent is already on screen, so nothing needs fetching.
    expect(getConversation).not.toHaveBeenCalled();
  });

  it('fetches a parent the current filter is hiding', async () => {
    getConversation.mockResolvedValue({
      ok: true,
      data: { conversation: { id: 'conv-archived', title: 'The archived original' } },
    });

    const { result } = renderLineage([
      conversation('conv-branch', 'Narrow streets', { forkedFromConversationId: 'conv-archived' }),
    ]);

    await waitFor(() => expect(result.current.get('conv-branch')?.title).toBe('The archived original'));
    expect(getConversation).toHaveBeenCalledWith('conv-archived');
  });

  it('shows nothing for a parent that has been deleted', async () => {
    const { result } = renderLineage([
      conversation('conv-branch', 'Narrow streets', { forkedFromConversationId: 'conv-gone' }),
    ]);

    await waitFor(() => expect(getConversation).toHaveBeenCalledWith('conv-gone'));
    expect(result.current.has('conv-branch')).toBe(false);
  });

  it('shows nothing when the lookup itself failed: a fault is not a lineage', async () => {
    getConversation.mockResolvedValue({ ok: false, error: 'database is locked' });

    const { result } = renderLineage([
      conversation('conv-branch', 'Narrow streets', { forkedFromConversationId: 'conv-parent' }),
    ]);

    await waitFor(() => expect(getConversation).toHaveBeenCalled());
    expect(result.current.has('conv-branch')).toBe(false);
  });

  it('leaves an ordinary conversation alone', () => {
    const { result } = renderLineage([conversation('conv-1', 'Plain thread')]);
    expect(result.current.size).toBe(0);
  });

  it('ignores a conversation that claims to be its own parent', () => {
    const { result } = renderLineage([
      conversation('conv-1', 'Plain thread', { forkedFromConversationId: 'conv-1' }),
    ]);

    expect(result.current.size).toBe(0);
    expect(getConversation).not.toHaveBeenCalled();
  });
});
