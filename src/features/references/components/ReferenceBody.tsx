import { useMemo, type MouseEvent } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { useCitationDisplayStore } from '@/stores/citationDisplayStore';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { SourceWithMetadata } from '@/types/conversation';
import { normalizeAssistantMarkdown } from '@/utils/assistantMarkdown';
import type { BookmarkPayload } from '@/utils/chatBookmarks';
import { createCitationMap } from '@/utils/citations';

interface ReferenceBodyProps {
  bookmark: ConversationMessageBookmarkDto;
  payload: BookmarkPayload | null;
  resolutionFailed: boolean;
  onViewSource: (source: SourceWithMetadata) => void;
}

/**
 * Read-only prose body of a reference. Role-aware typography: assistant ->
 * serif, user -> sans, system -> sans/secondary. Falls back to the sidebar
 * preview string while the full payload resolves.
 * Spec §5.3.
 */
export function ReferenceBody({ bookmark, payload, resolutionFailed, onViewSource }: ReferenceBodyProps) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const sources = payload?.sources;
  const citationMap = useMemo(() => createCitationMap(sources ?? []), [sources]);
  const citationNumbers = useMemo(() => [...citationMap.keys()], [citationMap]);
  const rawContent = payload?.content ?? bookmark.messagePreview ?? '';
  const content = useMemo(() => {
    if (bookmark.messageRole === 'assistant') {
      return normalizeAssistantMarkdown(rawContent);
    }
    return rawContent;
  }, [bookmark.messageRole, rawContent]);

  const handleSourceClick = (event: MouseEvent<HTMLElement>) => {
    if (!showCitations || !(event.target instanceof Element)) return;
    const chip = event.target.closest<HTMLElement>('[data-cite]');
    const source = chip ? citationMap.get(Number(chip.dataset.cite)) : undefined;
    if (!source) return;
    event.preventDefault();
    onViewSource(source);
  };

  if (bookmark.messageRole === 'system') {
    return (
      <div className="border-l-2 border-[hsl(var(--border-default))] pl-4">
        <div className="max-w-none wrap-break-word text-sm text-[hsl(var(--text-secondary))] wrap-anywhere" onClick={handleSourceClick}>
          <TiptapViewer content={content} citationNumbers={citationNumbers} showEvidence={showCitations} />
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
        onClick={handleSourceClick}
        className={
          isAssistant
            ? 'max-w-none wrap-break-word font-serif text-base leading-[1.65] text-[hsl(var(--text-primary))] wrap-anywhere'
            : 'max-w-none wrap-break-word text-base leading-normal text-[hsl(var(--text-primary))] wrap-anywhere'
        }
      >
        <TiptapViewer content={content} citationNumbers={citationNumbers} showEvidence={showCitations} />
      </div>
      {resolutionFailed && (
        <p className="mt-3 text-xs text-[hsl(var(--text-muted))]">
          Full message content couldn't load — showing preview.
        </p>
      )}
    </div>
  );
}
