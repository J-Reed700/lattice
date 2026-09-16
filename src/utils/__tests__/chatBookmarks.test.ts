import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { ConversationMessage } from '@/types/conversation';
import {
  parseBookmarkSourceReferences,
  resolveBookmarkPayload,
} from '@/utils/chatBookmarks';

const buildBookmark = (overrides: Partial<ConversationMessageBookmarkDto>): ConversationMessageBookmarkDto => ({
  id: 'bookmark_1',
  conversationId: 'conv_1',
  conversationTitle: 'Conversation 1',
  spaceId: 'space_general',
  messageId: 'msg_1',
  messageRole: 'assistant',
  messagePreview: 'Preview text',
  title: null,
  note: null,
  createdAt: '2026-02-19T10:00:00.000Z',
  ...overrides,
});

describe('chatBookmarks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('parses and deduplicates source references from metadata and direct sources', () => {
    const parsed = parseBookmarkSourceReferences(
      JSON.stringify({
        sources: [
          { documentId: 'doc_1', fileName: 'Alpha.pdf' },
          { documentId: 'doc_1', fileName: 'Alpha.pdf' },
        ],
      }),
      [
        { document_id: 'doc_2', file_path: '/tmp/beta.md' },
        { document_id: 'doc_2', file_path: '/tmp/beta.md' },
      ]
    );

    expect(parsed).toEqual([
      { documentId: 'doc_2', label: '/tmp/beta.md' },
      { documentId: 'doc_1', label: 'Alpha.pdf' },
    ]);
  });

  it('uses loaded conversation messages before calling the API', async () => {
    const bookmark = buildBookmark({});
    const loadedMessages: ConversationMessage[] = [
      {
        id: 'msg_1',
        conversationId: 'conv_1',
        role: 'assistant',
        content: 'Loaded content',
        tokens: 10,
        createdAt: '2026-02-19T10:00:00.000Z',
        status: 'completed',
        metadata: JSON.stringify({
          sources: [{ documentId: 'doc_9', fileName: 'Loaded.txt' }],
        }),
      },
    ];

    const payload = await resolveBookmarkPayload(bookmark, loadedMessages);
    expect(payload.content).toBe('Loaded content');
    expect(payload.sourceReferences).toEqual([{ documentId: 'doc_9', label: 'Loaded.txt' }]);
    expect(VaultAPI.getConversationMessages).not.toHaveBeenCalled();
  });

  it('throws when full conversation fetch fails', async () => {
    const bookmark = buildBookmark({ messagePreview: 'Fallback preview' });
    vi.mocked(VaultAPI.getConversationMessages).mockResolvedValueOnce({
      ok: false,
      error: 'conversation missing',
    });

    await expect(resolveBookmarkPayload(bookmark)).rejects.toThrow(
      'Unable to load full message content for reference capture: conversation missing'
    );
  });

  it('throws when message is not present in fetched conversation history', async () => {
    const bookmark = buildBookmark({});
    vi.mocked(VaultAPI.getConversationMessages).mockResolvedValueOnce({
      ok: true,
      data: { messages: [], total: 0 },
    });

    await expect(resolveBookmarkPayload(bookmark)).rejects.toThrow(
      'Unable to load full message content for reference capture: message was not found.'
    );
  });
});
