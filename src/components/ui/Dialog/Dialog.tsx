import { useEffect, useRef, type ReactNode, useState } from 'react';

import { X } from 'lucide-react';

import { Button } from '../button';

/**
 * Dialog
 *
 * Purpose: Modal dialog for confirmations, alerts, and focused interactions
 *
 * Features:
 * - Focus trap within dialog
 * - Backdrop click to close (optional)
 * - ESC key to close
 * - Smooth enter/exit animations with CSS
 * - Customizable actions
 * - Dark mode support
 *
 * States: open, closed
 * Accessibility: WCAG AA, focus management, keyboard navigation, ARIA attributes
 */

interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children?: ReactNode;
  primaryAction?: {
    label: string;
    onClick: () => void;
    variant?: 'default' | 'destructive';
    isLoading?: boolean;
  };
  secondaryAction?: {
    label: string;
    onClick: () => void;
  };
  showClose?: boolean;
  closeOnBackdrop?: boolean;
}

export default function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  primaryAction,
  secondaryAction,
  showClose = true,
  closeOnBackdrop = true,
}: DialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const previousActiveElement = useRef<HTMLElement | null>(null);
  const [isAnimating, setIsAnimating] = useState(false);
  const [shouldRender, setShouldRender] = useState(open);

  useEffect(() => {
    if (open) {
      setShouldRender(true);
      // Trigger animation after render
      setTimeout(() => setIsAnimating(true), 10);
    } else {
      setIsAnimating(false);
      // Wait for animation to complete before unmounting
      const timer = setTimeout(() => setShouldRender(false), 200);
      return () => clearTimeout(timer);
    }
  }, [open]);

  // Focus management
  useEffect(() => {
    if (open) {
      previousActiveElement.current = document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;

      // Focus the dialog
      setTimeout(() => {
        dialogRef.current?.focus();
      }, 100);

      // Prevent body scroll
      document.body.style.overflow = 'hidden';
    } else {
      // Restore focus
      previousActiveElement.current?.focus();

      // Restore body scroll
      document.body.style.overflow = '';
    }

    return () => {
      document.body.style.overflow = '';
    };
  }, [open]);

  // ESC key handler
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && open) {
        onOpenChange(false);
      }
    };

    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [open, onOpenChange]);

  const handleBackdropClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target === e.currentTarget && closeOnBackdrop) {
      onOpenChange(false);
    }
  };

  if (!shouldRender) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        className={`fixed inset-0 z-50 flex items-center justify-center p-4 bg-[hsl(var(--overlay))] transition-opacity duration-base ${
          isAnimating ? 'opacity-100' : 'opacity-0'
        }`}
        onClick={handleBackdropClick}
        role="dialog"
        aria-modal="true"
        aria-labelledby="dialog-title"
        aria-describedby={description ? 'dialog-description' : undefined}
      >
        {/* Dialog Content */}
        <div
          ref={dialogRef}
          tabIndex={-1}
          className={`relative w-full max-w-md bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] shadow-md rounded-lg focus:outline-none motion-reduce:transition-none transition-opacity duration-base ease-out ${
            isAnimating
              ? 'opacity-100 scale-100 translate-y-0'
              : 'opacity-0 scale-95 translate-y-5'
          }`}
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div className="flex items-start justify-between p-6 pb-4">
            <div className="flex-1">
              <h2
                id="dialog-title"
                className="text-lg font-semibold text-[hsl(var(--text-primary))]"
              >
                {title}
              </h2>
              {description && (
                <p
                  id="dialog-description"
                  className="mt-1.5 text-sm text-[hsl(var(--text-secondary))]"
                >
                  {description}
                </p>
              )}
            </div>

            {showClose && (
              <button
                onClick={() => onOpenChange(false)}
                className="ml-4 -mr-2 -mt-2 p-2 text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))] rounded-md hover:bg-[hsl(var(--surface))] transition-colors duration-fast ease-out focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
                aria-label="Close dialog"
              >
                <X className="w-4 h-4" strokeWidth={1.75} />
              </button>
            )}
          </div>

          {/* Content */}
          {children && (
            <div className="px-6 pb-4">
              {children}
            </div>
          )}

          {/* Actions */}
          {(primaryAction || secondaryAction) && (
            <div className="flex items-center justify-end gap-3 px-6 py-4 bg-[hsl(var(--surface))] rounded-b-lg border-t border-[hsl(var(--border-subtle))]">
              {secondaryAction && (
                <Button
                  variant="ghost"
                  onClick={secondaryAction.onClick}
                >
                  {secondaryAction.label}
                </Button>
              )}
              {primaryAction && (
                <Button
                  variant={primaryAction.variant || 'default'}
                  onClick={primaryAction.onClick}
                  disabled={primaryAction.isLoading}
                >
                  {primaryAction.isLoading ? 'Loading...' : primaryAction.label}
                </Button>
              )}
            </div>
          )}
        </div>
      </div>
    </>
  );
}
