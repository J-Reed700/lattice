import { useMemo } from 'react';

import { BookOpen, FileText, Globe, MessageSquare } from 'lucide-react';

import { Button } from '@/components/ui';
import type { SearchResult as SearchResultType } from '@/types';

import {
  getQueryTerms,
  getSearchResultDisplayMeta,
  RelevanceMeter,
  renderHighlightedExcerpt,
} from '../SearchResult/SearchResult';

interface SearchPreviewPaneProps {
  result: SearchResultType;
  query: string;
  onOpen: (result: SearchResultType) => void;
  onAskInChat?: (result: SearchResultType) => void;
}

export function SearchPreviewPane({
  result,
  query,
  onOpen,
  onAskInChat,
}: SearchPreviewPaneProps) {
  const meta = useMemo(() => getSearchResultDisplayMeta(result), [result]);
  const queryTerms = useMemo(() => getQueryTerms(query), [query]);

  const excerpts = useMemo(() => {
    const raw =
      result.highlights && result.highlights.length > 0
        ? result.highlights
        : result.content
          ? [result.content]
          : [];
    return raw.map((item) => item.trim()).filter(Boolean);
  }, [result.content, result.highlights]);

  const isWeb = Boolean(meta.rawPath && /^https?:\/\//i.test(meta.rawPath));
  const canAskInChat = Boolean(onAskInChat && (result.documentId || result.id));

  return (
    <aside
      aria-label="Quick preview"
      className="sticky top-6 rounded-xl border border-border-subtle bg-surface p-5 shadow-control"
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5 text-xs text-text-muted">
          <span className="inline-flex shrink-0 items-center gap-1 rounded-md bg-[hsl(var(--text-primary)/0.06)] px-2 py-0.5 text-[11px] font-medium text-text-secondary">
            {isWeb ? (
              <Globe className="h-3 w-3" strokeWidth={1.75} aria-hidden="true" />
            ) : (
              <FileText className="h-3 w-3" strokeWidth={1.75} aria-hidden="true" />
            )}
            {meta.typeLabel}
          </span>
          {meta.location && (
            <span className="truncate" title={meta.rawPath}>
              {meta.location}
            </span>
          )}
        </div>
        <RelevanceMeter
          score={meta.relevance}
          formattedScore={meta.formattedScore}
          components={meta.scoreComponents}
        />
      </div>

      <h2 className="mt-3 font-serif text-[19px] font-normal leading-snug tracking-tight text-text-primary">
        {meta.displayTitle}
      </h2>

      {meta.displayPath && (
        <p
          className="mt-1 truncate text-xs text-text-muted"
          title={meta.rawPath}
        >
          {meta.displayPath}
        </p>
      )}

      <div className="mt-4 border-t border-border-subtle pt-4">
        <div className="mb-2.5 flex items-center justify-between">
          <span className="text-[11px] font-medium tracking-[0.04em] uppercase text-text-tertiary">
            Matched passages
          </span>
          <span className="text-[11px] tabular-nums text-text-muted">
            {excerpts.length} {excerpts.length === 1 ? 'excerpt' : 'excerpts'}
          </span>
        </div>

        {excerpts.length > 0 ? (
          <div className="max-h-[420px] space-y-3 overflow-y-auto pr-1">
            {excerpts.map((excerpt, index) => (
              <blockquote
                key={`${result.id}-preview-${index}`}
                className="border-l-2 border-accent/45 pl-3.5 font-serif text-[14.5px] leading-relaxed text-text-secondary"
              >
                {renderHighlightedExcerpt(excerpt, queryTerms)}
              </blockquote>
            ))}
          </div>
        ) : (
          <p className="text-sm text-text-muted">No passage excerpt available for this match.</p>
        )}
      </div>

      <div className="mt-5 flex flex-wrap items-center gap-2 border-t border-border-subtle pt-4">
        <Button onClick={() => onOpen(result)} className="flex-1">
          <BookOpen className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
          Open full reader
        </Button>
        {canAskInChat && (
          <Button
            variant="outline"
            onClick={() => onAskInChat?.(result)}
            className="flex-1"
          >
            <MessageSquare className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
            Ask in Chat
          </Button>
        )}
      </div>
    </aside>
  );
}
