import { useEffect, useMemo, useRef } from 'react';

import { Columns3 } from 'lucide-react';

import { vaultDocumentIds } from '@/utils/conversationExport';
import { sanitizeFileName } from '@/utils/sanitize';

import { formatSourceLocation } from '../Reading/passageLocator';

import type { ClaimVerdict, SourceWithMetadata } from '../../types/conversation';

interface EvidenceMarginProps {
  /** The answer's cited passages, keyed by the number the text uses for them. */
  citationMap: Map<number, SourceWithMetadata>;
  /** Citation numbers to light up: the chip or sentence under the pointer. */
  activeNumbers: readonly number[];
  claimVerdicts?: readonly ClaimVerdict[];
  provenanceBySource?: Map<string, string>;
  resolvedLocations?: Map<string, string>;
  onOpen: (_number: number) => void;
  /** The note under the pointer, so the answer can light the matching chips. */
  onHoverNumber: (_number: number | null) => void;
  /**
   * Opens Compare on the documents the notes span. Omitted here rather than
   * navigating from the margin itself: this component is rendered inside the
   * answer, and the route belongs to whoever mounts it.
   */
  onCompareDocuments?: (_documentIds: string[]) => void;
}

/** What the checked sentences that cite a passage came to. */
function claimLine(number: number, verdicts: readonly ClaimVerdict[]): { text: string; flagged: boolean } | null {
  const citing = verdicts.filter((verdict) => verdict.citationIds.includes(number));
  if (citing.length === 0) return null;
  const contradicted = citing.filter((verdict) => verdict.verdict === 'contradicted').length;
  const missing = citing.filter((verdict) => verdict.verdict === 'unsupported').length;
  const backed = citing.length - contradicted - missing;
  const parts: string[] = [];
  if (backed > 0) parts.push(`backs ${backed} ${backed === 1 ? 'claim' : 'claims'}`);
  if (contradicted > 0) parts.push(`contradicts ${contradicted}`);
  if (missing > 0) parts.push(`${missing} not found here`);
  return { text: parts.join(' · '), flagged: contradicted + missing > 0 };
}

/**
 * The passages behind an answer, set beside it like the notes of an annotated
 * edition. Shown only where the thread is wide enough to hold a margin; the
 * footnote row and the source list under the answer are the narrow form.
 *
 * Reading an answer and checking it are one act, so the evidence is never a
 * scroll or a click away: it stays in view for the height of the answer, the
 * note for the citation under the pointer lights up, and a note lights its
 * citations in turn.
 */
export function EvidenceMargin({
  citationMap,
  activeNumbers,
  claimVerdicts = [],
  provenanceBySource,
  resolvedLocations,
  onOpen,
  onHoverNumber,
  onCompareDocuments,
}: EvidenceMarginProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const entries = useMemo(
    () => Array.from(citationMap.entries()).sort(([a], [b]) => a - b),
    [citationMap]
  );
  const documentCount = useMemo(
    () => new Set(entries.map(([, source]) => source.documentId)).size,
    [entries]
  );
  // Only files: Compare reads documents, and a web page has no rows to fill.
  const comparableDocumentIds = useMemo(
    () => vaultDocumentIds(entries.map(([, source]) => source)),
    [entries]
  );

  // Bring the lit note into view inside the margin without moving the thread.
  const firstActive = activeNumbers[0];
  useEffect(() => {
    const container = scrollRef.current;
    if (!container || firstActive === undefined) return;
    const note = container.querySelector<HTMLElement>(`[data-note="${firstActive}"]`);
    if (!note) return;
    const top = note.offsetTop;
    const bottom = top + note.offsetHeight;
    if (top < container.scrollTop) container.scrollTop = top - 8;
    else if (bottom > container.scrollTop + container.clientHeight) {
      container.scrollTop = bottom - container.clientHeight + 8;
    }
  }, [firstActive]);

  if (entries.length === 0) return null;

  return (
    <aside className="evidence-margin" aria-label="Evidence for this answer">
      <div ref={scrollRef} className="evidence-margin-notes">
        <p className="mb-2 flex items-baseline justify-between px-2.5 text-xxs uppercase tracking-[0.08em] text-text-muted">
          <span>Evidence</span>
          <span className="normal-case tracking-normal tabular-nums">
            {entries.length} {entries.length === 1 ? 'passage' : 'passages'} · {documentCount}{' '}
            {documentCount === 1 ? 'document' : 'documents'}
          </span>
        </p>
        <ol className="space-y-0.5">
          {entries.map(([number, source]) => {
            const location =
              resolvedLocations?.get(source.chunkId) ?? formatSourceLocation(source);
            const meta = [location, provenanceBySource?.get(source.chunkId)].filter(Boolean).join(' · ');
            const claims = claimLine(number, claimVerdicts);
            const passage = (source.excerpt || source.content || '').trim();
            return (
              <li key={number}>
                <button
                  type="button"
                  data-note={number}
                  data-active={activeNumbers.includes(number) || undefined}
                  onClick={() => onOpen(number)}
                  onMouseEnter={() => onHoverNumber(number)}
                  onMouseLeave={() => onHoverNumber(null)}
                  onFocus={() => onHoverNumber(number)}
                  onBlur={() => onHoverNumber(null)}
                  className="evidence-note"
                  aria-label={`Open passage ${number}, ${source.fileName}`}
                >
                  <span className="flex items-baseline gap-2">
                    <span className="evidence-note-number">{number}</span>
                    <span className="min-w-0 flex-1 truncate text-ui font-medium text-text-primary">
                      {sanitizeFileName(source.fileName)}
                    </span>
                  </span>
                  {meta ? <span className="mt-0.5 block truncate pl-[26px] text-xs text-text-muted">{meta}</span> : null}
                  {passage ? (
                    <span className="mt-1.5 pl-[26px] font-serif text-[13px] leading-[1.55] text-text-secondary line-clamp-3">
                      {passage}
                    </span>
                  ) : null}
                  {claims ? (
                    <span
                      className={`mt-1.5 block pl-[26px] text-[11px] ${
                        claims.flagged ? 'text-[hsl(var(--warning-fg))]' : 'text-text-muted'
                      }`}
                    >
                      {claims.text}
                    </span>
                  ) : null}
                </button>
              </li>
            );
          })}
        </ol>
        {/* An answer drawing on several documents invites the obvious next
            question — where do they differ? — so the margin offers it where
            the reader is already looking at the spread. Inside the scroller,
            not after it: the notes are the sticky element, and a sibling
            would drift away from them as the thread scrolls. */}
        {onCompareDocuments && comparableDocumentIds.length >= 2 ? (
          <button
            type="button"
            onClick={() => onCompareDocuments(comparableDocumentIds)}
            className="mt-2 flex w-full items-center gap-1.5 rounded-md border-t border-border-subtle px-2.5 pb-1 pt-2.5 text-left text-xs text-text-muted transition-colors duration-fast hover:text-text-secondary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Columns3 className="h-3.5 w-3.5 shrink-0" strokeWidth={1.6} aria-hidden="true" />
            Compare these {comparableDocumentIds.length} documents
          </button>
        ) : null}
      </div>
    </aside>
  );
}
