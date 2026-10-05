import { format, isThisYear } from 'date-fns';

import type { ConversationMessageBookmarkDto } from '@/types';
import type { ConversationSpaceDto } from '@/types/api/conversation';
import type { CapturedChatReference } from '@/utils/chatReferenceIndex';

interface ReferenceHeaderProps {
  bookmark: ConversationMessageBookmarkDto;
  space: ConversationSpaceDto | null;
  capture: CapturedChatReference | null;
  onOpenConversation: () => void;
  onOpenCapturedNote: () => void;
}

function formatFriendly(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return isThisYear(date)
    ? format(date, 'EEEE, MMMM d')
    : format(date, 'MMM d, yyyy');
}

function roleLabel(role: string): string {
  if (role === 'assistant') return 'Assistant';
  if (role === 'user') return 'You';
  return 'System';
}

/**
 * Reference reader header: serif origin title, origin-context + timestamp,
 * metadata row (role · capture status · space).
 * Spec §5.2.
 */
export function ReferenceHeader({
  bookmark,
  space,
  capture,
  onOpenConversation,
  onOpenCapturedNote,
}: ReferenceHeaderProps) {
  const userTitle = bookmark.title?.trim();
  const conversationTitle = bookmark.conversationTitle?.trim();
  const displayTitle = userTitle || conversationTitle || 'Untitled reference';
  const showConversationLink = Boolean(userTitle && conversationTitle);
  const friendlyDate = formatFriendly(bookmark.createdAt);

  return (
    <header className="mb-8">
      <h1 className="font-serif text-2xl font-semibold tracking-[-0.015em] text-[hsl(var(--text-primary))]">
        {displayTitle}
      </h1>
      {(showConversationLink || friendlyDate) && (
        <p className="mt-1 text-sm text-[hsl(var(--text-tertiary))]">
          {showConversationLink && conversationTitle ? (
            <>
              From{' '}
              <button
                type="button"
                onClick={onOpenConversation}
                className="text-[hsl(var(--accent))] underline-offset-2 hover:underline"
                title="Open conversation in Chat"
              >
                {conversationTitle}
              </button>
              {friendlyDate && <> · {friendlyDate}</>}
            </>
          ) : (
            friendlyDate
          )}
        </p>
      )}
      <p className="mt-2 text-xs text-[hsl(var(--text-muted))]">
        <span>{roleLabel(bookmark.messageRole)}</span>
        {capture && (
          <>
            {' · Captured to '}
            <button
              type="button"
              onClick={onOpenCapturedNote}
              className="text-[hsl(var(--accent))] underline-offset-2 hover:underline"
              title="Open captured note"
            >
              {capture.noteTitle || 'note'}
            </button>
          </>
        )}
        {space && <> · {space.name}</>}
      </p>
    </header>
  );
}
