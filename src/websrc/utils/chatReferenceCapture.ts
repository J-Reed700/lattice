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
  messageRole: string;
  messageContent: string;
  referenceTitle?: string | null;
  referenceNote?: string | null;
  sourceReferences?: ChatReferenceSource[];
  capturedAt?: Date;
  addSnapshot?: boolean;
  preferredNoteId?: string | null;
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

const escapeForRegex = (value: string): string =>
  value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const buildCaptureMarker = (conversationId: string, messageId: string): string =>
  `${sanitizeForId(conversationId)}::${sanitizeForId(messageId)}`;

const replaceLegacyMarkerWrappedBlock = (
  content: string,
  marker: string,
  replacement: string
): string | null => {
  const start = `<!-- lattice-capture-start:${marker} -->`;
  const end = `<!-- lattice-capture-end:${marker} -->`;
  const pattern = new RegExp(
    `${escapeForRegex(start)}[\\s\\S]*?${escapeForRegex(end)}`,
    'g'
  );

  if (!pattern.test(content)) {
    return null;
  }

  const replaced = content.replace(pattern, replacement);
  return replaced.replace(/\n{3,}/g, '\n\n').trim();
};

const unwrapLegacyCaptureBlocks = (content: string): string => {
  const blockPattern =
    /<!--\s*lattice-capture-start:[^>]+-->\s*([\s\S]*?)\s*<!--\s*lattice-capture-end:[^>]+-->/g;

  return content.replace(blockPattern, (_match, inner: string) => {
    const normalizedInner = String(inner).replace(/\r\n/g, '\n');
    const messageSplit = normalizedInner.split(/\n### Message\s*\n+/);
    if (messageSplit.length > 1) {
      const message = messageSplit.slice(1).join('\n### Message\n').trim();
      if (message) return message;
    }
    return normalizedInner.trim();
  });
};

const isoDateStamp = (capturedAt: Date): string => {
  const year = capturedAt.getFullYear();
  const month = String(capturedAt.getMonth() + 1).padStart(2, '0');
  const day = String(capturedAt.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
};

const defaultInboxTitle = (capturedAt: Date): string =>
  `Research Inbox · ${isoDateStamp(capturedAt)}`;

const buildSnapshotId = (conversationId: string, messageId: string): string =>
  `capture_${sanitizeForId(conversationId)}_${sanitizeForId(messageId)}`;

const resolveTargetNote = async (
  capturedAt: Date,
  preferredNoteId?: string | null,
): Promise<WorkspaceNote> => {
  const notesResult = await VaultAPI.listWorkspaceNotes();
  if (!notesResult.ok) {
    throw new Error(notesResult.error);
  }

  const notes = Array.isArray(notesResult.data.notes) ? notesResult.data.notes : [];
  const preferred = preferredNoteId
    ? notes.find((note) => note.id === preferredNoteId)
    : null;
  if (preferred) {
    return preferred;
  }

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
  const content = input.messageContent;
  if (!content.trim()) {
    throw new Error('Cannot capture an empty message.');
  }

  const capturedAt = input.capturedAt ?? new Date();
  const capturedAtIso = capturedAt.toISOString();
  const sourceReferences = input.sourceReferences ?? [];
  const sourceDocumentIds = unique(
    sourceReferences
      .map((source) => source.documentId?.trim() ?? '')
      .filter((id) => id.length > 0)
  );
  const conversationTitle = input.conversationTitle?.trim() || 'Untitled conversation';
  const shouldAddSnapshot = input.addSnapshot !== false;
  const captureMarker = buildCaptureMarker(input.conversationId, input.messageId);

  const targetNote = await resolveTargetNote(capturedAt, input.preferredNoteId);
  const currentContent = targetNote.content?.trimEnd() ?? '';
  const markerReplacedContent = replaceLegacyMarkerWrappedBlock(
    currentContent,
    captureMarker,
    content
  );
  const normalizedCurrentContent =
    unwrapLegacyCaptureBlocks(markerReplacedContent ?? currentContent)
      .replace(/^<!-- lattice-capture-(?:start|end):.*? -->\s*$/gm, '')
      .replace(/\n{3,}/g, '\n\n')
      .trimEnd();

  const nextContent = normalizedCurrentContent
    ? (
        normalizedCurrentContent.includes(content)
          ? normalizedCurrentContent
          : `${normalizedCurrentContent}\n\n${content}`
      )
    : content;

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
