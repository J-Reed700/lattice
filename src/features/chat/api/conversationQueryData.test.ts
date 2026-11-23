import { describe, expect, it, vi } from 'vitest';

import type { ApiClient } from '@/lib/api';

import { fetchBookmarks, toConversationMessage } from './conversationQueryData';

describe('fetchBookmarks', () => {
  it('asks for every bookmark of the conversation, with no limit or offset', async () => {
    const listMessageBookmarks = vi.fn().mockResolvedValue({ ok: true, data: { bookmarks: [{ id: 'b1' }] } });
    const api = { listMessageBookmarks } as unknown as ApiClient;

    const bookmarks = await fetchBookmarks('conv-1', ' notes ', api);

    expect(bookmarks).toEqual([{ id: 'b1' }]);
    const request = listMessageBookmarks.mock.calls[0]?.[0];
    expect(request).toEqual({ conversationId: 'conv-1', query: 'notes', limit: null, offset: null });
  });
});

describe('toConversationMessage', () => {
  it('extracts persisted error details from metadata onto failed messages', () => {
    const failed = toConversationMessage({
      id: 'm-fail',
      conversationId: 'conv-1',
      role: 'user',
      content: 'Why?',
      createdAt: '2026-09-30T00:00:00Z',
      status: 'failed',
      tokens: 1,
      metadata: JSON.stringify({ requestId: 'req-1', error: 'Model quota exceeded' }),
    });
    expect(failed.error).toBe('Model quota exceeded');

    const completed = toConversationMessage({
      id: 'm-ok',
      conversationId: 'conv-1',
      role: 'user',
      content: 'Why?',
      createdAt: '2026-09-30T00:00:00Z',
      status: 'completed',
      tokens: 1,
      metadata: JSON.stringify({ cancelled: true, error: 'ignored on completed' }),
    });
    expect(completed.error).toBeUndefined();
  });
});
