import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useWeeklySynthesisCandidatesQuery } from '../useWeeklySynthesisCandidatesQuery';

const listConversations = vi.fn();
const listPassageReferences = vi.fn();
const listWorkspaceNotes = vi.fn();

vi.mock('@/lib/api', () => {
  const api = {
    listConversations: () => listConversations(),
    listPassageReferences: (...args: unknown[]) => listPassageReferences(...args),
    listWorkspaceNotes: () => listWorkspaceNotes(),
  };
  return { VaultAPI: api, default: api };
});

const wrapper = ({ children }: { children: ReactNode }) => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
};

const recently = () => new Date(Date.now() - 60_000).toISOString();

function conversations(count: number) {
  return Array.from({ length: count }, (_, index) => ({
    id: `conv_${index}`,
    isArchived: false,
    updatedAt: recently(),
  }));
}

function passages(count: number) {
  return Array.from({ length: count }, (_, index) => ({
    id: `pref_${index}`,
    createdAt: recently(),
  }));
}

function notes(count: number) {
  return Array.from({ length: count }, (_, index) => ({
    id: `note_${index}`,
    title: `Page ${index}`,
    content: 'something',
    updatedAt: recently(),
  }));
}

describe('useWeeklySynthesisCandidatesQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('counts only what the last seven days hold', async () => {
    const old = new Date(Date.now() - 30 * 86_400_000).toISOString();
    listConversations.mockResolvedValue({
      ok: true,
      data: {
        conversations: [
          { id: 'a', isArchived: false, updatedAt: recently() },
          { id: 'b', isArchived: false, updatedAt: old },
          { id: 'c', isArchived: true, updatedAt: recently() },
        ],
      },
    });
    listPassageReferences.mockResolvedValue({ ok: true, data: passages(2) });
    listWorkspaceNotes.mockResolvedValue({
      ok: true,
      data: {
        notes: [
          ...notes(1),
          // An empty page and the week page itself are not sources.
          { id: 'blank', title: 'Blank', content: '   ', updatedAt: recently() },
          { id: 'week', title: 'Week of Sep 1', content: 'x', updatedAt: recently() },
        ],
      },
    });

    const { result } = renderHook(() => useWeeklySynthesisCandidatesQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(result.current.data).toEqual({
      conversations: 1,
      references: 2,
      notes: 1,
      total: 4,
    });
  });

  it('never promises more than a run reads (12 / 20 / 8)', async () => {
    listConversations.mockResolvedValue({ ok: true, data: { conversations: conversations(30) } });
    listPassageReferences.mockResolvedValue({ ok: true, data: passages(50) });
    listWorkspaceNotes.mockResolvedValue({ ok: true, data: { notes: notes(20) } });

    const { result } = renderHook(() => useWeeklySynthesisCandidatesQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(result.current.data).toEqual({
      conversations: 12,
      references: 20,
      notes: 8,
      total: 40,
    });
  });

  it('counts a failing call as nothing rather than failing the row', async () => {
    listConversations.mockResolvedValue({ ok: false, error: 'nope' });
    listPassageReferences.mockResolvedValue({ ok: false, error: 'nope' });
    listWorkspaceNotes.mockResolvedValue({ ok: false, error: 'nope' });

    const { result } = renderHook(() => useWeeklySynthesisCandidatesQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(result.current.data?.total).toBe(0);
  });
});
