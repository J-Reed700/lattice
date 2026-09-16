import { describe, expect, it } from 'vitest';

import type { WorkspaceNote } from '@/types/api/dailyNotes';
import {
  buildCapturedChatReferenceIndex,
  chatReferenceKey,
} from '@/utils/chatReferenceIndex';

const buildNote = (overrides: Partial<WorkspaceNote>): WorkspaceNote => ({
  id: 'note_default',
  title: 'Default Note',
  content: '',
  linkedDocumentIds: [],
  linkedConversationIds: [],
  highlights: [],
  stickyNotes: [],
  conversationSnapshots: [],
  createdAt: '2026-02-19T10:00:00.000Z',
  updatedAt: '2026-02-19T10:00:00.000Z',
  ...overrides,
});

describe('chatReferenceIndex', () => {
  it('indexes capture snapshots and ignores non-capture snapshots', () => {
    const notes = [
      buildNote({
        id: 'note_1',
        title: 'Research Inbox · 2026-02-19',
        conversationSnapshots: [
          {
            id: 'capture_conv_alpha_msg_1',
            conversationId: 'conv_alpha',
            conversationTitle: 'Alpha',
            capturedAt: '2026-02-19T10:00:00.000Z',
            messageCount: 1,
            messages: [{ id: 'msg_1', role: 'assistant', content: 'A', createdAt: '2026-02-19T10:00:00.000Z' }],
          },
          {
            id: 'snapshot_manual',
            conversationId: 'conv_alpha',
            conversationTitle: 'Alpha Manual',
            capturedAt: '2026-02-19T10:05:00.000Z',
            messageCount: 1,
            messages: [{ id: 'msg_2', role: 'assistant', content: 'B', createdAt: '2026-02-19T10:05:00.000Z' }],
          },
        ],
      }),
    ];

    const index = buildCapturedChatReferenceIndex(notes);
    expect(index.size).toBe(1);
    expect(index.get(chatReferenceKey('conv_alpha', 'msg_1'))?.noteId).toBe('note_1');
    expect(index.has(chatReferenceKey('conv_alpha', 'msg_2'))).toBe(false);
  });

  it('prefers the newest capture when the same message is captured multiple times', () => {
    const notes = [
      buildNote({
        id: 'note_old',
        title: 'Research Inbox · 2026-02-18',
        conversationSnapshots: [
          {
            id: 'capture_conv_alpha_msg_1_old',
            conversationId: 'conv_alpha',
            conversationTitle: 'Alpha',
            capturedAt: '2026-02-18T10:00:00.000Z',
            messageCount: 1,
            messages: [{ id: 'msg_1', role: 'assistant', content: 'Old', createdAt: '2026-02-18T10:00:00.000Z' }],
          },
        ],
      }),
      buildNote({
        id: 'note_new',
        title: 'Research Inbox · 2026-02-19',
        conversationSnapshots: [
          {
            id: 'capture_conv_alpha_msg_1_new',
            conversationId: 'conv_alpha',
            conversationTitle: 'Alpha',
            capturedAt: '2026-02-19T09:00:00.000Z',
            messageCount: 1,
            messages: [{ id: 'msg_1', role: 'assistant', content: 'New', createdAt: '2026-02-19T09:00:00.000Z' }],
          },
        ],
      }),
    ];

    const index = buildCapturedChatReferenceIndex(notes);
    const captured = index.get(chatReferenceKey('conv_alpha', 'msg_1'));
    expect(captured?.noteId).toBe('note_new');
    expect(captured?.snapshotId).toBe('capture_conv_alpha_msg_1_new');
  });
});
