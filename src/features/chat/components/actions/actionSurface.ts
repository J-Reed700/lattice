/**
 * The look shared by the surfaces that carry work out of the chat.
 *
 * `SpacePickerPopover` and `ModelPickerPopover` set the house style for a
 * floating panel; these constants keep the answer menu and the claim popover on
 * it so the verbs feel like one family rather than three separate menus.
 */

/** A floating panel: overlay surface, soft edge, lifted off the page. */
export const ACTION_SURFACE_CLASS =
  'surface-pop z-[60] w-[280px] rounded-xl bg-surface-overlay p-1.5 shadow-lg outline-none';

/** One verb in such a panel: an icon, the verb, and a line saying what it does. */
export const ACTION_ROW_CLASS =
  'flex w-full items-start gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] focus-visible:bg-[hsl(var(--text-primary)/0.06)] focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50';

/** The quiet line above a panel's verbs, saying what they act on. */
export const ACTION_HEADING_CLASS = 'px-2 pb-1.5 pt-1 text-xs text-text-muted';
