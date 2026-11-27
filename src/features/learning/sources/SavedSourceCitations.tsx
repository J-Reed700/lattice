import { useMemo } from 'react';

import { useSavedSourceReader, type SavedSourceTarget } from '@/features/learning/sources/useSavedSourceReader';
import { CitationFootnote } from '@/features/reading/components/CitationFootnote';
import { CitationSourcesRow } from '@/features/reading/components/CitationSourcesRow';
import { useReaderClosesWithOwner } from '@/features/reading/hooks/useReaderClosesWithOwner';
import { visibleText } from '@/features/reading/model/inlineMarkdown';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { SourceWithMetadata } from '@/types/conversation';

function validCitationNumber(value?: number | null): number | undefined {
  return typeof value === 'number' && Number.isInteger(value) && value > 0 ? value : undefined;
}

export interface SavedSourceCitation extends Omit<SavedSourceTarget, 'number'> {
  /** Backend-assigned citation number for this citation (1-based), when present. */
  number?: number | null;
  /** The source's title when the caller already has it; else the course library's. */
  title?: string | null;
}

/**
 * Who opens the reader: the list itself, which then closes it on unmount, or the
 * surface that owns it, which opens it and closes it. Exactly one of the two.
 */
type SavedSourceOpener =
  | {
      /** The text these citations belong to; the reader closes when it leaves the screen. */
      ownerKey: string;
      onOpen?: never;
    }
  | {
      /** Replaces opening the reader, for surfaces that record the opening first and own the reader. */
      onOpen: (citation: SavedSourceCitation, index: number) => void;
      ownerKey?: never;
    };

/** The words a quote shows: it is Markdown from the saved text, and a reader sees it without the syntax (unless it is all syntax). */
function cleanQuoteText(raw?: string | null): string {
  const trimmed = raw?.trim() ?? '';
  return visibleText(trimmed) || trimmed;
}

function shownQuote(citation: SavedSourceCitation): string {
  return cleanQuoteText(citation.quote);
}

/** What a citation's chip shows before its source is opened: the title, support relation, and quoted words. */
function chipSource(citation: SavedSourceCitation, title: string): SourceWithMetadata {
  const quote = shownQuote(citation);
  const support = citation.support;
  const citationNumber = validCitationNumber(citation.number);
  const anchoredBy = support
    ? [
        {
          occurrence: 0,
          relation: support.relation,
          anchors: support.anchors.map((anchor) => ({
            ...anchor,
            exact: cleanQuoteText(anchor.exact),
            span: null,
          })),
        },
      ]
    : undefined;
  return {
    documentId: `course:${citation.versionId}`,
    chunkId: citationNumber !== undefined ? `${citation.versionId}#cite-${citationNumber}` : citation.versionId,
    fileName: title,
    filePath: '',
    mimeType: 'text/html',
    category: 'Saved source',
    content: quote,
    excerpt: quote || undefined,
    score: 1,
    fileSizeBytes: 0,
    modifiedAt: new Date(0).toISOString(),
    ...(citationNumber !== undefined ? { citationId: citationNumber } : {}),
    anchoredBy,
    anchorBase: 0,
  };
}

/**
 * The saved sources a piece of Learning Studio text cites, drawn the way a chat
 * answer and a lesson draw theirs: a numbered chip for each, its popover naming
 * the source, and "View source" opening the docked reader on the quoted words.
 *
 * `quotes` lists each quote beside its number, for feedback and rationales
 * where the quoted words are the point; without it the chips sit in one
 * "Sources" row, as under a lesson section. Reading mode hides the chips, and
 * closes the reader they opened, as it does in chat and lessons.
 */
