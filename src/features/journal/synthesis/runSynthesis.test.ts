import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';


import { VaultAPI } from '@/lib/api';
import type { SynthesisProgressDto } from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { runSynthesis } from './runSynthesis';
import { useSynthesisStore } from './synthesisStore';

vi.mock('@/lib/api', () => {
  const api = { synthesizeJournalEntries: vi.fn(), quickCapture: vi.fn(), listWorkspaceNotes: vi.fn(), updateWorkspaceNote: vi.fn(), createWorkspaceNote: vi.fn() };
  return { VaultAPI: api, default: api };
});

const response = { synthesis: 'Decisions from the discussion.', entryCount: 2, chunkCount: 3, scope: 'conversation', conversationIds: ['first', 'second'] };
const options = { title: 'Research', heading: 'Research', request: { conversationIds: ['first', 'second'], scope: 'conversation' }, destination: { kind: 'capture' as const } };
const page = (id: string): WorkspaceNote => ({ id, title: id, content: 'Existing notes.', revision: 1, createdAt: '', updatedAt: '', journalId: 'journal', linkedDocumentIds: [], linkedConversationIds: [], highlights: [], stickyNotes: [], sources: [], conversationSnapshots: [] });
const client = () => new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(complete => { resolve = complete; });
  return { promise, resolve };
}

describe('synthesis workflow', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useSynthesisStore.setState({ job: null, minimized: false });
    vi.mocked(VaultAPI.synthesizeJournalEntries).mockResolvedValue({ ok: true, data: response });
    vi.mocked(VaultAPI.quickCapture).mockResolvedValue({ ok: true, data: { noteId: 'saved', noteTitle: 'Today', created: false } });
  });

  it('reports real progress, rejects duplicate starts and waits for persistence before success', async () => {
    const generate = deferred<Awaited<ReturnType<typeof VaultAPI.synthesizeJournalEntries>>>();
    const save = deferred<Awaited<ReturnType<typeof VaultAPI.quickCapture>>>();
    let report!: (progress: SynthesisProgressDto) => void;
    vi.mocked(VaultAPI.synthesizeJournalEntries).mockImplementation((_request, progress) => { report = progress!; return generate.promise; });
    vi.mocked(VaultAPI.quickCapture).mockReturnValue(save.promise);
    const running = runSynthesis(options, client());
    expect(useSynthesisStore.getState().job).toMatchObject({ status: 'running', stage: 'gathering' });
    report({ stage: 'reading', entryCount: 2, chunkIndex: 2, chunkCount: 3 });
    expect(useSynthesisStore.getState().job).toMatchObject({ stage: 'reading', chunkIndex: 2, chunkCount: 3 });
    useSynthesisStore.getState().dismiss();
    expect(useSynthesisStore.getState().job?.status).toBe('running');
    useSynthesisStore.getState().setMinimized(true);
    expect(await runSynthesis(options, client())).toBe(false);
    expect(useSynthesisStore.getState().minimized).toBe(false);
    expect(VaultAPI.synthesizeJournalEntries).toHaveBeenCalledTimes(1);
    report({ stage: 'writing', entryCount: 2, chunkIndex: null, chunkCount: 3 });
    generate.resolve({ ok: true, data: response });
    await vi.waitFor(() => expect(VaultAPI.quickCapture).toHaveBeenCalledOnce());
    expect(useSynthesisStore.getState().job).toMatchObject({ status: 'running', stage: 'saving' });
    report({ stage: 'reading', entryCount: 2, chunkIndex: 1, chunkCount: 3 });
    expect(useSynthesisStore.getState().job?.stage).toBe('saving');
    save.resolve({ ok: true, data: { noteId: 'saved', noteTitle: 'Today', created: false } });
    expect(await running).toBe(true);
    expect(useSynthesisStore.getState().job).toMatchObject({ status: 'completed', noteId: 'saved' });
    report({ stage: 'writing', entryCount: 2, chunkIndex: null, chunkCount: 3 });
    expect(useSynthesisStore.getState().job?.status).toBe('completed');
  });

  it('retains a generated synthesis on save failure and retries saving without regenerating', async () => {
    vi.mocked(VaultAPI.quickCapture).mockResolvedValueOnce({ ok: false, error: 'Disk full' });
    expect(await runSynthesis(options, client())).toBe(false);
    expect(useSynthesisStore.getState().job).toMatchObject({ status: 'failed', stage: 'saving', error: 'Disk full' });
    expect(await useSynthesisStore.getState().job?.retry?.()).toBe(true);
    expect(VaultAPI.synthesizeJournalEntries).toHaveBeenCalledOnce();
    expect(VaultAPI.quickCapture).toHaveBeenCalledTimes(2);
    expect(vi.mocked(VaultAPI.quickCapture).mock.calls[1]).toEqual(vi.mocked(VaultAPI.quickCapture).mock.calls[0]);
  });

  it('can retry a model failure and ignores callbacks from the failed attempt', async () => {
    let late!: (progress: SynthesisProgressDto) => void;
    vi.mocked(VaultAPI.synthesizeJournalEntries).mockImplementationOnce(async (_request, progress) => {
      late = progress!;
      return { ok: false, error: 'Model unavailable' };
    });
    expect(await runSynthesis(options, client())).toBe(false);
    expect(VaultAPI.quickCapture).not.toHaveBeenCalled();
    const retry = useSynthesisStore.getState().job?.retry?.();
    late({ stage: 'reading', entryCount: 99, chunkIndex: 99, chunkCount: 99 });
    expect(await retry).toBe(true);
    expect(useSynthesisStore.getState().job).toMatchObject({ status: 'completed', entryCount: 2 });
    expect(VaultAPI.synthesizeJournalEntries).toHaveBeenCalledTimes(2);
  });

  it('flushes edits and appends to the original destination using its fresh revision', async () => {
    const target = page('original');
    const other = page('now-open');
    const unregister = registerPendingSave(async () => { target.content = 'Edited while synthesis ran.'; target.revision = 2; return true; });
    vi.mocked(VaultAPI.listWorkspaceNotes).mockImplementation(async () => ({ ok: true, data: { notes: [other, target] } }));
    vi.mocked(VaultAPI.updateWorkspaceNote).mockImplementation(async note => ({ ok: true, data: { ...note, revision: note.revision + 1 } }));
    try {
      expect(await runSynthesis({ ...options, destination: { kind: 'note', noteId: target.id } }, client())).toBe(true);
      expect(VaultAPI.updateWorkspaceNote).toHaveBeenCalledWith(expect.objectContaining({
        id: 'original', revision: 2, content: expect.stringContaining('Edited while synthesis ran.'), linkedConversationIds: response.conversationIds,
      }));
      expect(useSynthesisStore.getState().job?.noteId).toBe('original');
    } finally { unregister(); }
  });

  it('retains the result if an open editor cannot save, then recovers without regeneration', async () => {
    const unregister = registerPendingSave(async () => false);
    expect(await runSynthesis(options, client())).toBe(false);
    expect(VaultAPI.quickCapture).not.toHaveBeenCalled();
    unregister();
    expect(await useSynthesisStore.getState().job?.retry?.()).toBe(true);
    expect(VaultAPI.synthesizeJournalEntries).toHaveBeenCalledOnce();
  });
});
