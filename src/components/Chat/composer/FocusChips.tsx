import { FileText, X } from 'lucide-react';

import type { SpaceDocument } from '../../../types';

/**
 * The documents this chat is pinned to, above the textarea.
 *
 * Focus narrows; it never widens. The backend intersects these ids with the
 * conversation's space before anything reads them, and the chips are here so
 * the narrowing is never a surprise at the bottom of an answer.
 */

export interface FocusChipsProps {
  documents: SpaceDocument[];
  onRemove: (_documentId: string) => void;
  onClear: () => void;
}

/** "Asking 2 documents in Heat Island Thesis" — the placeholder and the space label share it. */
export function describeFocus(count: number, spaceName: string | null): string {
  const documents = `${count} document${count === 1 ? '' : 's'}`;
  return spaceName ? `Asking ${documents} in ${spaceName}` : `Asking ${documents}`;
}

export function FocusChips({ documents, onRemove, onClear }: FocusChipsProps) {
  if (documents.length === 0) return null;

  return (
    <div className="flex flex-wrap items-center gap-1.5 px-4 pt-3" aria-label="Documents this chat is asking">
      <span className="text-xs font-medium text-[hsl(var(--text-secondary))]">Only</span>
      {documents.map((document) => (
        <span
          key={document.documentId}
          className="inline-flex h-6 min-w-0 max-w-[260px] items-center gap-1 rounded-md bg-[hsl(var(--accent)/0.1)] pl-1.5 pr-1 text-xs text-[hsl(var(--text-secondary))]"
        >
          <FileText className="h-3 w-3 shrink-0 opacity-70" strokeWidth={1.75} aria-hidden="true" />
          <span className="truncate" title={document.fileName}>
            {document.fileName}
          </span>
          <button
            type="button"
            onClick={() => onRemove(document.documentId)}
            aria-label={`Stop asking only ${document.fileName}`}
            className="pressable inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.08)] hover:text-[hsl(var(--text-primary))]"
          >
            <X className="h-3 w-3" strokeWidth={2} />
          </button>
        </span>
      ))}
      {documents.length > 1 && (
        <button
          type="button"
          onClick={onClear}
          className="rounded-sm px-1 text-xs text-[hsl(var(--text-muted))] underline-offset-2 transition-colors duration-fast hover:text-[hsl(var(--text-secondary))] hover:underline"
        >
          Ask the whole space
        </button>
      )}
    </div>
  );
}
