import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useAttachmentImport } from '@/features/chat/hooks/useAttachmentImport';
import { useStagedPicturesStore } from '@/features/chat/images/stagedPictures';
import { deferred } from '@/tests/deferred';

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
  add: vi.fn(),
  clear: vi.fn(),
  onDrop: (_paths: string[]) => {},
}));
const loadConversationLinkedDocuments = vi.hoisted(() => vi.fn());
const toast = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn(), info: vi.fn() }));

vi.mock('@/lib/api', () => ({ VaultAPI: api, default: api }));
vi.mock('@/features/chat/model/attachmentImportPoll', () => ({ pollAttachmentImport: poll }));
vi.mock('@/features/chat/hooks/useChatFileDrop', () => ({
  useChatFileDrop: (_ref: unknown, onDrop: (paths: string[]) => void) => {
    drop.onDrop = onDrop;
    return { isDragging: false, staged: drop.staged, add: drop.add, clear: drop.clear, remove: vi.fn() };
  },
}));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: unknown) => unknown) => select({ loadConversationLinkedDocuments }),
}));
vi.mock('@/stores/toastStore', () => ({ toast }));

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>;
}

function render(conversationId: string | null = 'conversation-1') {
  return renderHook(
    () => useAttachmentImport({ dropTargetRef: { current: null }, conversationId, scopedSpaceId: null }),
    { wrapper },
  );
}

const picture = {
  id: 'c'.repeat(32), ref: 'att-1', fileName: `${'c'.repeat(32)}.png`, mime: 'image/png', width: 640, height: 480,
  byteSize: 2048, caption: 'loaf.png', sourceUrl: null, sourceTitle: null, origin: 'attachment' as const,
  license: null, attribution: null, originalName: 'loaf.png',
};

async function runImport(result: { current: ReturnType<typeof useAttachmentImport> }) {
  let imported: unknown = 'unset';
  await act(async () => { imported = await result.current.importStaged(); });
  return imported;
}

beforeEach(() => {
  vi.clearAllMocks();
  drop.staged = [{ path: '/kitchen/sourdough.md', name: 'sourdough.md' }, { path: '/kitchen/rye.pdf', name: 'rye.pdf' }];
  api.startBatchFileImport.mockResolvedValue({ ok: true, data: 'job-1' });
  api.attachMessageImages.mockResolvedValue({ ok: true, data: [picture] });
  api.releaseMessageImages.mockResolvedValue({ ok: true, data: null });
  useStagedPicturesStore.setState({ byConversation: {} });
});

describe('pictures on the draft', () => {
  it('reports pictures as attaching until the store answers', async () => {
    const stored = deferred<unknown>();
    api.attachMessageImages.mockReturnValueOnce(stored.promise);
    const { result } = render();

    await act(async () => { drop.onDrop(['/photos/loaf.png']); });
    expect(result.current.isAttachingPictures).toBe(true);

    await act(async () => { stored.resolve({ ok: true, data: [picture] }); });
    expect(result.current.isAttachingPictures).toBe(false);
    expect(result.current.pictures).toEqual([picture]);
  });

  it('stores nothing for a drop without pictures', async () => {
    render();
    await act(async () => { drop.onDrop(['/kitchen/rye.pdf']); });
    expect(api.attachMessageImages).not.toHaveBeenCalled();
  });

  it('does nothing with pictures or files before a conversation exists', async () => {
    const { result } = render(null);

    await act(async () => { drop.onDrop(['/photos/loaf.png']); });
    await act(async () => {
      result.current.removePicture(picture.id);
      result.current.clear();
    });

    expect(api.attachMessageImages).not.toHaveBeenCalled();
    expect(api.releaseMessageImages).not.toHaveBeenCalled();
    expect(drop.clear).toHaveBeenCalled();
    expect(result.current.takePictures()).toEqual([]);
    expect(await runImport(result)).toBeNull();
    expect(api.startBatchFileImport).not.toHaveBeenCalled();
  });
});

