import { useMemo, useState } from 'react';

import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { ChevronDown, ChevronRight } from 'lucide-react';

import type { SourceWithMetadata } from '../../types/conversation';

interface GroupedSourceChunk {
  key: string;
  chunkId: string;
  excerpt: string;
  section?: string;
  chunkIndex?: number;
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
  onViewSource: (source: SourceWithMetadata) => void;
}

const isWebSource = (source: SourceWithMetadata): boolean => {
  const category = source.category?.toLowerCase() ?? '';
  return source.documentId.startsWith('web:') || category.includes('web article');
};

const escapeRegExp = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const termEntropy = (term: string) => {
  if (!term) return 0;
  const counts = new Map<string, number>();
  for (const ch of term) {
    counts.set(ch, (counts.get(ch) || 0) + 1);
  }
  let entropy = 0;
  for (const count of counts.values()) {
    const p = count / term.length;
    entropy -= p * Math.log2(p);
  }
  return entropy;
};

const termSalience = (term: string) => {
  const entropy = termEntropy(term);
  const lengthFactor = Math.log(term.length + 1);
  return entropy * (0.65 + 0.35 * lengthFactor);
};

const selectInformativeTerms = (terms: string[], maxTerms: number) => {
  const normalized = Array.from(
    new Set(
      terms
        .map((term) => term.trim().toLowerCase())
        .filter((term) => term.length >= 3 && /[a-z]/i.test(term))
    )
  );
  if (normalized.length === 0) return [];

  const ranked = normalized
    .map((term) => ({ term, score: termSalience(term) }))
    .sort((a, b) => b.score - a.score || a.term.localeCompare(b.term));

  const bestScore = ranked[0]?.score ?? 0;
  if (bestScore <= Number.EPSILON) {
    return ranked.slice(0, maxTerms).map((entry) => entry.term);
  }

  const selected = ranked
    .filter((entry) => entry.score >= bestScore * 0.45)
    .slice(0, maxTerms)
    .map((entry) => entry.term);

  if (selected.length > 0) return selected;
  return ranked.slice(0, maxTerms).map((entry) => entry.term);
};

const renderHighlightedText = (text: string, highlights?: string[]) => {
  if (!highlights || highlights.length === 0) return text;

  const selected = selectInformativeTerms(highlights, 10);
  if (selected.length === 0) return text;

  const ordered = [...selected].sort((a, b) => b.length - a.length);
  const pattern = new RegExp(`\\b(${ordered.map(escapeRegExp).join('|')})\\b`, 'gi');
  const parts = text.split(pattern);
  const lookup = new Set(selected.map((term) => term.toLowerCase()));

  return parts.map((part, idx) =>
    lookup.has(part.toLowerCase()) ? (
      <mark
        key={`hl-${idx}`}
        className="rounded-sm bg-[hsl(var(--accent-muted))] px-0.5 text-[hsl(var(--text-primary))]"
      >
        {part}
      </mark>
    ) : (
      <span key={`hl-${idx}`}>{part}</span>
    )
  );
};

export function SourceCitations({ sources, onViewSource }: SourceCitationsProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const prefersReducedMotion = useReducedMotion();

  const maxKbScore = useMemo(() => {
    const kbScores = sources
      .filter((source) => !isWebSource(source))
      .map((source) => source.score)
      .filter((score) => Number.isFinite(score) && score > 0);
    if (kbScores.length === 0) return 0;
    return Math.max(...kbScores);
  }, [sources]);

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
            score: chunk.score,
            highlights: chunk.highlights ?? source.highlights,
          }))
        : [{
            chunkId: source.chunkId || `${sourceKey}:chunk-${sourceIdx}`,
            excerpt: source.excerpt ?? source.content,
            section: source.section,
            chunkIndex: source.chunkIndex,
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

  const formatSourceMetric = (source: SourceWithMetadata, fallbackRank: number): string => {
    if (isWebSource(source)) {
      const rank = source.chunkIndex !== undefined ? source.chunkIndex : fallbackRank;
      return `Rank #${rank}`;
    }
    if (maxKbScore <= 0) {
      return 'No score';
    }
    const normalized = Math.min(1, Math.max(0, source.score / maxKbScore));
    return `${(normalized * 100).toFixed(1)}% relevance`;
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
          exit={prefersReducedMotion ? { opacity: 0 } : { height: 0, opacity: 0 }}
          transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          style={{ overflow: 'hidden' }}
        >
        <ul className="mt-3 space-y-4">
          {groupedSources.map((group, idx) => {
            const source = group.primarySource;
            const metaParts = [
              source.category,
              formatSourceMetric(source, idx + 1),
              group.chunks.length > 0
                ? `${group.chunks.length} excerpt${group.chunks.length !== 1 ? 's' : ''}`
                : undefined,
            ].filter(Boolean);

            return (
              <li key={group.key} className="space-y-2">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline gap-2">
                      <span className="font-mono text-xs text-[hsl(var(--text-muted))]">
                        [{idx + 1}]
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
                      const chunkMetaParts = [
                        chunk.section,
                      ].filter(Boolean);

                      return (
                        <div
                          key={chunk.key}
                          className="rounded-sm border border-subtle bg-surface px-4 py-3"
                        >
                          {chunkMetaParts.length > 0 && (
                            <div className="mb-1 text-xs text-[hsl(var(--text-muted))]">
                              {chunkMetaParts.join(' · ')}
                            </div>
                          )}
                          <p className="text-sm text-[hsl(var(--text-secondary))] leading-relaxed line-clamp-4 break-words">
                            {renderHighlightedText(chunk.excerpt, chunk.highlights)}
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
