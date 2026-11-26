import { useMemo } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { CitationHoverCard } from '@/features/reading/components/CitationHoverCard';
import { useCitationInteractions } from '@/features/reading/hooks/useCitationInteractions';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { SourceWithMetadata } from '@/types/conversation';
import type { BookmarkPayload } from '@/utils/chatBookmarks';
import { drawReply, replyOf } from '@/utils/replyPieces';

interface ReferenceBodyProps {
  bookmark: ConversationMessageBookmarkDto;
  payload: BookmarkPayload | null;
  resolutionFailed: boolean;
  onViewSource: (source: SourceWithMetadata, occurrence?: number | null) => void;
}

/**
 * Read-only prose body of a reference. Role-aware typography: assistant ->
 * serif, user -> sans, system -> sans/secondary. Falls back to the sidebar
 * preview string while the full payload resolves.
 * Spec §5.3.
 */
export function ReferenceBody({ bookmark, payload, resolutionFailed, onViewSource }: ReferenceBodyProps) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const resolvedLocations = useReaderStore((state) => state.resolvedLocations);
  const sources = payload?.sources ?? [];
  const rawContent = payload?.content ?? bookmark.messagePreview ?? '';
  const reply = payload?.reply;
  const isAssistant = bookmark.messageRole === 'assistant';
  const drawn = useMemo(
    () => (isAssistant ? drawReply(replyOf({ content: rawContent, reply })) : null),
    [isAssistant, rawContent, reply],
  );
  const content = drawn ? drawn.markdown : rawContent;

  const {
    citationNumbers,
    articleRef,
    citationHover,
    hoveredSource,
    handleBodyClick,
    handleBodyMouseOver,
    handleBodyMouseLeave,
  } = useCitationInteractions({
    sources,
    ownerKey: `reference:${bookmark.id}`,
    numbered: sources.length > 0,
    showCitations,
    onOpen: (source, occurrence) => onViewSource(source, occurrence),
    claims: [],
  });

  if (bookmark.messageRole === 'system') {
    return (
      <div className="border-l-2 border-[hsl(var(--border-default))] pl-4">
        <article
          ref={articleRef}
          className="max-w-none wrap-break-word text-sm text-[hsl(var(--text-secondary))] wrap-anywhere"
          onClick={handleBodyClick}
          onMouseOver={handleBodyMouseOver}
          onMouseLeave={handleBodyMouseLeave}
        >
          <TiptapViewer
            content={content}
            citationNumbers={citationNumbers}
            marks={drawn?.numbers}
            showEvidence={showCitations}
          />
        </article>
        {resolutionFailed && (
          <p className="mt-3 text-xs text-[hsl(var(--text-muted))]">
            Full message content couldn't load — showing preview.
          </p>
        )}
        <CitationHoverCard
          hover={showCitations ? citationHover : null}
          source={hoveredSource}
          location={hoveredSource ? resolvedLocations.get(hoveredSource.chunkId) ?? null : null}
        />
      </div>
    );
  }

  return (
    <div>
      <article
        ref={articleRef}
        onClick={handleBodyClick}
        onMouseOver={handleBodyMouseOver}
        onMouseLeave={handleBodyMouseLeave}
        className={
          isAssistant
            ? 'max-w-none wrap-break-word font-serif text-base leading-[1.65] text-[hsl(var(--text-primary))] wrap-anywhere'
            : 'max-w-none wrap-break-word text-base leading-normal text-[hsl(var(--text-primary))] wrap-anywhere'
        }
      >
        <TiptapViewer
          content={content}
          citationNumbers={citationNumbers}
          marks={drawn?.numbers}
          showEvidence={showCitations}
        />
      </article>
      {resolutionFailed && (
        <p className="mt-3 text-xs text-[hsl(var(--text-muted))]">
          Full message content couldn't load — showing preview.
        </p>
      )}
      <CitationHoverCard
        hover={showCitations ? citationHover : null}
        source={hoveredSource}
        location={hoveredSource ? resolvedLocations.get(hoveredSource.chunkId) ?? null : null}
      />
    </div>
  );
}
