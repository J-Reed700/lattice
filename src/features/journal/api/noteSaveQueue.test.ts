import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { createNoteSaveQueue } from './noteSaveQueue';

vi.mock('@/lib/api', () => ({ VaultAPI: { updateWorkspaceNote: vi.fn() } }));
const note: WorkspaceNote = {
  id: 'note',
  revision: 0,
  title: 'Journal',
  journalId: null,
  content: 'Initial',
  linkedDocumentIds: [],
  linkedConversationIds: [],
  highlights: [],
  stickyNotes: [],
  conversationSnapshots: [],
  sources: [],
  createdAt: '',
  updatedAt: '',
};
describe('editor save revisions', () => {
  beforeEach(() => vi.clearAllMocks());
  it('advances queued drafts only through this editor’s successful writes', async () => {
    let revision = 0;
    vi.mocked(VaultAPI.updateWorkspaceNote).mockImplementation(
      async (draft) => {
        expect(draft.revision).toBe(revision);
        return { ok: true, data: { ...draft, revision: ++revision } };
      },
    );
    const save = createNoteSaveQueue();
    const writes = await Promise.all([
      save({ ...note, content: 'First' }),
      save({ ...note, content: 'Second' }),
    ]);
    expect(writes.map((saved) => saved.revision)).toEqual([1, 2]);
    expect(writes[1].content).toBe('Second');
  });
  it('retains the stale revision after a conflict instead of overwriting another writer', async () => {
    vi.mocked(VaultAPI.updateWorkspaceNote).mockResolvedValue({
      ok: false,
      error: 'Page changed elsewhere',
    });
    const save = createNoteSaveQueue();
    await expect(save(note)).rejects.toThrow('Page changed elsewhere');
    await expect(save({ ...note, content: 'Draft remains' })).rejects.toThrow(
      'Page changed elsewhere',
    );
    expect(
      vi
        .mocked(VaultAPI.updateWorkspaceNote)
        .mock.calls.map(([draft]) => draft.revision),
    ).toEqual([0, 0]);
  });
});
