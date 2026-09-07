import { type ReactNode } from 'react';

import { cn } from '@/lib/utils';

/**
 * PageHeader
 *
 * The only page title component. Reading-column routes (Home, Search,
 * Import, Settings panes, Library) render exactly one of these at the top.
 *
 * - `title` is a noun. Serif, one line.
 * - `meta` is a single line of data ("1,247 documents · 2 folders"), never
 *   a sentence explaining the page.
 * - `actions` is an optional right-aligned cluster of small ghost or
 *   secondary buttons.
 *
 * See `.design/UX-OVERHAUL-BRIEF.md` §2.
 */
interface PageHeaderProps {
  title: string;
  meta?: ReactNode;
  actions?: ReactNode;
  className?: string;
}

export function PageHeader({ title, meta, actions, className }: PageHeaderProps) {
  return (
    <header className={cn('mb-10 flex items-end justify-between gap-6', className)}>
      <div className="min-w-0">
        <h1 className="font-serif text-3xl font-semibold tracking-[-0.02em] text-text-primary">
          {title}
        </h1>
        {meta ? (
          <p className="mt-1 text-sm text-text-tertiary tabular-nums">{meta}</p>
        ) : null}
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
    </header>
  );
}

/**
 * SectionHeading — the section title used under a PageHeader. Matches the
 * Home screen. Follow it with a `border-t border-border-subtle` container
 * whose rows carry `border-b border-border-subtle`.
 */
export function SectionHeading({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <h2 className={cn('pb-3 text-lg font-medium text-text-secondary', className)}>{children}</h2>
  );
}
