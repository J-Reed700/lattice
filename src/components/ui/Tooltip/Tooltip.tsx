import { type ReactNode } from 'react';

import * as TooltipPrimitive from '@radix-ui/react-tooltip';

/**
 * Tooltip
 *
 * Purpose: Display contextual information on hover or focus
 *
 * Features:
 * - Keyboard accessible (ESC to close)
 * - Positioning with arrow indicator
 * - Configurable delay
 * - Portal rendering for proper z-index layering
 * - Respects reduced motion preferences
 *
 * States: closed, open
 * Accessibility: WCAG AA, keyboard navigation, ARIA labeling
 */

export interface TooltipProps {
  children: React.ReactNode;
  content: string | React.ReactNode;
  side?: 'top' | 'right' | 'bottom' | 'left';
  delay?: number;
  disabled?: boolean;
  sideOffset?: number;
}

export function Tooltip({
  children,
  content,
  side = 'top',
  delay = 200,
  disabled = false,
  sideOffset = 8,
}: TooltipProps) {
  if (disabled) {
    return <>{children}</>;
  }

  return (
    <TooltipPrimitive.Root delayDuration={delay}>
      <TooltipPrimitive.Trigger asChild>
        {children}
      </TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={sideOffset}
          className="
            z-50
            overflow-hidden
            rounded-md
            border
            border-[hsl(var(--border-subtle))]
            bg-[hsl(var(--surface-raised))]
            px-3
            py-2
            text-sm
            text-[hsl(var(--text-primary))]
            shadow-md
            animate-in
            fade-in-0
            zoom-in-95
            data-[state=closed]:animate-out
            data-[state=closed]:fade-out-0
            data-[state=closed]:zoom-out-95
            data-[side=bottom]:slide-in-from-top-2
            data-[side=left]:slide-in-from-right-2
            data-[side=right]:slide-in-from-left-2
            data-[side=top]:slide-in-from-bottom-2
          "
        >
          {content}
          <TooltipPrimitive.Arrow className="fill-[hsl(var(--surface-raised))]" />
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

/**
 * TooltipProvider
 *
 * Purpose: Provides context for all tooltips in the tree
 *
 * Usage: Wrap your app or a section of your app with this provider
 * to enable tooltip functionality.
 *
 * Example:
 * ```tsx
 * <TooltipProvider>
 *   <App />
 * </TooltipProvider>
 * ```
 */
export interface TooltipProviderProps {
  children: ReactNode;
  delayDuration?: number;
  skipDelayDuration?: number;
  disableHoverableContent?: boolean;
}

export function TooltipProvider({
  children,
  delayDuration = 200,
  skipDelayDuration = 300,
  disableHoverableContent = false,
}: TooltipProviderProps) {
  return (
    <TooltipPrimitive.Provider
      delayDuration={delayDuration}
      skipDelayDuration={skipDelayDuration}
      disableHoverableContent={disableHoverableContent}
    >
      {children}
    </TooltipPrimitive.Provider>
  );
}

export const TooltipTrigger = TooltipPrimitive.Trigger;
export const TooltipContent = TooltipPrimitive.Content;
export const TooltipPortal = TooltipPrimitive.Portal;
export const TooltipArrow = TooltipPrimitive.Arrow;
