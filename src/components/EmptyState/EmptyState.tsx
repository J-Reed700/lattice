import { type ReactNode } from 'react';

import { cn } from '@/lib/utils';

/**
 * EmptyState
 *
 * One sentence, one way forward. The glyph sits in a quiet tile, the title is
 * set in the reading face, and the single action (with its shortcut, if it has
 * one) is the only thing on the page that looks pressable.
 */
export interface EmptyStateProps {
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: {
    label: string;
    onClick: () => void;
    shortcut?: string;
  };
  className?: string;
}

export const EmptyState = ({ icon, title, description, action, className }: EmptyStateProps) => (
  <div className={cn('flex flex-col items-center justify-center px-6 py-16 text-center', className)}>
    {icon ? (
      <div className="mb-4 flex h-11 w-11 items-center justify-center rounded-xl bg-[hsl(var(--text-primary)/0.05)] text-text-tertiary shadow-[inset_0_0_0_1px_hsl(var(--text-primary)/0.05)] [&>svg]:h-5 [&>svg]:w-5">
        {icon}
      </div>
    ) : null}
    <p className="font-serif text-[19px] font-medium tracking-[-0.015em] text-text-primary">{title}</p>
    {description ? <p className="mt-1.5 max-w-sm text-ui leading-relaxed text-text-muted">{description}</p> : null}
    {action ? (
      <button
        type="button"
        onClick={action.onClick}
        className="pressable mt-5 inline-flex h-8 items-center gap-2 rounded-md border border-border-default bg-surface px-3 text-ui font-medium text-text-primary shadow-control transition-[background-color,border-color,scale] duration-fast hover:border-border-strong hover:bg-surface-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {action.label}
        {action.shortcut ? <kbd className="kbd">{action.shortcut}</kbd> : null}
      </button>
    ) : null}
  </div>
);
