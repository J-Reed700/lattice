import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useAttachmentImport } from '@/features/chat/hooks/useAttachmentImport';
import { useStagedPicturesStore } from '@/features/chat/images/stagedPictures';

const api = vi.hoisted(() => ({
  startBatchFileImport: vi.fn(),
  getBatchJobStatus: vi.fn(),
  setDocumentsSpaceMembership: vi.fn(),
  attachMessageImages: vi.fn(),
  releaseMessageImages: vi.fn(),
}));
const poll = vi.hoisted(() => vi.fn());
const drop = vi.hoisted(() => ({
  staged: [] as { path: string; name: string }[],
  clear: vi.fn(),
  onDrop: (_paths: string[]) => {},
  stageable: (_path: string): boolean => true,
}));
const loadConversationLinkedDocuments = vi.hoisted(() => vi.fn());
const toast = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn(), info: vi.fn() }));

vi.mock('@/lib/api', () => ({ VaultAPI: api, default: api }));
vi.mock('@/features/chat/model/attachmentImportPoll', () => ({ pollAttachmentImport: poll }));
vi.mock('@/features/chat/hooks/useChatFileDrop', () => ({
  useChatFileDrop: (_ref: unknown, onDrop: (paths: string[]) => void, stageable: (path: string) => boolean) => {
    drop.onDrop = onDrop;
    drop.stageable = stageable;
    return { isDragging: false, staged: drop.staged, add: vi.fn(), clear: drop.clear, remove: vi.fn() };
  },
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
  api.releaseMessageImages.mockResolvedValue({ ok: true, data: null });
  useStagedPicturesStore.setState({ byConversation: {} });
});

const picture = (n: number) => ({
  id: String(n).repeat(32), ref: `att-${n}`, fileName: `${String(n).repeat(32)}.jpg`, mime: 'image/jpeg',
  width: 800, height: 600, byteSize: 1000, caption: `p${n}.jpg`, sourceUrl: null, sourceTitle: null,
  origin: 'attachment' as const, license: null, attribution: null, originalName: `p${n}.jpg`,
});

describe('staging pictures on the draft', () => {
  it('stores dropped pictures at once and leaves documents to the send', async () => {
    api.attachMessageImages.mockResolvedValue({ ok: true, data: [picture(1)] });
    const { result } = render();

    expect(drop.stageable('/tmp/a.pdf')).toBe(true);
    expect(drop.stageable('/tmp/photo.JPG')).toBe(false);
    await act(async () => { drop.onDrop(['/tmp/a.pdf', '/tmp/photo.JPG']); });

    expect(api.attachMessageImages).toHaveBeenCalledWith('conversation-1', ['/tmp/photo.JPG']);
    expect(result.current.pictures).toEqual([picture(1)]);
  });

  it('releases a removed picture, and every picture when the draft is cancelled', async () => {
    useStagedPicturesStore.getState().add('conversation-1', [picture(1), picture(2), picture(3)]);
    const { result } = render();

    await act(async () => { result.current.removePicture(picture(1).id); });
    expect(api.releaseMessageImages).toHaveBeenLastCalledWith('conversation-1', [picture(1).id]);

    await act(async () => { result.current.clear(); });
    expect(api.releaseMessageImages).toHaveBeenLastCalledWith('conversation-1', [picture(2).id, picture(3).id]);
    expect(drop.clear).toHaveBeenCalled();
    expect(result.current.pictures).toEqual([]);
  });

  it('hands the pictures to the send without releasing them', async () => {
    useStagedPicturesStore.getState().add('conversation-1', [picture(1)]);
    const { result } = render();

    let taken: ReturnType<typeof result.current.takePictures> = [];
    act(() => { taken = result.current.takePictures(); });

    expect(taken).toEqual([picture(1)]);
    expect(result.current.pictures).toEqual([]);
    expect(api.releaseMessageImages).not.toHaveBeenCalled();
  });
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
