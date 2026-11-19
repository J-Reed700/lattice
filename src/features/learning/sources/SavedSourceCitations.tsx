import { useMemo } from 'react';

import { useSavedSourceReader, type SavedSourceTarget } from '@/features/learning/sources/useSavedSourceReader';
import { CitationFootnote } from '@/features/reading/components/CitationFootnote';
import { CitationSourcesRow } from '@/features/reading/components/CitationSourcesRow';
import { useReaderClosesWithOwner } from '@/features/reading/hooks/useReaderClosesWithOwner';
import { visibleText } from '@/features/reading/model/inlineMarkdown';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { SourceWithMetadata } from '@/types/conversation';

export interface SavedSourceCitation extends SavedSourceTarget {
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
function shownQuote(citation: SavedSourceCitation): string {
  const raw = citation.quote?.trim() ?? '';
  return visibleText(raw) || raw;
}

/** What a citation's chip shows before its source is opened: the title and the quoted words. */
function chipSource(citation: SavedSourceCitation, title: string, index: number): SourceWithMetadata {
  const quote = shownQuote(citation);
  return {
    documentId: `course:${citation.versionId}`,
    chunkId: `${citation.versionId}#chip-${index}`,
    fileName: title,
    filePath: '',
    mimeType: 'text/html',
    category: 'Saved source',
    content: quote,
    excerpt: quote || undefined,
    score: 1,
    fileSizeBytes: 0,
    modifiedAt: new Date(0).toISOString(),
    citationId: index + 1,
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
  citations: SavedSourceCitation[];
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
    () => citations.map((citation, index) => chipSource(citation, citation.title?.trim() || titleOf(citation.versionId) || 'Saved source', index)),
    [citations, titleOf],
  );
  if (citations.length === 0) return null;
  if (!citationsVisible) return null;

  const view = (index: number) => {
    if (onOpen) onOpen(citations[index]!, index);
    else if (ownerKey !== null) void reader.open(ownerKey, citations, { index });
  };
  const chip = (index: number) => (
    <CitationFootnote
      number={index + 1}
      source={chips[index]!}
      onViewFile={() => view(index)}
      disabledReason={disabledReason}
    />
  );

  if (!quotes) {
    return (
      <CitationSourcesRow
        label={label}
        className={className}
        entries={citations.map((citation, index) => ({
          key: `${citation.versionId}-${index}`,
          number: index + 1,
          source: chips[index]!,
          onView: () => view(index),
          disabledReason,
          title: chips[index]!.fileName,
        }))}
      />
    );
  }

  return (
    <ol aria-label={label} className={`${className} space-y-2`}>
      {citations.map((citation, index) => {
        const quote = shownQuote(citation);
        return (
          <li key={`${citation.versionId}-${index}`} className="flex items-start gap-2 text-xs leading-5">
            <span className="mt-px shrink-0">{chip(index)}</span>
            <span className="min-w-0">
              {quote && <q className="italic text-text-secondary">{quote}</q>}
              <span className="text-text-muted">{quote ? ' — ' : ''}{chips[index]!.fileName}</span>
            </span>
          </li>
        );
      })}
    </ol>
  );
}
