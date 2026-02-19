import { VaultAPI } from '../lib/api';

import type { ChatReferenceSource } from './chatReferenceCapture';
import type { ConversationMessageBookmarkDto } from '../types';
import type { ConversationMessage } from '../types/conversation';

export interface BookmarkPayload {
  content: string;
  sourceReferences: ChatReferenceSource[];
}

const toStringOrEmpty = (value: unknown): string =>
  typeof value === 'string' ? value : '';

export const parseBookmarkSourceReferences = (
  metadata: string | null | undefined,
  directSources?: unknown
): ChatReferenceSource[] => {
  const byKey = new Map<string, ChatReferenceSource>();

  const addSource = (raw: unknown) => {
    if (!raw || typeof raw !== 'object') return;
    const source = raw as Record<string, unknown>;
    const documentIdRaw = source.documentId ?? source.document_id;
    const fileNameRaw = source.fileName ?? source.file_name;
    const filePathRaw = source.filePath ?? source.file_path;

    const documentId = toStringOrEmpty(documentIdRaw).trim() || undefined;
    const label = (toStringOrEmpty(fileNameRaw) || toStringOrEmpty(filePathRaw)).trim() || undefined;
    if (!documentId && !label) return;

    const key = `${documentId ?? ''}|${label ?? ''}`;
    if (!byKey.has(key)) {
      byKey.set(key, { documentId, label });
    }
  };

  if (Array.isArray(directSources)) {
    directSources.forEach(addSource);
  }

  if (metadata) {
    try {
      const parsed = JSON.parse(metadata) as Record<string, unknown>;
      if (Array.isArray(parsed.sources)) {
        parsed.sources.forEach(addSource);
      }
    } catch {
      // Ignore malformed metadata JSON.
    }
  }

  return [...byKey.values()];
};

export async function resolveBookmarkPayload(
  bookmark: ConversationMessageBookmarkDto,
  loadedMessages?: ConversationMessage[]
): Promise<BookmarkPayload> {
  const loadedMessage = loadedMessages?.find((message) => message.id === bookmark.messageId);
  if (loadedMessage?.content) {
    return {
      content: loadedMessage.content,
      sourceReferences: parseBookmarkSourceReferences(
        loadedMessage.metadata,
        loadedMessage.sources
      ),
    };
  }

  const result = await VaultAPI.getConversationMessages(bookmark.conversationId);
  if (!result.ok) {
    throw new Error(
      `Unable to load full message content for reference capture: ${result.error}`
    );
  }

  const messages = Array.isArray(result.data) ? result.data : result.data.messages;
  const matched = messages.find((message) => message.id === bookmark.messageId);
  if (!matched?.content) {
    throw new Error(
      'Unable to load full message content for reference capture: message was not found.'
    );
  }

  return {
    content: matched.content,
    sourceReferences: parseBookmarkSourceReferences(matched.metadata, matched.sources),
  };
}
