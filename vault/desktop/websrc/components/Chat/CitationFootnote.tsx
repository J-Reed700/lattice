import { type FC } from 'react';

import * as Tooltip from '@radix-ui/react-tooltip';

import type { SourceWithMetadata } from '@/types/conversation';
import { sanitizeFileName } from '@/utils/sanitize';

/**
 * CitationFootnote
 *
 * Purpose: Display inline citation with hover tooltip and click-to-preview
 *
 * Features:
 * - Clickable superscript citation number [1], [2], etc.
 * - Tooltip shows file metadata and content preview
 * - Accessible with keyboard navigation
 * - Smooth animations
 *
 * Accessibility: WCAG AA, keyboard nav, ARIA labels, semantic HTML
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
    <Tooltip.Provider delayDuration={300}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <sup
            onClick={onViewFile}
            className="
              cursor-pointer
              text-[var(--accent-primary)] hover:text-[var(--accent-hover)]
              dark:text-[var(--accent-light)] dark:hover:text-[var(--accent-primary)]
              font-semibold
              transition-colors
              px-0.5
              underline decoration-dotted
              inline-block
            "
            role="button"
            tabIndex={0}
            aria-label={`Citation ${number}: ${sanitizedFileName}`}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onViewFile();
              }
            }}
          >
            [{number}]
          </sup>
        </Tooltip.Trigger>

        <Tooltip.Portal>
          <Tooltip.Content
            className="
              max-w-xs p-3
              bg-[var(--surface-primary)] dark:bg-[var(--surface-elevated)]
              border border-[var(--border-color)] dark:border-[var(--border-hover)]
              rounded-lg shadow-lg
              z-50
              animate-in fade-in-0 zoom-in-95
              data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95
            "
            sideOffset={5}
            side="top"
          >
            <div className="space-y-2">
              <p className="font-semibold text-sm text-[var(--text-primary)] dark:text-[var(--text-primary)] break-words">
                {sanitizedFileName}
              </p>
              <p className="text-xs text-[var(--text-tertiary)] dark:text-[var(--text-tertiary)]">
                {source.category}
              </p>
              <p className="text-xs text-[var(--text-primary)] dark:text-[var(--text-secondary)] line-clamp-3 break-words">
                {source.excerpt ?? source.content}
              </p>
              <p className="text-xs text-[var(--accent-primary)] dark:text-[var(--accent-light)] font-medium pt-1 border-t border-[var(--border-color)] dark:border-[var(--border-hover)]">
                Click to view source
              </p>
            </div>
            <Tooltip.Arrow className="fill-[var(--surface-primary)] dark:fill-[var(--surface-elevated)]" />
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
};