describe('choosing files', () => {
  it('stages every chosen file and stores the pictures among them', async () => {
    vi.mocked(open).mockResolvedValueOnce(['/kitchen/sourdough.md', '/photos/loaf.png'] as never);
    const { result } = render();

    await act(async () => { await result.current.chooseFiles(); });

    expect(open).toHaveBeenCalledWith({ multiple: true });
    expect(drop.add).toHaveBeenCalledWith(['/kitchen/sourdough.md', '/photos/loaf.png']);
    expect(api.attachMessageImages).toHaveBeenCalledWith('conversation-1', ['/photos/loaf.png']);
  });

  it('accepts a single chosen path', async () => {
    vi.mocked(open).mockResolvedValueOnce('/kitchen/rye.pdf' as never);
    const { result } = render();

    await act(async () => { await result.current.chooseFiles(); });

    expect(drop.add).toHaveBeenCalledWith(['/kitchen/rye.pdf']);
  });

  it('stages nothing when the picker is dismissed or unavailable', async () => {
    vi.mocked(open).mockResolvedValueOnce(null).mockRejectedValueOnce(new Error('not in a Tauri webview'));
    const { result } = render();

    await act(async () => { await result.current.chooseFiles(); });
    await act(async () => { await result.current.chooseFiles(); });

    expect(drop.add).not.toHaveBeenCalled();
    expect(api.attachMessageImages).not.toHaveBeenCalled();
  });
});

describe('importing the staged files', () => {
  it('returns nothing when no files are staged', async () => {
    drop.staged = [];
    const { result } = render();
    expect(await runImport(result)).toBeNull();
    expect(api.startBatchFileImport).not.toHaveBeenCalled();
  });

  it('refuses a second import while one is running', async () => {
    const started = deferred<unknown>();
    api.startBatchFileImport.mockReturnValueOnce(started.promise);
    poll.mockResolvedValue({ kind: 'finished', documentIds: ['d1', 'd2'], added: 2, failed: 0 });
    const { result } = render();

    let first!: Promise<unknown>;
    act(() => { first = result.current.importStaged(); });
    expect(result.current.isImporting).toBe(true);
    expect(await runImport(result)).toBeNull();
    expect(api.startBatchFileImport).toHaveBeenCalledTimes(1);

    await act(async () => {
      started.resolve({ ok: true, data: 'job-1' });
      await first;
    });
    expect(result.current.isImporting).toBe(false);
  });

  it('keeps the files staged when the job status cannot be read', async () => {
    poll.mockResolvedValue({ kind: 'unreachable', error: 'status unavailable' });
    const { result } = render();

    expect(await runImport(result)).toBeNull();
    expect(toast.error).toHaveBeenCalledWith("Couldn't confirm these files were attached", { message: 'status unavailable' });
    expect(drop.clear).not.toHaveBeenCalled();
    expect(loadConversationLinkedDocuments).not.toHaveBeenCalled();
  });

  it('sends the ids it has when indexing is still running at the deadline', async () => {
    poll.mockResolvedValue({ kind: 'pending', documentIds: ['d1'] });
    const { result } = render();

    expect(await runImport(result)).toEqual({ names: ['sourdough.md', 'rye.pdf'], documentIds: ['d1'] });
    expect(drop.clear).toHaveBeenCalled();
    expect(toast.info).toHaveBeenCalledWith('Still attaching 2 files', {
      message: "They'll appear in this conversation when indexing finishes.",
    });
    expect(loadConversationLinkedDocuments).toHaveBeenCalledWith('conversation-1');
  });

  it('names one still-attaching file in the singular', async () => {
    drop.staged = [{ path: '/garden/compost.md', name: 'compost.md' }];
    poll.mockResolvedValue({ kind: 'pending', documentIds: ['d1'] });
    const { result } = render();

    await runImport(result);
    expect(toast.info).toHaveBeenCalledWith('Still attaching 1 file', expect.any(Object));
  });

  it('keeps the files staged when every file failed', async () => {
    poll.mockResolvedValue({ kind: 'finished', documentIds: [], added: 0, failed: 2 });
    const { result } = render();

    expect(await runImport(result)).toBeNull();
    expect(toast.error).toHaveBeenCalledWith("Couldn't attach these files", { message: '2 failed to import.' });
    expect(drop.clear).not.toHaveBeenCalled();
  });

  it('counts every requested file as failed when the job reports no failures but added none', async () => {
    drop.staged = [{ path: '/garden/compost.md', name: 'compost.md' }];
    poll.mockResolvedValue({ kind: 'finished', documentIds: [], added: 0, failed: 0 });
    const { result } = render();

    await runImport(result);
    expect(toast.error).toHaveBeenCalledWith("Couldn't attach these files", { message: '1 failed to import.' });
  });

  it('reports the files it could not read alongside the ones it attached', async () => {
    poll.mockResolvedValue({ kind: 'finished', documentIds: ['d1'], added: 1, failed: 1 });
    const { result } = render();

    expect(await runImport(result)).toEqual({ names: ['sourdough.md', 'rye.pdf'], documentIds: ['d1'] });
    expect(toast.success).toHaveBeenCalledWith('Attached 1 file', {
      message: "1 couldn't be read. The rest are indexing now.",
    });
    expect(drop.clear).toHaveBeenCalled();
  });
});
