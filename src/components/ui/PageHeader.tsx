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
 */
interface PageHeaderProps {
  title: string;
  meta?: ReactNode;
  actions?: ReactNode;
  className?: string;
}

export function PageHeader({ title, meta, actions, className }: PageHeaderProps) {
  return (
    <header className={cn('mb-6 flex flex-wrap items-end justify-between gap-4', className)}>
      <div className="min-w-0">
        <h1 className="font-serif text-[28px] font-normal leading-[1.15] tracking-[-0.025em] text-text-primary">
          {title}
        </h1>
        {meta ? (
          <p className="mt-1.5 text-ui text-text-muted tabular-nums">{meta}</p>
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
    <h2 className={cn('pb-2.5 text-[15px] font-medium tracking-[-0.005em] text-text-primary', className)}>{children}</h2>
  );
}
