import { beforeEach, describe, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';
import type { WorkspaceNote } from '@/types/api/dailyNotes';
import { captureChatReferenceToWorkspaceNote } from '@/utils/chatReferenceCapture';

const buildNote = (overrides: Partial<WorkspaceNote>): WorkspaceNote => ({
  id: 'note_1',
  title: 'Research Inbox · 2026-02-22',
  content: '',
  linkedDocumentIds: [],
  linkedConversationIds: [],
  highlights: [],
  stickyNotes: [],
  conversationSnapshots: [],
  createdAt: '2026-02-22T10:00:00.000Z',
  updatedAt: '2026-02-22T10:00:00.000Z',
  ...overrides,
});

describe('chatReferenceCapture', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('preserves markdown table syntax in captured note content', async () => {
    const existing = buildNote({});
    const tableMarkdown = [
      'Summary table:',
      '',
      '| Name | Score |',
      '| --- | ---: |',
      '| Alpha | 10 |',
      '| Beta | 20 |',
    ].join('\n');

    vi.mocked(VaultAPI.listWorkspaceNotes).mockResolvedValueOnce({
      ok: true,
      data: { notes: [existing] },
    });

    vi.mocked(VaultAPI.updateWorkspaceNote).mockImplementationOnce(async (note) => ({
      ok: true,
      data: note,
    }));

    await captureChatReferenceToWorkspaceNote({
      conversationId: 'conv_alpha',
      conversationTitle: 'Alpha Conversation',
      messageId: 'msg_table_1',
      messageRole: 'assistant',
      messageContent: tableMarkdown,
    });

    const updateCall = vi.mocked(VaultAPI.updateWorkspaceNote).mock.calls[0];
    expect(updateCall).toBeTruthy();
    const persisted = updateCall[0];
    expect(persisted.content).toContain('| Name | Score |');
    expect(persisted.content).toContain('| --- | ---: |');
    expect(persisted.content).toContain('| Alpha | 10 |');
    expect(persisted.content).not.toContain('> | Name | Score |');
  });

  it('replaces legacy marker-wrapped capture content with raw markdown', async () => {
    const legacyContent = [
      '<!-- recall-capture-start:conv_alpha::msg_table_legacy -->',
      '## Chat Reference · Old Capture',
      'Captured: 2/20/2026, 1:00:00 PM',
      'Conversation: Alpha Conversation',
      'Message Role: assistant',
      'Message ID: msg_table_legacy',
      '',
      '### Message',
      '',
      '| Name | Score |',
      '| --- | ---: |',
      '| Old | 1 |',
      '<!-- recall-capture-end:conv_alpha::msg_table_legacy -->',
      '',
      '## Chat Reference · Different Message',
      'Message ID: msg_other',
      '',
      'hello',
    ].join('\n');

    const existing = buildNote({ content: legacyContent });
    const freshMarkdown = [
      'Latest table:',
      '',
      '| Name | Score |',
      '| --- | ---: |',
      '| New | 42 |',
    ].join('\n');

    vi.mocked(VaultAPI.listWorkspaceNotes).mockResolvedValueOnce({
      ok: true,
      data: { notes: [existing] },
    });

    vi.mocked(VaultAPI.updateWorkspaceNote).mockImplementationOnce(async (note) => ({
      ok: true,
      data: note,
    }));

    await captureChatReferenceToWorkspaceNote({
      conversationId: 'conv_alpha',
      conversationTitle: 'Alpha Conversation',
      messageId: 'msg_table_legacy',
      messageRole: 'assistant',
      messageContent: freshMarkdown,
    });

    const persisted = vi.mocked(VaultAPI.updateWorkspaceNote).mock.calls[0][0];
    expect(persisted.content).toContain('| New | 42 |');
    expect(persisted.content).not.toContain('| Old | 1 |');
    expect(persisted.content).not.toContain('<!-- recall-capture-start:');
    expect(persisted.content).not.toContain('### Message');
    expect(persisted.content).toContain('## Chat Reference · Different Message');
  });

  it('unwraps legacy marker-wrapped blocks even when capturing a different message', async () => {
    const legacyWrapped = [
      '<!-- recall-capture-start:conv_alpha::msg_old -->',
      '## Chat Reference · Legacy',
      'Captured: 2/20/2026, 1:00:00 PM',
      'Conversation: Alpha Conversation',
      'Message Role: assistant',
      'Message ID: msg_old',
      '',
      '### Message',
      '',
      '| Legacy | Value |',
      '| --- | --- |',
      '| Old | 1 |',
      '<!-- recall-capture-end:conv_alpha::msg_old -->',
    ].join('\n');

    const existing = buildNote({ content: legacyWrapped });
    const newContent = [
      'New block',
      '',
      '| Name | Score |',
      '| --- | ---: |',
      '| Fresh | 9 |',
    ].join('\n');

    vi.mocked(VaultAPI.listWorkspaceNotes).mockResolvedValueOnce({
      ok: true,
      data: { notes: [existing] },
    });

    vi.mocked(VaultAPI.updateWorkspaceNote).mockImplementationOnce(async (note) => ({
      ok: true,
      data: note,
    }));

    await captureChatReferenceToWorkspaceNote({
      conversationId: 'conv_alpha',
      conversationTitle: 'Alpha Conversation',
      messageId: 'msg_new',
      messageRole: 'assistant',
      messageContent: newContent,
    });

    const persisted = vi.mocked(VaultAPI.updateWorkspaceNote).mock.calls[0][0];
    expect(persisted.content).toContain('| Old | 1 |');
    expect(persisted.content).toContain('| Fresh | 9 |');
    expect(persisted.content).not.toContain('<!-- recall-capture-start:');
    expect(persisted.content).not.toContain('### Message');
    expect(persisted.content).not.toContain('## Chat Reference · Legacy');
  });
});
