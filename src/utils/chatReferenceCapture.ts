import VaultAPI from '../lib/api';

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

export async function captureChatReferenceToWorkspaceNote(
  input: CaptureChatReferenceInput,
): Promise<CaptureChatReferenceResult> {
  const capturedAt = input.capturedAt ?? new Date();
  const date = [capturedAt.getFullYear(), String(capturedAt.getMonth() + 1).padStart(2, '0'), String(capturedAt.getDate()).padStart(2, '0')].join('-');
  const result = await VaultAPI.captureReference({
    conversationId: input.conversationId,
    conversationTitle: input.conversationTitle?.trim() || 'Untitled conversation',
    messageId: input.messageId,
    messageRole: input.messageRole,
    messageContent: input.messageContent,
    documentIds: [...new Set((input.sourceReferences ?? []).map(source => source.documentId?.trim() ?? '').filter(Boolean))],
    capturedAt: capturedAt.toISOString(),
    inboxTitle: `Research Inbox · ${date}`,
    preferredNoteId: input.preferredNoteId ?? null,
    addSnapshot: input.addSnapshot !== false,
  });
  if (!result.ok) throw new Error(result.error);
  return { ...result.data, snapshotId: result.data.snapshotId ?? undefined };
}
