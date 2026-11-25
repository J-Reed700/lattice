/**
 * Class strings shared by Settings panes (and a couple of indexing status
 * components in `features/files`), aligned with Wave 1 `Button` and `Input`
 * tokens so every pane reads as one calm minimalist surface.
 */

import { settingsFieldClass } from '@/components/ui';
import { cn } from '@/lib/utils';

/** A number or otherwise short field parked in a row's control column. */
export const NUMBER_FIELD_CLASS = cn(settingsFieldClass, 'w-28 text-right tabular-nums');

export const SECONDARY_BUTTON_CLASS =
  'pressable inline-flex h-8 shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg border border-border-default bg-surface px-3 text-ui font-medium text-text-primary shadow-control transition-[background-color,border-color,color,scale] duration-fast ease-out hover:border-border-strong hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-45';

export const GHOST_BUTTON_CLASS =
  'pressable inline-flex h-8 shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg px-3 text-ui font-medium text-text-secondary transition-[background-color,color,scale] duration-fast ease-out hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-45';

/** Standardized primary CTA using Option A `--action` tokens (deep slate ink / crisp porcelain). */
export const PRIMARY_BUTTON_CLASS =
  'pressable inline-flex h-8 shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg bg-action px-3 text-ui font-medium text-action-fg shadow-action transition-[background-color,box-shadow,scale] duration-fast ease-out hover:bg-action-hover focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-bg disabled:cursor-not-allowed disabled:opacity-45';

/** The 28px square button that appears at the end of a removable list row. */
export const ROW_ACTION_CLASS =
  'inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-danger-fg focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-45';

export const CHECKBOX_CLASS =
  'h-4 w-4 rounded-sm border-border-default bg-surface accent-accent disabled:opacity-50';

/**
 * Ensure unchecked Switch tracks remain clearly legible against both light
 * architectural porcelain and dark obsidian-slate page grounds.
 */
export const SWITCH_CLASS = 'data-[state=unchecked]:bg-border-default';
