import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useAttachmentImport } from '@/features/chat/hooks/useAttachmentImport';

const api = vi.hoisted(() => ({
  startBatchFileImport: vi.fn(),
  getBatchJobStatus: vi.fn(),
  setDocumentsSpaceMembership: vi.fn(),
}));
const poll = vi.hoisted(() => vi.fn());
const drop = vi.hoisted(() => ({ staged: [] as { path: string; name: string }[], clear: vi.fn() }));
const loadConversationLinkedDocuments = vi.hoisted(() => vi.fn());
const toast = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn(), info: vi.fn() }));

vi.mock('@/lib/api', () => ({ VaultAPI: api }));
vi.mock('@/features/chat/model/attachmentImportPoll', () => ({ pollAttachmentImport: poll }));
vi.mock('@/features/chat/hooks/useChatFileDrop', () => ({
  useChatFileDrop: () => ({ isDragging: false, staged: drop.staged, add: vi.fn(), clear: drop.clear, remove: vi.fn() }),
}));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: unknown) => unknown) => select({ loadConversationLinkedDocuments }),
}));
vi.mock('@/stores/toastStore', () => ({ toast }));

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>;
}

function render(scopedSpaceId: string | null = null) {
  return renderHook(
    () => useAttachmentImport({ dropTargetRef: { current: null }, conversationId: 'conversation-1', scopedSpaceId }),
    { wrapper },
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  drop.staged = [{ path: '/tmp/a.pdf', name: 'a.pdf' }, { path: '/tmp/b.md', name: 'b.md' }];
  api.startBatchFileImport.mockResolvedValue({ ok: true, data: 'job-1' });
});

describe('attaching staged files to a conversation', () => {
  it('hands the send the names and the ids, and clears what it attached', async () => {
    poll.mockResolvedValue({ kind: 'finished', documentIds: ['doc-a', 'doc-b'], added: 2, failed: 0 });
    const { result } = render();

    let imported: Awaited<ReturnType<typeof result.current.importStaged>> = null;
    await act(async () => { imported = await result.current.importStaged(); });

    expect(api.startBatchFileImport).toHaveBeenCalledWith(['/tmp/a.pdf', '/tmp/b.md'], 'conversation-1');
    expect(imported).toEqual({ names: ['a.pdf', 'b.md'], documentIds: ['doc-a', 'doc-b'] });
    expect(drop.clear).toHaveBeenCalled();
    expect(api.setDocumentsSpaceMembership).not.toHaveBeenCalled();
    expect(loadConversationLinkedDocuments).toHaveBeenCalledWith('conversation-1');
  });

  it('files the attachments into a conversation\'s own space', async () => {
    poll.mockResolvedValue({ kind: 'finished', documentIds: ['doc-a'], added: 1, failed: 0 });
    const { result } = render('space-thesis');

    await act(async () => { await result.current.importStaged(); });

    expect(api.setDocumentsSpaceMembership).toHaveBeenCalledWith(['doc-a'], 'space-thesis', true);
  });

  it('keeps the files staged and aborts the send when the import cannot start', async () => {
    api.startBatchFileImport.mockResolvedValue({ ok: false, error: 'disk full' });
    const { result } = render();

    let imported: unknown = 'unset';
    await act(async () => { imported = await result.current.importStaged(); });

    expect(imported).toBeNull();
    expect(drop.clear).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith("Couldn't add these files", { message: 'disk full' });
  });

  it('aborts the send when a slow import has no ids for the turn to read', async () => {
    poll.mockResolvedValue({ kind: 'pending', documentIds: [] });
    const { result } = render();

    let imported: unknown = 'unset';
    await act(async () => { imported = await result.current.importStaged(); });

    expect(imported).toBeNull();
    expect(drop.clear).not.toHaveBeenCalled();
  });
});
