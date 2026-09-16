import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor, act } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ConversationMessageBookmarkDto } from '@/types';
import type { PassageReferenceDto } from '@/types/api/references';

import { useReferenceInbox } from '../useReferenceInbox';


const listMessageBookmarks = vi.fn();
const listWorkspaceNotes = vi.fn();
const listConversationSpaces = vi.fn();
const listJournals = vi.fn();
const listPassageReferences = vi.fn();

vi.mock('@/lib/api', () => {
  const api = {
    listMessageBookmarks: (...args: unknown[]) => listMessageBookmarks(...args),
    listWorkspaceNotes: () => listWorkspaceNotes(),
    listConversationSpaces: () => listConversationSpaces(),
    listJournals: () => listJournals(),
    listPassageReferences: (...args: unknown[]) => listPassageReferences(...args),
    getConversationMessages: async () => ({ ok: false, error: 'not used' }),
  };
  return { VaultAPI: api, default: api };
});

function bookmark(
  overrides: Partial<ConversationMessageBookmarkDto> = {},
): ConversationMessageBookmarkDto {
  return {
    id: 'bm_chat',
    conversationId: 'conv_1',
    conversationTitle: 'Sleep study',
    spaceId: 'space_general',
    messageId: 'msg_1',
    messageRole: 'assistant',
    messagePreview: 'A preview line.',
    title: null,
    note: null,
    createdAt: '2026-09-05T10:00:00.000Z',
    ...overrides,
  } as ConversationMessageBookmarkDto;
}

function passage(overrides: Partial<PassageReferenceDto> = {}): PassageReferenceDto {
  return {
    id: 'pref_1',
    documentId: 'doc_1',
    chunkId: 'chunk_1',
    filePath: '/vault/paper.pdf',
    fileName: 'paper.pdf',
    locator: 'p. 12',
    text: 'A saved passage.',
    title: null,
    note: null,
    createdAt: '2026-09-04T10:00:00.000Z',
    ...overrides,
  };
}

/** A note carrying the snapshot that marks `conv_1 / msg_1` as captured. */
const capturedNote = {
  id: 'note_1',
  title: 'Sep 5',
  content: '',
  linkedDocumentIds: [],
  linkedConversationIds: [],
  highlights: [],
  stickyNotes: [],
  conversationSnapshots: [
    {
      id: 'capture_1',
      conversationId: 'conv_1',
      conversationTitle: 'Sleep study',
      capturedAt: '2026-09-05T11:00:00.000Z',
      messageCount: 1,
      messages: [
        { id: 'msg_1', role: 'assistant', content: 'x', createdAt: '2026-09-05T10:00:00.000Z' },
      ],
    },
  ],
  createdAt: '2026-09-05T09:00:00.000Z',
  updatedAt: '2026-09-05T11:00:00.000Z',
};

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

async function mountInbox() {
  const view = renderHook(() => useReferenceInbox({ requestedReferenceId: null }), {
    wrapper,
  });
  await waitFor(() => expect(view.result.current.isLoading).toBe(false));
  await waitFor(() => expect(view.result.current.filteredItems.length).toBeGreaterThan(0));
  return view;
}

describe('useReferenceInbox filters', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listMessageBookmarks.mockResolvedValue({
      ok: true,
      data: {
        bookmarks: [
          bookmark({ id: 'bm_chat', createdAt: '2026-09-05T10:00:00.000Z' }),
          bookmark({
            id: 'bm_journal',
            spaceId: 'space_journal',
            createdAt: '2026-09-06T10:00:00.000Z',
            messageId: 'msg_2',
          }),
        ],
      },
    });
    listWorkspaceNotes.mockResolvedValue({ ok: true, data: { notes: [capturedNote] } });
    listConversationSpaces.mockResolvedValue({ ok: true, data: [] });
    listJournals.mockResolvedValue({
      ok: true,
      data: [{ id: 'space_journal', name: 'Journal 1', isArchived: false }],
    });
    listPassageReferences.mockResolvedValue({
      ok: true,
      data: [
        passage({ id: 'pref_1', createdAt: '2026-09-04T10:00:00.000Z' }),
        passage({ id: 'pref_2', createdAt: '2026-09-07T10:00:00.000Z' }),
      ],
    });
  });

  it('merges both kinds newest-first', async () => {
    const { result } = await mountInbox();
    expect(result.current.filteredItems.map((item) => item.id)).toEqual([
      'pref_2',
      'bm_journal',
      'bm_chat',
      'pref_1',
    ]);
  });

  it('origin "document" keeps only saved passages', async () => {
    const { result } = await mountInbox();
    act(() => result.current.setOriginFilter('document'));
    await waitFor(() =>
      expect(result.current.filteredItems.every((item) => item.kind === 'passage')).toBe(true),
    );
    expect(result.current.filteredItems.map((item) => item.id)).toEqual(['pref_2', 'pref_1']);
  });

  it('origin "journal" keeps only bookmarks whose space is a journal', async () => {
    const { result } = await mountInbox();
    act(() => result.current.setOriginFilter('journal'));
    await waitFor(() =>
      expect(result.current.filteredItems.map((item) => item.id)).toEqual(['bm_journal']),
    );
  });

  it('origin "chat" keeps only bookmarks whose space is not a journal', async () => {
    const { result } = await mountInbox();
    act(() => result.current.setOriginFilter('chat'));
    await waitFor(() =>
      expect(result.current.filteredItems.map((item) => item.id)).toEqual(['bm_chat']),
    );
  });

  it('"captured" keeps only captured message bookmarks — a passage is never captured', async () => {
    const { result } = await mountInbox();
    act(() => result.current.setStatusChip('captured'));
    await waitFor(() =>
      expect(result.current.filteredItems.map((item) => item.id)).toEqual(['bm_chat']),
    );
  });
});
