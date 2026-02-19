import VaultAPI from '../lib/api';

import type {
  ConversationSnapshot,
  SnapshotMessage,
  WorkspaceNote,
} from '../types/api/dailyNotes';

export interface ChatReferenceSource {
  documentId?: string | null;
  label?: string | null;
}

export interface CaptureChatReferenceInput {
  conversationId: string;
  conversationTitle?: string | null;
  messageId: string;
  messageRole: 'user' | 'assistant' | 'system';
  messageContent: string;
  referenceTitle?: string | null;
  referenceNote?: string | null;
  sourceReferences?: ChatReferenceSource[];
  capturedAt?: Date;
  addSnapshot?: boolean;
}

export interface CaptureChatReferenceResult {
  noteId: string;
  noteTitle: string;
  linkedDocumentCount: number;
  snapshotId?: string;
}

const unique = (values: string[]): string[] => [...new Set(values.filter(Boolean))];

const sanitizeForId = (value: string): string =>
  value.replace(/[^a-zA-Z0-9_-]+/g, '_').slice(0, 72);

const isoDateStamp = (capturedAt: Date): string => {
  const year = capturedAt.getFullYear();
  const month = String(capturedAt.getMonth() + 1).padStart(2, '0');
  const day = String(capturedAt.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
};

const defaultInboxTitle = (capturedAt: Date): string =>
  `Research Inbox · ${isoDateStamp(capturedAt)}`;

const summarizeContent = (content: string): string => {
  const compact = content.replace(/\s+/g, ' ').trim();
  if (!compact) return 'Chat Reference';
  return compact.length > 88 ? `${compact.slice(0, 88)}...` : compact;
};

const quoteBlock = (content: string): string =>
  content
    .trim()
    .split('\n')
    .map((line) => (line.length > 0 ? `> ${line}` : '>'))
    .join('\n');

const buildSnapshotId = (conversationId: string, messageId: string): string =>
  `capture_${sanitizeForId(conversationId)}_${sanitizeForId(messageId)}`;

const resolveTargetNote = async (capturedAt: Date): Promise<WorkspaceNote> => {
  const notesResult = await VaultAPI.listWorkspaceNotes();
  if (!notesResult.ok) {
    throw new Error(notesResult.error);
  }

  const notes = Array.isArray(notesResult.data.notes) ? notesResult.data.notes : [];
  const targetTitle = defaultInboxTitle(capturedAt);
  const existingDailyInbox = notes.find((note) => note.title.trim() === targetTitle);
  if (existingDailyInbox) {
    return existingDailyInbox;
  }

  const createResult = await VaultAPI.createWorkspaceNote(targetTitle);
  if (!createResult.ok) {
    throw new Error(createResult.error);
  }
  return createResult.data;
};

export async function captureChatReferenceToWorkspaceNote(
  input: CaptureChatReferenceInput
): Promise<CaptureChatReferenceResult> {
  const content = input.messageContent.trim();
  if (!content) {
    throw new Error('Cannot capture an empty message.');
  }

  const capturedAt = input.capturedAt ?? new Date();
  const capturedAtIso = capturedAt.toISOString();
  const sourceReferences = input.sourceReferences ?? [];
  const sourceLabels = unique(
    sourceReferences
      .map((source) => source.label?.trim() ?? '')
      .filter((label) => label.length > 0)
  );
  const sourceDocumentIds = unique(
    sourceReferences
      .map((source) => source.documentId?.trim() ?? '')
      .filter((id) => id.length > 0)
  );
  const referenceTitle = input.referenceTitle?.trim() || summarizeContent(content);
  const conversationTitle = input.conversationTitle?.trim() || 'Untitled conversation';
  const referenceNote = input.referenceNote?.trim();
  const shouldAddSnapshot = input.addSnapshot !== false;

  const blockLines: string[] = [
    `## Chat Reference · ${referenceTitle}`,
    `Captured: ${capturedAt.toLocaleString()}`,
    `Conversation: ${conversationTitle}`,
    `Message Role: ${input.messageRole}`,
    `Message ID: ${input.messageId}`,
  ];

  if (referenceNote) {
    blockLines.push(`Note: ${referenceNote}`);
  }

  if (sourceLabels.length > 0) {
    blockLines.push('Sources:');
    blockLines.push(...sourceLabels.map((label) => `- ${label}`));
  }

  blockLines.push('', quoteBlock(content), '');
  const block = blockLines.join('\n');

  const targetNote = await resolveTargetNote(capturedAt);
  const currentContent = targetNote.content?.trimEnd() ?? '';
  const nextContent = currentContent ? `${currentContent}\n\n${block}` : block.trim();
  const snapshotId = buildSnapshotId(input.conversationId, input.messageId);

  const snapshotMessage: SnapshotMessage = {
    id: input.messageId,
    role: input.messageRole,
    content,
    createdAt: capturedAtIso,
  };

  const existingSnapshots = Array.isArray(targetNote.conversationSnapshots)
    ? targetNote.conversationSnapshots
    : [];
  const nextSnapshots: ConversationSnapshot[] = shouldAddSnapshot
    ? [
        {
          id: snapshotId,
          conversationId: input.conversationId,
          conversationTitle,
          capturedAt: capturedAtIso,
          messageCount: 1,
          messages: [snapshotMessage],
        },
        ...existingSnapshots.filter((snapshot) => snapshot.id !== snapshotId),
      ].slice(0, 120)
    : existingSnapshots;

  const updatedNote: WorkspaceNote = {
    ...targetNote,
    content: nextContent,
    linkedConversationIds: unique([
      ...((Array.isArray(targetNote.linkedConversationIds)
        ? targetNote.linkedConversationIds
        : [])),
      input.conversationId,
    ]),
    linkedDocumentIds: unique([
      ...((Array.isArray(targetNote.linkedDocumentIds)
        ? targetNote.linkedDocumentIds
        : [])),
      ...sourceDocumentIds,
    ]),
    conversationSnapshots: nextSnapshots,
    updatedAt: capturedAtIso,
  };

  const updateResult = await VaultAPI.updateWorkspaceNote(updatedNote);
  if (!updateResult.ok) {
    throw new Error(updateResult.error);
  }

  return {
    noteId: updateResult.data.id,
    noteTitle: updateResult.data.title,
    linkedDocumentCount: sourceDocumentIds.length,
    snapshotId: shouldAddSnapshot ? snapshotId : undefined,
  };
}
