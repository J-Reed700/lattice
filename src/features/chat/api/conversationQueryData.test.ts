import { describe, expect, it, vi } from 'vitest';

import type { ApiClient } from '@/lib/api';

import { fetchBookmarks } from './conversationQueryData';

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
