import { type ReactNode } from 'react';

import { cn } from '@/lib/utils';

/**
 * SidebarHeader
 *
 * The 48px header row shared by every sidebar (Chat, Journal, References,
 * Library rail). Title on the left, icon buttons on the right.
 */
interface SidebarHeaderProps {
  title: string;
  /** IconButtons, right-aligned. */
  actions?: ReactNode;
  className?: string;
}

export function SidebarHeader({ title, actions, className }: SidebarHeaderProps) {
  return (
    <div
      className={cn(
        'flex h-12 shrink-0 items-center justify-between px-4',
        className,
      )}
    >
      <h2 className="truncate text-ui font-semibold tracking-[-0.01em] text-text-primary">{title}</h2>
      {actions ? <div className="flex shrink-0 items-center gap-0.5">{actions}</div> : null}
    </div>
  );
}

/**
 * SidebarSearch — the 32px search field that sits under a SidebarHeader.
 */
interface SidebarSearchProps {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  className?: string;
}

export function SidebarSearch({ value, onChange, placeholder, className }: SidebarSearchProps) {
  return (
    <input
      type="search"
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder={placeholder}
      aria-label={placeholder}
      className={cn(
        'h-8 w-full rounded-md border border-transparent bg-[hsl(var(--text-primary)/0.055)] px-2.5 text-ui text-text-primary placeholder:text-text-muted outline-none transition-[background-color,border-color,box-shadow] duration-fast hover:bg-[hsl(var(--text-primary)/0.08)] focus:border-accent/60 focus:bg-surface focus:shadow-[0_0_0_3px_hsl(var(--accent)/0.14)]',
        className,
      )}
    />
  );
}

/**
 * SidebarTabs — one row of text filter tabs. Max five. Underline marks the
 * active tab.
 */
interface SidebarTabsProps<T extends string> {
  value: T;
  onChange: (value: T) => void;
  options: ReadonlyArray<{ id: T; label: string }>;
  className?: string;
}

export function SidebarTabs<T extends string>({ value, onChange, options, className }: SidebarTabsProps<T>) {
  return (
    <div role="tablist" className={cn('flex items-center gap-3 text-xs', className)}>
      {options.map((option) => {
        const isActive = option.id === value;
        return (
          <button
            key={option.id}
            type="button"
            role="tab"
            aria-selected={isActive}
            onClick={() => onChange(option.id)}
            className={cn(
              '-mb-px border-b-[1.5px] pb-1.5 font-medium transition-colors duration-fast',
              isActive
                ? 'border-accent text-text-primary'
                : 'border-transparent text-text-tertiary hover:text-text-primary',
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
