import { type ComponentPropsWithoutRef, forwardRef } from 'react';

import { cn } from '@/lib/utils';

import { Tooltip, TooltipContent, TooltipTrigger } from './tooltip';

/**
 * IconButton
 *
 * A 28px square ghost button for sidebar headers and toolbars. Always has an
 * accessible label; the tooltip shows the label and, if given, the shortcut.
 */
interface IconButtonProps extends Omit<ComponentPropsWithoutRef<'button'>, 'aria-label'> {
  label: string;
  shortcut?: string;
  /** Visual size. `sm` is 28px, `md` is 32px. */
  size?: 'sm' | 'md';
  /** Render as pressed/active (e.g. a toggled filter). */
  active?: boolean;
  tooltipSide?: 'top' | 'right' | 'bottom' | 'left';
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(
  ({ label, shortcut, size = 'sm', active = false, tooltipSide = 'bottom', className, children, ...props }, ref) => {
    const button = (
      <button
        ref={ref}
        type="button"
        aria-label={label}
        aria-pressed={active || undefined}
        className={cn(
          'pressable inline-flex shrink-0 items-center justify-center rounded-md transition-[background-color,color,scale] duration-fast',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
          'disabled:cursor-not-allowed disabled:opacity-50',
          size === 'sm' ? 'h-7 w-7 [&>svg]:h-[15px] [&>svg]:w-[15px]' : 'h-8 w-8 [&>svg]:h-4 [&>svg]:w-4',
          active
            ? 'bg-accent-muted text-accent'
            : 'text-text-tertiary hover:bg-[hsl(var(--text-primary)/0.07)] hover:text-text-primary',
          className,
        )}
        {...props}
      >
        {children}
      </button>
    );

    return (
      <Tooltip delayDuration={300}>
        <TooltipTrigger asChild>{button}</TooltipTrigger>
        <TooltipContent side={tooltipSide} sideOffset={6}>
          <span className="flex items-center gap-2">
            <span>{label}</span>
            {shortcut ? <kbd className="kbd">{shortcut}</kbd> : null}
          </span>
        </TooltipContent>
      </Tooltip>
    );
  },
);

IconButton.displayName = 'IconButton';
