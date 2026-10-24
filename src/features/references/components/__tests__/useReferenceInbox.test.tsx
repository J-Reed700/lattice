import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor, act } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useReferenceInbox } from '@/features/references/hooks/useReferenceInbox';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { PassageReferenceDto } from '@/types/api/references';



const listMessageBookmarks = vi.fn();
const listWorkspaceNotes = vi.fn();
const listConversationSpaces = vi.fn();
const listJournals = vi.fn();
const listPassageReferences = vi.fn();
const getConversationMessages = vi.fn();

vi.mock('@/lib/api', () => {
  const api = {
    listMessageBookmarks: (...args: unknown[]) => listMessageBookmarks(...args),
    listWorkspaceNotes: () => listWorkspaceNotes(),
    listConversationSpaces: () => listConversationSpaces(),
    listJournals: () => listJournals(),
    listPassageReferences: (...args: unknown[]) => listPassageReferences(...args),
    getConversationMessages: (...args: unknown[]) => getConversationMessages(...args),
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
    getConversationMessages.mockResolvedValue({ ok: false, error: 'not used' });
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

describe('useReferenceInbox selected reference content', () => {
  const message = { id: 'msg_1', role: 'assistant', content: 'The full answer.', createdAt: '2026-09-05T10:00:00.000Z' };

  beforeEach(() => {
    vi.clearAllMocks();
    listMessageBookmarks.mockResolvedValue({ ok: true, data: { bookmarks: [bookmark()] } });
    listWorkspaceNotes.mockResolvedValue({ ok: true, data: { notes: [] } });
    listConversationSpaces.mockResolvedValue({ ok: true, data: [] });
    listJournals.mockResolvedValue({ ok: true, data: [] });
    listPassageReferences.mockResolvedValue({ ok: true, data: [] });
    getConversationMessages.mockResolvedValue({ ok: true, data: [message] });
  });

  it('reads the selected message once and reuses it for capture', async () => {
    const { result } = await mountInbox();
    act(() => result.current.setSelectedId('bm_chat'));
    await waitFor(() => expect(result.current.selectedPayload?.content).toBe('The full answer.'));
    expect(result.current.isResolvingSelected).toBe(false);
    expect(result.current.resolutionFailed).toBe(false);

    const payload = await result.current.getPayload(bookmark());
    expect(payload.content).toBe('The full answer.');
    expect(getConversationMessages).toHaveBeenCalledTimes(1);
    expect(getConversationMessages).toHaveBeenCalledWith('conv_1');
  });

  it('says when the message cannot be read, and reads it again when reopened', async () => {
    getConversationMessages.mockResolvedValueOnce({ ok: false, error: 'gone' });
    const { result } = await mountInbox();
    act(() => result.current.setSelectedId('bm_chat'));
    await waitFor(() => expect(result.current.resolutionFailed).toBe(true));
    expect(result.current.selectedPayload).toBeNull();
    expect(result.current.isResolvingSelected).toBe(false);

    act(() => result.current.setSelectedId(null));
    act(() => result.current.setSelectedId('bm_chat'));
    await waitFor(() => expect(result.current.selectedPayload?.content).toBe('The full answer.'));
    expect(result.current.resolutionFailed).toBe(false);
  });
});

/** A reply the test sends when it chooses, so the two lists can arrive in either order. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

const bookmarksReply = { ok: true, data: { bookmarks: [bookmark({ id: 'bm_chat' })] } };
const passagesReply = {
  ok: true,
  data: [passage({ id: 'pref_1', createdAt: '2026-09-07T10:00:00.000Z' })],
};

function mountRequested(requestedReferenceId: string) {
  return renderHook(() => useReferenceInbox({ requestedReferenceId }), { wrapper });
}

/** Lets effects queued by the last render run before asserting what they did. */
const settle = () => act(async () => {});

describe('useReferenceInbox requested reference', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listWorkspaceNotes.mockResolvedValue({ ok: true, data: { notes: [] } });
    listConversationSpaces.mockResolvedValue({ ok: true, data: [] });
    listJournals.mockResolvedValue({ ok: true, data: [] });
    getConversationMessages.mockResolvedValue({ ok: false, error: 'not used' });
  });

  it('keeps a linked bookmark selected while saved passages arrive first', async () => {
    const bookmarks = deferred<typeof bookmarksReply>();
    listMessageBookmarks.mockReturnValue(bookmarks.promise);
    listPassageReferences.mockResolvedValue(passagesReply);
    const { result } = mountRequested('bm_chat');

    await waitFor(() => expect(result.current.filteredItems.map((item) => item.id)).toEqual(['pref_1']));
    await settle();
    expect(result.current.selectedId).toBeNull();
    expect(result.current.isRequestPending).toBe(true);

    await act(async () => bookmarks.resolve(bookmarksReply));
    await waitFor(() => expect(result.current.selectedId).toBe('bm_chat'));
    expect(result.current.isRequestPending).toBe(false);
  });

  it('keeps a linked passage selected while bookmarks arrive first', async () => {
    const passages = deferred<typeof passagesReply>();
    listMessageBookmarks.mockResolvedValue(bookmarksReply);
    listPassageReferences.mockReturnValue(passages.promise);
    const { result } = mountRequested('pref_1');

    await waitFor(() => expect(result.current.filteredItems.map((item) => item.id)).toEqual(['bm_chat']));
    await settle();
    expect(result.current.selectedId).toBeNull();
    expect(result.current.isRequestPending).toBe(true);

    await act(async () => passages.resolve(passagesReply));
    await waitFor(() => expect(result.current.selectedId).toBe('pref_1'));
    expect(result.current.isRequestPending).toBe(false);
  });

  it('falls back to the newest reference only once both lists have loaded without the link', async () => {
    listMessageBookmarks.mockResolvedValue(bookmarksReply);
    listPassageReferences.mockResolvedValue(passagesReply);
    const { result } = mountRequested('bm_deleted');

    await waitFor(() => expect(result.current.selectedId).toBe('pref_1'));
    expect(result.current.isRequestPending).toBe(false);
  });

  it('reports saved passages that failed beside loaded bookmarks, keeps a passage link waiting, and retries', async () => {
    listMessageBookmarks.mockResolvedValue(bookmarksReply);
    listPassageReferences.mockResolvedValueOnce({ ok: false, error: 'The reference store is locked.' });
    const { result } = mountRequested('pref_1');

    await waitFor(() => expect(result.current.listLoadFailed).toBe(true));
    await settle();
    expect(result.current.filteredItems.map((item) => item.id)).toEqual(['bm_chat']);
    expect(result.current.selectedId).toBeNull();
    expect(result.current.isRequestPending).toBe(true);

    listPassageReferences.mockResolvedValue(passagesReply);
    act(() => result.current.retryLoad());
    await waitFor(() => expect(result.current.selectedId).toBe('pref_1'));
    expect(result.current.listLoadFailed).toBe(false);
    expect(listMessageBookmarks).toHaveBeenCalledTimes(1);
  });

  it('reports bookmarks that failed beside loaded passages, and retries only them', async () => {
    listMessageBookmarks.mockResolvedValueOnce({ ok: false, error: 'Database busy.' });
    listPassageReferences.mockResolvedValue(passagesReply);
    const { result } = renderHook(() => useReferenceInbox({ requestedReferenceId: null }), { wrapper });

    await waitFor(() => expect(result.current.listLoadFailed).toBe(true));
    expect(result.current.filteredItems.map((item) => item.id)).toEqual(['pref_1']);

    listMessageBookmarks.mockResolvedValue(bookmarksReply);
    act(() => result.current.retryLoad());
    await waitFor(() => expect(result.current.listLoadFailed).toBe(false));
    expect(result.current.filteredItems.map((item) => item.id)).toEqual(['pref_1', 'bm_chat']);
    expect(listPassageReferences).toHaveBeenCalledTimes(1);
  });
});
