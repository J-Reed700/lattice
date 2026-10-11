import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import type { JournalSynthesisDto } from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';
import { apiCall } from '@/shared/ipc/transport';
import { jobFixture } from '@/tests/fixtures/jobs';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { synthesisKeys } from './api';
import { applySynthesis, runSynthesis } from './runSynthesis';
import { useSynthesisPanel } from './synthesisPanel';

vi.mock('@/lib/api', () => {
  const api = { quickCapture: vi.fn(), listWorkspaceNotes: vi.fn(), updateWorkspaceNote: vi.fn(), createWorkspaceNote: vi.fn() };
  return { VaultAPI: api, default: api };
});
vi.mock('@/shared/ipc/transport', () => ({ apiCall: vi.fn() }));

const result = { synthesis: 'Decisions from the discussion.', entryCount: 2, chunkCount: 3, scope: 'conversation', conversationIds: ['first', 'second'], citations: [], sources: [] };
const options = { title: 'Research', heading: 'Research', request: { conversationIds: ['first', 'second'], scope: 'conversation', maxEntries: 2 }, destination: { kind: 'capture' as const } };
const page = (id: string): WorkspaceNote => ({ id, title: id, content: 'Existing notes.', revision: 1, createdAt: '', updatedAt: '', journalId: 'journal', linkedDocumentIds: [], linkedConversationIds: [], highlights: [], stickyNotes: [], sources: [], conversationSnapshots: [] });
const client = () => new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });

function synthesis(overrides: Partial<JournalSynthesisDto> = {}, job: Parameters<typeof jobFixture>[0] = {}): JournalSynthesisDto {
  return {
    job: jobFixture({ id: 'synthesis-1', kind: 'journal.synthesis', subjectId: 'capture', status: 'completed', resultRef: 'capture', finishedAt: 5, ...job }),
    title: 'Research', heading: 'Research', destination: { kind: 'capture' }, conversationIds: ['first', 'second'], activity: null,
    ...overrides,
  };
}

/** The backend's synthesis commands, with what each was asked. */
function backend(list: JournalSynthesisDto[] = []) {
  vi.mocked(apiCall).mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case 'list_journal_syntheses': return { ok: true, data: list };
      case 'synthesize_journal_entries': return { ok: true, data: synthesis({}, { id: 'started', status: 'pending', finishedAt: null, resultRef: null }) };
      case 'get_journal_synthesis_result': return { ok: true, data: result };
      case 'mark_journal_synthesis_applied': return { ok: true, data: true };
      default: throw new Error(`unexpected ${command} ${JSON.stringify(args)}`);
    }
  });
}

const calls = (command: string) => vi.mocked(apiCall).mock.calls.filter(([name]) => name === command);

