import type { ConversationMessageBookmarkDto } from '@/types';
import type { PassageReferenceDto } from '@/types/api/references';

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

/** Merges message bookmarks and passage references into one list, newest first. */
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

export const PASSAGE_ORIGIN = 'Document';

export function passageTitle(passage: PassageReferenceDto): string {
  return passage.title?.trim() || passage.fileName;
}

export function passagePreview(passage: PassageReferenceDto): string {
  return passage.text.split('\n').find((line) => line.trim() !== '')?.trim() ?? '';
}

export function bookmarkTitle(bookmark: ConversationMessageBookmarkDto): {
  title: string;
  hasTitle: boolean;
} {
  const named = bookmark.title?.trim() || bookmark.conversationTitle?.trim() || '';
  return named
    ? { title: named, hasTitle: true }
    : { title: 'Untitled reference', hasTitle: false };
}

export function bookmarkOrigin(isJournalOrigin: boolean): string {
  return isJournalOrigin ? 'Journal' : 'Chat';
}

export function bookmarkPreview(bookmark: ConversationMessageBookmarkDto): string {
  return bookmark.messagePreview?.trim() ?? '';
}
