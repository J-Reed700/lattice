import { useMemo } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import type { ConversationMessageBookmarkDto } from '@/types';
import { normalizeAssistantMarkdown } from '@/utils/assistantMarkdown';
import type { BookmarkPayload } from '@/utils/chatBookmarks';

interface ReferenceBodyProps {
  bookmark: ConversationMessageBookmarkDto;
  payload: BookmarkPayload | null;
  resolutionFailed: boolean;
}

/**
 * Read-only prose body of a reference. Role-aware typography: assistant ->
 * serif, user -> sans, system -> sans/secondary. Falls back to the sidebar
 * preview string while the full payload resolves.
 * Spec §5.3.
 */
export function ReferenceBody({ bookmark, payload, resolutionFailed }: ReferenceBodyProps) {
  const rawContent = payload?.content ?? bookmark.messagePreview ?? '';
  const content = useMemo(() => {
    if (bookmark.messageRole === 'assistant') {
      return normalizeAssistantMarkdown(rawContent);
    }
    return rawContent;
  }, [bookmark.messageRole, rawContent]);

  if (bookmark.messageRole === 'system') {
    return (
      <div className="border-l-2 border-[hsl(var(--border-default))] pl-4">
        <div className="max-w-none break-words text-sm text-[hsl(var(--text-secondary))] [overflow-wrap:anywhere]">
          <TiptapViewer content={content} />
        </div>
        {resolutionFailed && (
          <p className="mt-3 text-xs text-[hsl(var(--text-muted))]">
            Full message content couldn't load — showing preview.
          </p>
        )}
      </div>
    );
  }

  const isAssistant = bookmark.messageRole === 'assistant';
  return (
    <div>
      <div
        className={
          isAssistant
            ? 'max-w-none break-words font-serif text-base leading-[1.65] text-[hsl(var(--text-primary))] [overflow-wrap:anywhere]'
            : 'max-w-none break-words text-base leading-[1.5] text-[hsl(var(--text-primary))] [overflow-wrap:anywhere]'
        }
      >
        <TiptapViewer content={content} />
      </div>
      {resolutionFailed && (
        <p className="mt-3 text-xs text-[hsl(var(--text-muted))]">
          Full message content couldn't load — showing preview.
        </p>
      )}
    </div>
  );
}
