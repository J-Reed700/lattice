import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import { captureChatReferenceToWorkspaceNote } from '@/utils/chatReferenceCapture';

vi.mock('@/lib/api', () => {
  const api = { captureReference: vi.fn() };
  return { default: api, VaultAPI: api };
});
describe('chat reference capture transport', () => {
  beforeEach(() => vi.clearAllMocks());
  it('sends an append operation with unchanged markdown and the local destination date', async () => {
    vi.mocked(VaultAPI.captureReference).mockResolvedValue({ ok: true, data: { noteId: 'note', noteTitle: 'Journal', snapshotId: 'snapshot', linkedDocumentCount: 1 } });
    const content = '| Name | Score |\n| --- | ---: |\n| Alpha | 10 |';
    const result = await captureChatReferenceToWorkspaceNote({ conversationId: 'conversation', messageId: 'message', messageRole: 'assistant', messageContent: content, preferredNoteId: 'note', capturedAt: new Date(2026, 9, 5, 12), sourceReferences: [{ documentId: 'document' }, { documentId: 'document' }] });
    expect(VaultAPI.captureReference).toHaveBeenCalledWith(expect.objectContaining({ messageContent: content, inboxTitle: 'Research Inbox · 2026-10-05', preferredNoteId: 'note', documentIds: ['document'] }));
    expect(result.snapshotId).toBe('snapshot');
  });
  it('propagates a rejected capture without announcing a destination', async () => {
    vi.mocked(VaultAPI.captureReference).mockResolvedValue({ ok: false, error: 'Destination removed' });
    await expect(captureChatReferenceToWorkspaceNote({ conversationId: 'conversation', messageId: 'message', messageRole: 'assistant', messageContent: 'Content' })).rejects.toThrow('Destination removed');
  });
});
