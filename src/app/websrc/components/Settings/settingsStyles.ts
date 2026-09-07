/**
 * Class strings shared by every Settings pane, so the panes look like one
 * surface rather than eleven. Built on the `SettingsSection` primitives in
 * `components/ui`.
 */

import { cn } from '@/lib/utils';

import { settingsFieldClass } from '../ui';

/** A number or otherwise short field parked in a row's control column. */
export const NUMBER_FIELD_CLASS = cn(settingsFieldClass, 'w-28 text-right tabular-nums');

export const SECONDARY_BUTTON_CLASS =
  'inline-flex h-8 shrink-0 items-center rounded-sm border border-border-default bg-surface px-3 text-sm text-text-primary transition-colors duration-fast hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50';

export const GHOST_BUTTON_CLASS =
  'inline-flex h-8 shrink-0 items-center rounded-sm px-3 text-sm text-text-secondary transition-colors duration-fast hover:bg-surface-raised hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-50';

export const PRIMARY_BUTTON_CLASS =
  'inline-flex h-8 shrink-0 items-center rounded-sm bg-accent px-3 text-sm font-medium text-accent-fg transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50';

/** The 28px square button that appears at the end of a removable list row. */
export const ROW_ACTION_CLASS =
  'inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-text-muted transition-colors duration-fast hover:bg-surface-raised hover:text-danger-fg disabled:cursor-not-allowed disabled:opacity-50';

export const CHECKBOX_CLASS =
  'h-4 w-4 rounded-sm border-border-default bg-bg accent-accent disabled:opacity-50';

/**
 * The shared Switch paints its unchecked track `surface-raised`, which is
 * pure white in the light theme and therefore invisible on the page ground.
 * Every Settings switch carries this so "off" reads as off in both themes.
 */
export const SWITCH_CLASS = 'data-[state=unchecked]:bg-border-default';
