import { type FC } from 'react';

import * as Popover from '@radix-ui/react-popover';

import type { SourceWithMetadata } from '@/types/conversation';
import { sanitizeFileName } from '@/utils/sanitize';

/**
 * CitationFootnote
 *
 * Purpose: Inline superscript citation marker (e.g. [1], [2]) that opens
 * a click-to-pin popover showing source metadata and a preview.
 *
 * Follows CHAT-REDESIGN-SPEC §3.4 — click-to-pin (not hover tooltip),
 * accent marker, serif-first editorial styling.
 */

interface CitationFootnoteProps {
  number: number;
  source: SourceWithMetadata;
  onViewFile: () => void;
}

export const CitationFootnote: FC<CitationFootnoteProps> = ({
  number,
  source,
  onViewFile,
}) => {
  const sanitizedFileName = sanitizeFileName(source.fileName);

  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <sup
          role="button"
          tabIndex={0}
          aria-label={`Citation ${number}: ${sanitizedFileName}`}
          className="inline-block cursor-pointer px-0.5 font-mono text-xs text-[hsl(var(--accent))] transition-colors duration-fast hover:text-[hsl(var(--accent-hover))] hover:underline"
        >
          [{number}]
        </sup>
      </Popover.Trigger>

      <Popover.Portal>
        <Popover.Content
          sideOffset={6}
          side="top"
          align="start"
          className="z-50 max-w-xs rounded-md border border-subtle bg-surface-raised p-3 text-[hsl(var(--text-primary))] shadow-md outline-none data-[state=open]:animate-in data-[state=open]:duration-base data-[state=open]:ease-out data-[state=closed]:animate-out data-[state=closed]:duration-fast data-[state=closed]:ease-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95"
        >
          <div className="space-y-2">
            <p className="text-sm font-semibold text-[hsl(var(--text-primary))] break-words">
              {sanitizedFileName}
            </p>
            {source.category && (
              <p className="text-xs text-[hsl(var(--text-muted))]">
                {source.category}
              </p>
            )}
            <p className="text-xs text-[hsl(var(--text-secondary))] line-clamp-3 break-words">
              {source.excerpt ?? source.content}
            </p>
            <button
              type="button"
              onClick={onViewFile}
              className="block pt-1 text-xs text-[hsl(var(--accent))] underline-offset-2 hover:underline"
            >
              View source
            </button>
          </div>
          <Popover.Arrow className="fill-[hsl(var(--surface-raised))]" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
};
