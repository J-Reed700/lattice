import { useMemo } from 'react';

import { messageImages } from '@/features/chat/controller/messageMetadata';
import type { DisplayMessage } from '@/types/conversation';

/**
 * What a message's metadata records: the files and pictures it brought in,
 * whether the model was shown those pictures, and the chat it continues.
 */
export function useMessageMetadata(message: DisplayMessage, isUser: boolean) {

  // The files this message brought into the conversation, stamped on it when
  // it was sent. Absent metadata or a parse failure is "no attachments",
  // never an error — older messages simply have no record.
  const attachmentNames = useMemo(() => {
    if (!isUser || !('metadata' in message) || !message.metadata) return [] as string[];
    try {
      const parsed: unknown = JSON.parse(message.metadata);
      const names = (parsed as { attachments?: unknown })?.attachments;
      if (!Array.isArray(names)) return [] as string[];
      return names.filter((name): name is string => typeof name === 'string');
    } catch {
      return [] as string[];
    }
  }, [isUser, message]);

  // A chat continued from another opens with a summary of it. That turn was
  // written from the old thread, not in answer to anything here, so it says
  // where it came from and links back.
  const continuedFrom = useMemo(() => {
    if (isUser || !('metadata' in message) || !message.metadata) return null;
    try {
      const parsed = (JSON.parse(message.metadata) as { continuedFrom?: { conversationId?: unknown; title?: unknown } })?.continuedFrom;
      if (typeof parsed?.conversationId !== 'string') return null;
      return { conversationId: parsed.conversationId, title: typeof parsed.title === 'string' ? parsed.title : 'the earlier chat' };
    } catch {
      return null;
    }
  }, [isUser, message]);

  // The pictures a user message carried. A pending bubble holds them itself;
  // a saved one has them in its metadata.
  const pictures = useMemo(() => (isUser ? messageImages(message) : []), [isUser, message]);

  // `false` only when the turn told the model the pictures were there instead
  // of showing them; unknown (older messages, a pending bubble) is `null`.
  const picturesSeen = useMemo(() => {
    if (!isUser || !('metadata' in message) || !message.metadata) return null;
    try {
      const seen = (JSON.parse(message.metadata) as { picturesSeen?: unknown })?.picturesSeen;
      return typeof seen === 'boolean' ? seen : null;
    } catch {
      return null;
    }
  }, [isUser, message]);

  return { attachmentNames, pictures, picturesSeen, continuedFrom };
}
