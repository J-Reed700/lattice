import type { ConversationMessageBookmarkDto } from '@/types';
import type { PassageReferenceDto } from '@/types/api/references';

/**
 * One row in the reference inbox. Message bookmarks reference a chat turn;
 * passages reference a place in a document. They are different entities kept
 * in different tables, merged only for display.
 */
export type InboxItem =
  | {
      kind: 'message';
      id: string;
      createdAt: string;
      bookmark: ConversationMessageBookmarkDto;
    }
  | {
      kind: 'passage';
      id: string;
      createdAt: string;
      passage: PassageReferenceDto;
    };

function timestamp(value: string): number {
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

/**
 * Merges message bookmarks and passage references into one list, newest first.
 * Ties break message-before-passage, then by id, so the order is total and the
 * list never reshuffles between renders. An unparseable timestamp sinks to the
 * end rather than throwing.
 */
export function mergeInboxItems(
  bookmarks: ConversationMessageBookmarkDto[],
  passages: PassageReferenceDto[],
): InboxItem[] {
  const items: InboxItem[] = [
    ...bookmarks.map<InboxItem>((bookmark) => ({
      kind: 'message',
      id: bookmark.id,
      createdAt: bookmark.createdAt,
      bookmark,
    })),
    ...passages.map<InboxItem>((passage) => ({
      kind: 'passage',
      id: passage.id,
      createdAt: passage.createdAt,
      passage,
    })),
  ];

  return items.sort((a, b) => {
    const delta = timestamp(b.createdAt) - timestamp(a.createdAt);
    if (delta !== 0) return delta;
    if (a.kind !== b.kind) return a.kind === 'message' ? -1 : 1;
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });
}

/** Origin label for a saved passage. */
export const PASSAGE_ORIGIN = 'Document';

/** Title of a saved passage: its own title, else the file it came from. */
export function passageTitle(passage: PassageReferenceDto): string {
  return passage.title?.trim() || passage.fileName;
}

/** One-line preview of a saved passage: its first non-blank line. */
export function passagePreview(passage: PassageReferenceDto): string {
  return passage.text.split('\n').find((line) => line.trim() !== '')?.trim() ?? '';
}

/**
 * Title of a message bookmark: its own title, else the conversation it came
 * from. `hasTitle` is false when neither exists and the fallback is showing,
 * which is what the row renders in italics.
 */
export function bookmarkTitle(bookmark: ConversationMessageBookmarkDto): {
  title: string;
  hasTitle: boolean;
} {
  const named = bookmark.title?.trim() || bookmark.conversationTitle?.trim() || '';
  return named
    ? { title: named, hasTitle: true }
    : { title: 'Untitled reference', hasTitle: false };
}

/** Origin label for a message bookmark: "Journal" when its space is a journal. */
export function bookmarkOrigin(isJournalOrigin: boolean): string {
  return isJournalOrigin ? 'Journal' : 'Chat';
}

/** One-line preview of a message bookmark. */
export function bookmarkPreview(bookmark: ConversationMessageBookmarkDto): string {
  return bookmark.messagePreview?.trim() ?? '';
}
