import type { WorkspaceNote } from '../types/api/dailyNotes';

export interface CapturedChatReference {
  conversationId: string;
  messageId: string;
  snapshotId: string;
  capturedAt: string;
  noteId: string;
  noteTitle: string;
}

export const chatReferenceKey = (conversationId: string, messageId: string): string =>
  `${conversationId}::${messageId}`;

const toTime = (value: string): number => {
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? 0 : parsed;
};

export const buildCapturedChatReferenceIndex = (
  notes: WorkspaceNote[]
): Map<string, CapturedChatReference> => {
  const index = new Map<string, CapturedChatReference>();

  for (const note of notes) {
    const snapshots = Array.isArray(note.conversationSnapshots)
      ? note.conversationSnapshots
      : [];
    for (const snapshot of snapshots) {
      if (!snapshot.id.startsWith('capture_')) {
        continue;
      }

      const firstMessage = Array.isArray(snapshot.messages)
        ? snapshot.messages[0]
        : undefined;
      if (!snapshot.conversationId || !firstMessage?.id) {
        continue;
      }

      const key = chatReferenceKey(snapshot.conversationId, firstMessage.id);
      const nextValue: CapturedChatReference = {
        conversationId: snapshot.conversationId,
        messageId: firstMessage.id,
        snapshotId: snapshot.id,
        capturedAt: snapshot.capturedAt,
        noteId: note.id,
        noteTitle: note.title,
      };
      const current = index.get(key);
      if (!current || toTime(nextValue.capturedAt) >= toTime(current.capturedAt)) {
        index.set(key, nextValue);
      }
    }
  }

  return index;
};