describe('synthesis workflow', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useSynthesisPanel.setState({ minimized: false, autoSave: [], saving: {}, saved: null });
    vi.mocked(VaultAPI.quickCapture).mockResolvedValue({ ok: true, data: { noteId: 'saved', noteTitle: 'Today', created: false } });
  });

  it('starts a job with its destination fixed and refuses a second while one runs', async () => {
    backend();
    const queries = client();

    expect(await runSynthesis(options, queries)).toBe(true);

    expect(calls('synthesize_journal_entries')[0][1]).toEqual({ request: { request: options.request, destination: { kind: 'capture' }, title: 'Research', heading: 'Research' } });
    expect(useSynthesisPanel.getState().autoSave).toEqual(['started']);
    expect(queries.getQueryData<JournalSynthesisDto[]>(synthesisKeys.list)?.map(item => item.job.id)).toEqual(['started']);
    useSynthesisPanel.setState({ minimized: true });
    expect(await runSynthesis(options, queries)).toBe(false);
    expect(useSynthesisPanel.getState().minimized).toBe(false);
    expect(calls('synthesize_journal_entries')).toHaveLength(1);
  });

  it('saves a finished synthesis once, then marks it applied', async () => {
    backend([synthesis()]);
    const queries = client();
    queries.setQueryData(synthesisKeys.list, [synthesis()]);

    const [first, second] = await Promise.all([applySynthesis(synthesis(), queries), applySynthesis(synthesis(), queries)]);

    expect([first, second]).toEqual([true, false]);
    expect(VaultAPI.quickCapture).toHaveBeenCalledOnce();
    expect(vi.mocked(VaultAPI.quickCapture).mock.calls[0][0]).toContain('Decisions from the discussion.');
    expect(calls('mark_journal_synthesis_applied')).toEqual([['mark_journal_synthesis_applied', { jobId: 'synthesis-1' }]]);
    expect(queries.getQueryData(synthesisKeys.list)).toEqual([]);
    expect(useSynthesisPanel.getState().saved).toMatchObject({ jobId: 'synthesis-1', noteId: 'saved', noteTitle: 'Today' });
  });

  it('saves every source and conversation link with the generated text', async () => {
    const sources = Array.from({ length: 25 }, (_, index) => ({
      documentId: `document-${index}`, chunkId: `chunk-${index}`, content: `Evidence ${index}`,
      score: 1, path: null, position: null, fileName: `Source ${index}`, filePath: `/sources/${index}.pdf`,
      mimeType: 'application/pdf', category: 'Document', fileSizeBytes: 100, modifiedAt: '',
      citationId: index + 1, pageNumber: index + 1, excerpt: null, highlights: null, section: null,
      chunkIndex: null, chunkExcerpts: null, webSnapshot: null,
    }));
    backend([synthesis()]);
    vi.mocked(apiCall).mockImplementation(async (command: string) => command === 'get_journal_synthesis_result'
      ? { ok: true, data: { ...result, synthesis: 'First claim [1]. Last claim [25].', conversationIds: ['conversation-1'], sources } }
      : { ok: true, data: true });

    expect(await applySynthesis(synthesis(), client())).toBe(true);

    expect(VaultAPI.quickCapture).toHaveBeenCalledWith(
      expect.stringContaining('First claim [1]. Last claim [25].'), sources, ['conversation-1'],
    );
  });

  it('keeps a synthesis whose save failed, unapplied, to save again', async () => {
    backend([synthesis()]);
    vi.mocked(VaultAPI.quickCapture).mockResolvedValueOnce({ ok: false, error: 'Disk full' });
    const queries = client();
    queries.setQueryData(synthesisKeys.list, [synthesis()]);

    expect(await applySynthesis(synthesis(), queries)).toBe(false);

    expect(useSynthesisPanel.getState().saving).toEqual({ 'synthesis-1': { error: 'Disk full' } });
    expect(calls('mark_journal_synthesis_applied')).toHaveLength(0);
    expect(queries.getQueryData<JournalSynthesisDto[]>(synthesisKeys.list)).toHaveLength(1);
    expect(await applySynthesis(synthesis(), queries)).toBe(true);
    expect(calls('get_journal_synthesis_result')).toHaveLength(2);
  });

  it('flushes edits and appends to the original destination using its fresh revision', async () => {
    const target = page('original');
    const other = page('now-open');
    backend();
    const unregister = registerPendingSave(async () => { target.content = 'Edited while synthesis ran.'; target.revision = 2; return true; });
    vi.mocked(VaultAPI.listWorkspaceNotes).mockImplementation(async () => ({ ok: true, data: { notes: [other, target] } }));
    vi.mocked(VaultAPI.updateWorkspaceNote).mockImplementation(async note => ({ ok: true, data: { ...note, revision: note.revision + 1 } }));
    try {
      expect(await applySynthesis(synthesis({ destination: { kind: 'note', noteId: target.id } }), client())).toBe(true);
      expect(VaultAPI.updateWorkspaceNote).toHaveBeenCalledWith(expect.objectContaining({
        id: 'original', revision: 2, content: expect.stringContaining('Edited while synthesis ran.'), linkedConversationIds: result.conversationIds,
      }));
      expect(useSynthesisPanel.getState().saved?.noteId).toBe('original');
    } finally { unregister(); }
  });

  it('does not save over an open editor that cannot save, and keeps the result', async () => {
    backend([synthesis()]);
    const unregister = registerPendingSave(async () => false);
    expect(await applySynthesis(synthesis(), client())).toBe(false);
    expect(VaultAPI.quickCapture).not.toHaveBeenCalled();
    expect(calls('mark_journal_synthesis_applied')).toHaveLength(0);
    unregister();
    expect(await applySynthesis(synthesis(), client())).toBe(true);
  });
});
