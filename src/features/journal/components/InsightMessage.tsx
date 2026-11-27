import { useMemo } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { attachAnchoredOccurrences } from '@/features/reading/citations';
import { CitationHoverCard } from '@/features/reading/components/CitationHoverCard';
import { CitationSourcesRow } from '@/features/reading/components/CitationSourcesRow';
import { useCitationInteractions } from '@/features/reading/hooks/useCitationInteractions';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import { parseSources } from '@/shared/conversations/sourceMetadata';
import { useDrawnReply } from '@/shared/conversations/useDrawnReply';
import type { SnapshotMessage } from '@/types/api/dailyNotes';
import type { SourceWithMetadata } from '@/types/conversation';
import { sanitizeFileName } from '@/utils/sanitize';

/**
 * Parses full structured source metadata from an insight message while
 * accepting legacy `sourceReferences` / `source_references` arrays.
 */
export function parseInsightSources(metadata: string | null | undefined): SourceWithMetadata[] {
  if (!metadata) return [];
  try {
    const parsed = JSON.parse(metadata) as Record<string, unknown>;
    const rawSources = Array.isArray(parsed.sources)
      ? parsed.sources
      : Array.isArray(parsed.sourceReferences)
        ? parsed.sourceReferences
        : Array.isArray(parsed.source_references)
          ? parsed.source_references
          : [];
    return parseSources(rawSources);
  } catch {
    return [];
  }
}

interface InsightMessageProps {
  message: SnapshotMessage;
  onOpenSource: (source: SourceWithMetadata, occurrence?: number | null) => void;
}

export function InsightMessage({ message, onOpenSource }: InsightMessageProps) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const resolvedLocations = useReaderStore((state) => state.resolvedLocations);
  const drawn = useDrawnReply(message);

  const sources = useMemo(
    () => attachAnchoredOccurrences(parseInsightSources(message.metadata), message.reply),
    [message.metadata, message.reply],
  );

  const {
    citationMap,
    citationNumbers,
    articleRef,
    citationHover,
    hoveredSource,
    handleBodyClick,
    handleBodyMouseOver,
    handleBodyMouseLeave,
  } = useCitationInteractions({
    sources,
    ownerKey: `insight:${message.id}`,
    numbered: sources.length > 0,
    showCitations,
    onOpen: (source, occurrence) => onOpenSource(source, occurrence),
    claims: [],
  });

  const footerEntries = useMemo(
    () =>
      Array.from(citationMap.entries())
        .sort(([a], [b]) => a - b)
        .map(([number, source]) => ({
          number,
          source,
          title: sanitizeFileName(source.fileName),
          resolvedLocation: resolvedLocations.get(source.chunkId),
          onView: () => onOpenSource(source, null),
        })),
    [citationMap, onOpenSource, resolvedLocations],
  );

  const createdLabel = useMemo(() => {
    const date = new Date(message.createdAt);
    if (Number.isNaN(date.getTime())) return '';
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }, [message.createdAt]);

  return (
    <article
      ref={articleRef}
      className="border-b border-[hsl(var(--border-subtle)/0.6)] py-4 last:border-b-0"
    >
      {createdLabel && (
        <p className="mb-1.5 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
          {createdLabel}
        </p>
      )}
      <div
        className="journal-insight-body max-w-none wrap-break-word font-serif text-[15px] leading-[1.65] wrap-anywhere"
        onClick={handleBodyClick}
        onMouseOver={handleBodyMouseOver}
        onMouseLeave={handleBodyMouseLeave}
      >
        <TiptapViewer
          content={drawn?.markdown ?? message.content}
          marks={drawn?.numbers}
          citationNumbers={citationNumbers}
          showEvidence={showCitations}
        />
      </div>
      {showCitations && footerEntries.length > 0 && (
        <CitationSourcesRow entries={footerEntries} className="mt-3" />
      )}
      <CitationHoverCard
        hover={showCitations ? citationHover : null}
        source={hoveredSource}
        location={hoveredSource ? resolvedLocations.get(hoveredSource.chunkId) ?? null : null}
      />
    </article>
  );
}