export function SavedSourceCitations({
  programId,
  citations,
  quotes = false,
  label = 'Sources',
  disabledReason,
  className = 'mt-3',
  ...opener
}: {
  programId: string;
  citations: readonly SavedSourceCitation[];
  quotes?: boolean;
  label?: string;
  disabledReason?: string;
  className?: string;
} & SavedSourceOpener) {
  const reader = useSavedSourceReader(programId);
  const citationsVisible = useCitationDisplayStore((state) => state.visible);
  const ownerKey = opener.ownerKey ?? null;
  const onOpen = opener.onOpen ?? null;
  useReaderClosesWithOwner(ownerKey);
  const { titleOf } = reader;
  const chips = useMemo(
    () => citations.map((citation) => chipSource(citation, citation.title?.trim() || titleOf(citation.versionId) || 'Saved source')),
    [citations, titleOf],
  );
  const readerTargets = useMemo<SavedSourceTarget[]>(
    () => citations.map((citation) => ({ ...citation, number: validCitationNumber(citation.number) })),
    [citations],
  );
  if (citations.length === 0) return null;
  if (!citationsVisible) return null;

  const view = (index: number) => {
    if (onOpen) onOpen(citations[index]!, index);
    else if (ownerKey !== null) void reader.open(ownerKey, readerTargets, { index });
  };
  const unnumberedAction = (index: number) =>
    disabledReason ? (
      <span className="text-text-muted">{disabledReason}</span>
    ) : (
      <button
        type="button"
        onClick={() => view(index)}
        className="font-medium text-accent underline-offset-2 hover:underline"
      >
        View source
      </button>
    );

  if (!quotes) {
    const numberedEntries = citations.flatMap((citation, index) => {
      const citationNumber = validCitationNumber(citation.number);
      if (citationNumber === undefined) return [];
      return [{
        key: `${citation.versionId}-${citationNumber}-${index}`,
        number: citationNumber,
        source: chips[index]!,
        onView: () => view(index),
        disabledReason,
        title: chips[index]!.fileName,
      }];
    });
    const unnumberedIndices = citations.flatMap((citation, index) =>
      validCitationNumber(citation.number) === undefined ? [index] : [],
    );

    if (unnumberedIndices.length === 0) {
      return (
        <CitationSourcesRow
          label={label}
          className={className}
          entries={numberedEntries}
        />
      );
    }

    return (
      <div className={`${className} space-y-1.5 text-xs`}>
        {numberedEntries.length > 0 && (
          <CitationSourcesRow label={label} className="" entries={numberedEntries} />
        )}
        <div className="flex flex-wrap items-center gap-2">
          {numberedEntries.length === 0 && <span className="text-xxs text-text-muted">{label}</span>}
          {unnumberedIndices.map((index) => (
            <span key={`${citations[index]!.versionId}-unnumbered-${index}`} className="inline-flex items-center gap-1.5">
              <span className="max-w-[18rem] truncate text-text-secondary">{chips[index]!.fileName}</span>
              {unnumberedAction(index)}
            </span>
          ))}
        </div>
      </div>
    );
  }

  return (
    <ol aria-label={label} className={`${className} space-y-2`}>
      {citations.map((citation, index) => {
        const quote = shownQuote(citation);
        const isVerbatimQuote = citation.support?.relation === 'quotes';
        const citationNumber = validCitationNumber(citation.number);
        return (
          <li key={`${citation.versionId}-${citationNumber ?? 'unnumbered'}-${index}`} className="flex items-start gap-2 text-xs leading-5">
            <span className="mt-px shrink-0">
              {citationNumber !== undefined ? (
                <CitationFootnote
                  number={citationNumber}
                  source={chips[index]!}
                  onViewFile={() => view(index)}
                  disabledReason={disabledReason}
                />
              ) : (
                unnumberedAction(index)
              )}
            </span>
            <span className="min-w-0">
              {quote && (isVerbatimQuote
                ? <q className="italic text-text-secondary">{quote}</q>
                : <span className="text-text-secondary">{quote}</span>)}
              <span className="text-text-muted">{quote ? ' — ' : ''}{chips[index]!.fileName}</span>
            </span>
          </li>
        );
      })}
    </ol>
  );
}
