import { type FC, useRef } from 'react';

import * as Popover from '@radix-ui/react-popover';

import { formatSourceLocation } from '@/shared/sources/passageLocator';
import type { SourceWithMetadata } from '@/types/conversation';
import { sanitizeFileName } from '@/utils/sanitize';


/**
 * CitationFootnote
 *
 * Purpose: Inline superscript citation marker (e.g. [1], [2]) that opens
 * a click-to-pin popover showing source metadata and a preview.
 *
 * Click to pin the citation rather than showing it in a hover tooltip;
 * accent marker, serif-first editorial styling.
 */

interface CitationFootnoteProps {
  number: number;
  source: SourceWithMetadata;
  /** "from your journal" / "from your references", when earned. */
  provenanceLabel?: string;
  /** A location a viewer has already resolved, e.g. "p. 12". */
  resolvedLocation?: string;
  onViewFile: () => void;
  /** Why the source cannot be opened right now; the citation still shows what it cites. */
  disabledReason?: string;
}

export const CitationFootnote: FC<CitationFootnoteProps> = ({
  number,
  source,
  provenanceLabel,
  resolvedLocation,
  onViewFile,
  disabledReason,
}) => {
  const sanitizedFileName = sanitizeFileName(source.fileName);
  // Only what is known: a resolved page, a timestamp, or a heading.
  const location = resolvedLocation ?? formatSourceLocation(source);
  const triggerRef = useRef<HTMLButtonElement>(null);
  // Set when "View source" hands focus to the reader. On a narrow screen the
  // reader is a non-modal dialog that closes when focus moves outside it, so the
  // popover must not take focus back to the chip when it unmounts.
  const handedToReaderRef = useRef(false);

  return (
    <Popover.Root
      onOpenChange={(open) => {
        if (open) handedToReaderRef.current = false;
      }}
    >
      <Popover.Trigger asChild>
        <button
          ref={triggerRef}
          type="button"
          aria-label={`Citation ${number}: ${sanitizedFileName}`}
          className="inline-flex h-[18px] min-w-[18px] cursor-pointer items-center justify-center rounded-sm border border-border-subtle bg-surface px-1 font-mono text-xxs tabular-nums text-text-secondary transition-colors duration-fast hover:border-accent hover:text-accent focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          {number}
        </button>
      </Popover.Trigger>

      <Popover.Portal>
        <Popover.Content
          sideOffset={6}
          side="top"
          align="start"
          onCloseAutoFocus={(event) => {
            if (!handedToReaderRef.current) return;
            handedToReaderRef.current = false;
            event.preventDefault();
          }}
          className="z-50 max-w-xs rounded-md border border-subtle bg-surface-raised p-3 text-[hsl(var(--text-primary))] shadow-md outline-hidden data-[state=open]:animate-in data-[state=open]:duration-base data-[state=open]:ease-out data-[state=closed]:animate-out data-[state=closed]:duration-fast data-[state=closed]:ease-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95"
        >
          <div className="space-y-2">
            <p className="text-sm font-semibold text-[hsl(var(--text-primary))] wrap-break-word">
              {sanitizedFileName}
            </p>
            {source.category && (
              <p className="text-xs text-[hsl(var(--text-muted))]">
                {source.category}
              </p>
            )}
            {location && (
              <p className="text-xs text-[hsl(var(--text-muted))]">{location}</p>
            )}
            {provenanceLabel && (
              <p className="text-xs text-[hsl(var(--text-muted))]">{provenanceLabel}</p>
            )}
            <p className="text-xs text-[hsl(var(--text-secondary))] line-clamp-3 wrap-break-word">
              {source.excerpt ?? source.content}
            </p>
            {disabledReason ? (
              <p className="pt-1 text-xs text-[hsl(var(--text-muted))]">{disabledReason}</p>
            ) : (
              <Popover.Close asChild>
                <button
                  type="button"
                  onClick={() => {
                    // Focus waits on the chip, not on this button, which is about to
                    // go: the reader returns focus to whatever had it when it opened.
                    triggerRef.current?.focus({ preventScroll: true });
                    handedToReaderRef.current = true;
                    onViewFile();
                  }}
                  className="block pt-1 text-xs text-[hsl(var(--accent))] underline-offset-2 hover:underline"
                  title={`Open ${sanitizedFileName}`}
                >
                  View source
                </button>
              </Popover.Close>
            )}
          </div>
          <Popover.Arrow className="fill-[hsl(var(--surface-raised))]" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
};
