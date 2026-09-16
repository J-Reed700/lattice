import { useMemo, useState } from 'react';

import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { ChevronDown, ChevronRight } from 'lucide-react';

import { formatSourceLocation } from '../Reading/passageLocator';

import type { SourceWithMetadata } from '../../types/conversation';

interface GroupedSourceChunk {
  key: string;
  chunkId: string;
  excerpt: string;
  section?: string;
  chunkIndex?: number;
  pageNumber?: number;
  score: number;
  highlights?: string[];
}

interface GroupedSourceEntry {
  key: string;
  primarySource: SourceWithMetadata;
  chunks: GroupedSourceChunk[];
  firstSeenIndex: number;
}

interface SourceCitationsProps {
  sources: SourceWithMetadata[];
  /** "from your journal" / "from your references", keyed by chunk id. */
  provenanceBySource?: Map<string, string>;
  /** Locations a viewer has already resolved, keyed by chunk id. */
  resolvedLocations?: Map<string, string>;
  onViewSource: (_source: SourceWithMetadata) => void;
}

const isWebSource = (source: SourceWithMetadata): boolean => {
  const category = source.category?.toLowerCase() ?? '';
  return source.documentId.startsWith('web:') || category.includes('web article');
};

export function SourceCitations({
  sources,
  provenanceBySource,
  resolvedLocations,
  onViewSource,
}: SourceCitationsProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const prefersReducedMotion = useReducedMotion();

  const groupedSources = useMemo<GroupedSourceEntry[]>(() => {
    const normalizeExcerpt = (value?: string): string | null => {
      if (!value) return null;
      const trimmed = value.trim();
      return trimmed.length > 0 ? trimmed : null;
    };

    const grouped = new Map<string, GroupedSourceEntry>();

    sources.forEach((source, sourceIdx) => {
      const sourceKey = source.documentId || `${source.filePath}:${source.fileName}`;
      const existing = grouped.get(sourceKey);

      const chunkCandidates = source.chunkExcerpts && source.chunkExcerpts.length > 0
        ? source.chunkExcerpts.map((chunk, chunkIdx) => ({
            chunkId: chunk.chunkId || `${source.chunkId || sourceIdx}-${chunkIdx}`,
            excerpt: chunk.excerpt,
            section: chunk.section,
            chunkIndex: chunk.chunkIndex,
            pageNumber: chunk.pageNumber,
            score: chunk.score,
            highlights: chunk.highlights ?? source.highlights,
          }))
        : [{
            chunkId: source.chunkId || `${sourceKey}:chunk-${sourceIdx}`,
            excerpt: source.excerpt ?? source.content,
            section: source.section,
            chunkIndex: source.chunkIndex,
            pageNumber: source.pageNumber,
            score: source.score,
            highlights: source.highlights,
          }];

      const mappedChunks = chunkCandidates
        .map((chunk) => {
          const normalizedExcerpt = normalizeExcerpt(chunk.excerpt);
          if (!normalizedExcerpt) return null;
          const chunkKey = `${sourceKey}:${chunk.chunkId}:${normalizedExcerpt.slice(0, 64)}`;
          return {
            key: chunkKey,
            chunkId: chunk.chunkId,
            excerpt: normalizedExcerpt,
            section: chunk.section,
            chunkIndex: chunk.chunkIndex,
            pageNumber: chunk.pageNumber,
            score: chunk.score,
            highlights: chunk.highlights,
          } as GroupedSourceChunk;
        })
        .filter((chunk): chunk is GroupedSourceChunk => chunk !== null);

      if (!existing) {
        grouped.set(sourceKey, {
          key: sourceKey,
          primarySource: source,
          chunks: mappedChunks,
          firstSeenIndex: sourceIdx,
        });
        return;
      }

      if (source.score > existing.primarySource.score) {
        existing.primarySource = source;
      }

      const seenChunks = new Set(existing.chunks.map((chunk) => chunk.key));
      for (const chunk of mappedChunks) {
        if (!seenChunks.has(chunk.key)) {
          existing.chunks.push(chunk);
          seenChunks.add(chunk.key);
        }
      }
    });

    const groupedSourcesArray = Array.from(grouped.values());

    for (const group of groupedSourcesArray) {
      group.chunks.sort((a, b) => {
        if (a.chunkIndex !== undefined && b.chunkIndex !== undefined) {
          return a.chunkIndex - b.chunkIndex;
        }
        if (a.chunkIndex !== undefined) return -1;
        if (b.chunkIndex !== undefined) return 1;
        return b.score - a.score;
      });
    }

    groupedSourcesArray.sort((a, b) => {
      if (a.primarySource.score === b.primarySource.score) {
        return a.firstSeenIndex - b.firstSeenIndex;
      }
      return b.primarySource.score - a.primarySource.score;
    });

    return groupedSourcesArray;
  }, [sources]);

  const totalExcerptCount = useMemo(
    () => groupedSources.reduce((total, group) => total + group.chunks.length, 0),
    [groupedSources]
  );

  // Web results carry a real ordinal from the search engine, so say it — 1-based,
  // the way the list is numbered. Retrieval scores are not a percentage of
  // anything a reader can name (normalising them made the best hit "100%
  // relevance" every time), so nothing is said about them at all.
  const formatSourceMetric = (
    source: SourceWithMetadata,
    fallbackRank: number
  ): string | undefined => {
    if (!isWebSource(source)) return undefined;
    const rank = source.chunkIndex !== undefined ? source.chunkIndex + 1 : fallbackRank;
    return `Rank ${rank}`;
  };

  if (sources.length === 0) return null;

  const summaryText =
    totalExcerptCount > groupedSources.length
      ? `${groupedSources.length} source${groupedSources.length !== 1 ? 's' : ''} · ${totalExcerptCount} excerpts`
      : `${groupedSources.length} source${groupedSources.length !== 1 ? 's' : ''}`;

  return (
    <div className="mt-4">
      <button
        type="button"
        onClick={() => setIsExpanded((prev) => !prev)}
        className="inline-flex items-center gap-1.5 text-sm text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))]"
        aria-expanded={isExpanded}
      >
        {isExpanded ? (
          <ChevronDown className="h-3.5 w-3.5" />
        ) : (
          <ChevronRight className="h-3.5 w-3.5" />
        )}
        <span>{summaryText}</span>
      </button>

      <AnimatePresence initial={false}>
      {isExpanded && (
        <motion.div
          key="source-citations-expand"
          initial={prefersReducedMotion ? false : { height: 0, opacity: 0 }}
          animate={{ height: 'auto', opacity: 1 }}
          exit={prefersReducedMotion ? undefined : { height: 0, opacity: 0 }}
          transition={prefersReducedMotion ? { duration: 0 } : { duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          style={{ overflow: 'hidden' }}
        >
        <ul className="mt-3 space-y-4">
          {groupedSources.map((group, idx) => {
            const source = group.primarySource;
            // Provenance leads: "from your journal" is the most useful thing
            // a reader can learn about a citation at a glance.
            const metaParts = [
              provenanceBySource?.get(source.chunkId),
              source.category,
              formatSourceMetric(source, idx + 1),
              group.chunks.length > 0
                ? `${group.chunks.length} excerpt${group.chunks.length !== 1 ? 's' : ''}`
                : undefined,
            ].filter(Boolean);

            return (
              <li key={group.key} className="space-y-2 rounded-lg border border-border-subtle bg-surface px-4 py-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline gap-2">
                      <span className="font-mono text-xs text-[hsl(var(--text-muted))]">
                        [{source.citationId ?? idx + 1}]
                      </span>
                      <span className="text-sm font-semibold text-[hsl(var(--text-primary))] break-words">
                        {source.fileName}
                      </span>
                    </div>
                    {metaParts.length > 0 && (
                      <div className="mt-0.5 text-xs text-[hsl(var(--text-muted))]">
                        {metaParts.join(' · ')}
                      </div>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => onViewSource(source)}
                    className="shrink-0 text-xs text-[hsl(var(--accent))] underline-offset-2 hover:underline"
                    title={`Open ${source.fileName}`}
                  >
                    View source
                  </button>
                </div>

                {group.chunks.length > 0 && (
                  <div className="space-y-1.5">
                    {group.chunks.map((chunk) => {
                      // Where the passage lives, when that is actually known.
                      // A chunk ordinal is not a location, so nothing shows
                      // rather than "Chunk 7".
                      const chunkMetaParts = [
                        resolvedLocations?.get(chunk.chunkId) ??
                          formatSourceLocation({
                            section: chunk.section,
                            chunkId: chunk.chunkId,
                            pageNumber: chunk.pageNumber,
                          }),
                      ].filter(Boolean);

                      return (
                        <div
                          key={chunk.key}
                          className="border-l-2 border-accent/30 pl-4 py-2"
                        >
                          {chunkMetaParts.length > 0 && (
                            <div className="mb-1 text-xs text-[hsl(var(--text-muted))]">
                              {chunkMetaParts.join(' · ')}
                            </div>
                          )}
                          <button type="button"
                            className="mb-1 text-xs text-[hsl(var(--accent))] hover:underline"
                            onClick={() => onViewSource(sources.find(candidate => candidate.chunkId === chunk.chunkId) ?? source)}>
                            View passage [{sources.find(candidate => candidate.chunkId === chunk.chunkId)?.citationId ?? source.citationId ?? idx + 1}]
                          </button>
                          <p className="text-sm text-[hsl(var(--text-secondary))] leading-relaxed line-clamp-4 break-words">
                            {chunk.excerpt}
                          </p>
                        </div>
                      );
                    })}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
        </motion.div>
      )}
      </AnimatePresence>
    </div>
  );
}
