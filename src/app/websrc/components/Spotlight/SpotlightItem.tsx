import { Command } from 'cmdk';
import { motion, useReducedMotion } from 'framer-motion';
import { type LucideIcon } from 'lucide-react';

export interface SpotlightItemProps {
  /**
   * Stable value passed to cmdk. Also used as the node's key upstream.
   * cmdk uses this for its internal match tracking when shouldFilter is
   * enabled — we pass it as-is so the palette stays deterministic.
   */
  value: string;
  icon: LucideIcon;
  label: string;
  secondary?: string;
  shortcut?: string;
  trailing?: React.ReactNode;
  onSelect: () => void;
}

/**
 * SpotlightItem
 *
 * One result row in the unified Spotlight. Renders an icon, a primary
 * label, optional secondary text, and an optional trailing element
 * (shortcut kbd or custom chip).
 *
 * Selection visual: a 2px accent bar inset vertically so it reads as a
 * highlight rather than a divider, plus a subtle surface fill. Motion
 * uses framer-motion `layoutId="spotlight-selection"` so the bar slides
 * between rows; reduced-motion callers get a static bar.
 *
 * This component replaces the old `CommandItem` from
 * `components/CommandItem` — it's the same idea but token-based and
 * rich enough to carry secondary text and trailing slots.
 */
export function SpotlightItem({
  value,
  icon: Icon,
  label,
  secondary,
  shortcut,
  trailing,
  onSelect,
}: SpotlightItemProps) {
  const prefersReducedMotion = useReducedMotion();

  return (
    <Command.Item
      value={value}
      onSelect={onSelect}
      className="group relative flex w-full cursor-pointer select-none items-center gap-3 rounded-sm px-4 py-2.5 text-left transition-colors duration-fast aria-selected:bg-[hsl(var(--surface))]"
    >
      <span
        className="pointer-events-none absolute left-0 top-2 bottom-2 w-0.5 bg-[hsl(var(--accent))] opacity-0 transition-opacity duration-fast group-aria-selected:opacity-100"
        aria-hidden="true"
      >
        {!prefersReducedMotion && (
          <motion.span
            layoutId="spotlight-selection"
            className="absolute inset-0 bg-[hsl(var(--accent))]"
            transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          />
        )}
      </span>

      <Icon
        className="h-4 w-4 shrink-0 text-[hsl(var(--text-tertiary))] group-aria-selected:text-[hsl(var(--text-secondary))]"
        strokeWidth={1.75}
        aria-hidden="true"
      />

      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-[hsl(var(--text-primary))]">
          {label}
        </div>
        {secondary && (
          <div className="mt-0.5 truncate text-xs text-[hsl(var(--text-muted))]">
            {secondary}
          </div>
        )}
      </div>

      {trailing && (
        <div className="ml-2 flex shrink-0 items-center gap-1 text-[hsl(var(--text-muted))]">
          {trailing}
        </div>
      )}

      {shortcut && (
        <kbd className="ml-2 rounded-sm border border-[hsl(var(--border-default))] px-1.5 py-0.5 font-mono text-xxs text-[hsl(var(--text-muted))]">
          {shortcut}
        </kbd>
      )}
    </Command.Item>
  );
}
