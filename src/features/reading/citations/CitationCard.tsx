import type { ReactNode } from 'react';

import { FileText } from 'lucide-react';

import '@/features/reading/components/reading.css';
import type { RelationDto } from '@/lib/bindings';

import { citationAccessibleName, relationLabel } from './relation';

export interface CitationCardProps {
  number: number;
  title: string;
  site?: string | null;
  relation?: RelationDto | null;
  /** The cited sentences, in the order they stand in the source. */
  excerpts: readonly string[];
  /** The saved passage, shown clamped when there are no excerpts. */
  passage?: string | null;
  /** The headings above the cited text, outermost first. */
  trail?: readonly string[];
  footer?: ReactNode;
  onOpen?: () => void;
}

const EXCERPT_CLASS =
  'cite-card-excerpt font-serif text-[13.5px] leading-relaxed text-text-secondary';

/** The one card that says what a citation is: how it is used, where it is from, and the sentences behind it. */
export function CitationCard({
  number,
  title,
  site,
  relation,
  excerpts,
  passage,
  trail,
  footer,
  onOpen,
}: CitationCardProps) {
  const quoted = relation === 'quotes';
  const shownPassage = passage?.trim() ?? '';

  return (
    <div role="group" aria-label={citationAccessibleName(number, title, relation)}>
      {relation ? (
        <p
          className="mb-1.5 text-[11px] font-semibold tracking-wide text-text-muted"
          style={{ fontVariantCaps: 'all-small-caps' }}
        >
          {relationLabel(relation)}
        </p>
      ) : null}
      <div className="flex items-start gap-2.5">
        <span className="mt-px flex h-5 min-w-5 items-center justify-center rounded-[5px] bg-accent-muted px-1 text-[11px] font-semibold tabular-nums text-accent">
          {number}
        </span>
        <p className="flex min-w-0 flex-1 items-center gap-1.5 truncate text-ui font-medium text-text-primary">
          <FileText className="h-3.5 w-3.5 shrink-0 text-text-tertiary" strokeWidth={1.6} />
          <span className="truncate">{title}</span>
          {site ? <span className="shrink-0 font-normal text-text-muted"> · {site}</span> : null}
        </p>
      </div>
      {excerpts.length > 0 ? (
        <div className="mt-2.5 space-y-1">
          {excerpts.map((excerpt, index) => (
            <div key={`${index}:${excerpt}`}>
              {index > 0 ? (
                <p className="mb-1 pl-3 text-text-muted" aria-hidden="true">
                  …
                </p>
              ) : null}
              <blockquote className={EXCERPT_CLASS}>{quoted ? `“${excerpt}”` : excerpt}</blockquote>
            </div>
          ))}
        </div>
      ) : shownPassage ? (
        <p className={`mt-2.5 line-clamp-5 ${EXCERPT_CLASS}`}>{shownPassage}</p>
      ) : null}
      {trail && trail.length > 0 ? <p className="mt-2 text-xs text-text-muted">{trail.join(' › ')}</p> : null}
      {footer}
      {onOpen ? (
        <button
          type="button"
          onClick={onOpen}
          className="mt-2.5 text-xs font-medium text-accent transition-colors duration-fast hover:text-[hsl(var(--accent-hover))]"
        >
          Open source
        </button>
      ) : null}
    </div>
  );
}
