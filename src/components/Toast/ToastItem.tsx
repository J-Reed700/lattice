/**
 * ToastItem - Individual toast notification card
 *
 * Purpose: Renders a single toast with:
 * - Type-specific icon and styling
 * - Title and optional message
 * - Close button (if dismissible)
 * - Optional action button
 * - Progress bar for auto-dismiss countdown
 * - Pause on hover support
 *
 * States: default, hover, exiting
 * Accessibility: WCAG AA, keyboard navigation, ARIA live regions
 */

import React, { useState, useEffect, useRef } from 'react';

import { SuccessIcon, ErrorIcon, WarningIcon, InfoIcon, CloseIcon } from './ToastIcons';
import { type Toast } from '../../stores/toastStore';

interface ToastItemProps {
  toast: Toast;
  onDismiss: (id: string) => void;
  pauseOnHover: boolean;
  index: number;
}

export const ToastItem: React.FC<ToastItemProps> = ({
  toast,
  onDismiss,
  pauseOnHover,
  index,
}) => {
  const [isExiting, setIsExiting] = useState(false);
  const [isPaused, setIsPaused] = useState(false);
  const [progress, setProgress] = useState(100);
  const progressIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const startTimeRef = useRef<number>(Date.now());
  const remainingTimeRef = useRef<number>(toast.duration || 0);

  const getTypeStyles = () => {
    switch (toast.type) {
      case 'success':
        return {
          bg: 'hsl(var(--success-muted))',
          border: 'hsl(var(--success-fg))',
          icon: <SuccessIcon className="text-[hsl(var(--success-fg))]" />,
          iconBg: 'hsl(var(--success-muted))',
        };
      case 'error':
        return {
          bg: 'hsl(var(--danger-muted))',
          border: 'hsl(var(--danger-fg))',
          icon: <ErrorIcon className="text-[hsl(var(--danger-fg))]" />,
          iconBg: 'hsl(var(--danger-muted))',
        };
      case 'warning':
        return {
          bg: 'hsl(var(--warning-muted))',
          border: 'hsl(var(--warning-fg))',
          icon: <WarningIcon className="text-[hsl(var(--warning-fg))]" />,
          iconBg: 'hsl(var(--warning-muted))',
        };
      case 'info':
        return {
          bg: 'hsl(var(--accent-muted))',
          border: 'hsl(var(--accent))',
          icon: <InfoIcon className="text-[hsl(var(--accent))]" />,
          iconBg: 'hsl(var(--accent-muted))',
        };
    }
  };

  const styles = getTypeStyles();

  const handleDismiss = () => {
    setIsExiting(true);
    setTimeout(() => {
      onDismiss(toast.id);
    }, 300); // Match animation duration
  };

  const handleAction = () => {
    if (toast.action) {
      toast.action.onClick();
      handleDismiss();
    }
  };

  // Progress bar animation
  useEffect(() => {
    if (!toast.duration || toast.duration === 0) return;

    const updateProgress = () => {
      const elapsed = Date.now() - startTimeRef.current;
      const remaining = Math.max(0, remainingTimeRef.current - elapsed);
      const duration = toast.duration ?? 4000;
      const progressValue = (remaining / duration) * 100;

      setProgress(progressValue);

      if (remaining <= 0) {
        handleDismiss();
      }
    };

    if (!isPaused) {
      startTimeRef.current = Date.now();
      progressIntervalRef.current = setInterval(updateProgress, 16); // ~60fps
    } else {
      if (progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
        remainingTimeRef.current = (progress / 100) * toast.duration;
      }
    }

    return () => {
      if (progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isPaused, toast.duration]);

  // Pause on hover
  const handleMouseEnter = () => {
    if (pauseOnHover) {
      setIsPaused(true);
    }
  };

  const handleMouseLeave = () => {
    if (pauseOnHover) {
      setIsPaused(false);
    }
  };

  return (
    <div
      role="status"
      aria-live="polite"
      aria-atomic="true"
      className={`
        relative w-full max-w-sm overflow-hidden
        rounded-md border border-border-subtle bg-surface-raised shadow-md
        transition-opacity duration-base ease-out
        ${isExiting ? 'opacity-0' : 'opacity-100'}
        ${index > 0 ? 'mt-2' : ''}
      `}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      <div className="flex items-start gap-3 px-3.5 py-3">
        <div className="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center [&>svg]:h-4 [&>svg]:w-4">
          {toast.icon || styles.icon}
        </div>

        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-text-primary">{toast.title}</p>
          {toast.message && (
            <p className="mt-0.5 text-xs text-text-secondary break-words">{toast.message}</p>
          )}
          {toast.action && (
            <button
              type="button"
              onClick={handleAction}
              className="mt-2 text-xs font-medium text-text-primary underline-offset-2 hover:underline"
            >
              {toast.action.label}
            </button>
          )}
        </div>

        {toast.dismissible && (
          <button
            type="button"
            onClick={handleDismiss}
            className="-mr-1 -mt-1 shrink-0 rounded-sm p-1 text-text-muted transition-colors duration-fast hover:bg-surface hover:text-text-primary"
            aria-label="Dismiss notification"
          >
            <CloseIcon className="h-3.5 w-3.5" />
          </button>
        )}
      </div>

      {toast.duration && toast.duration > 0 && (
        <div
          className="absolute bottom-0 left-0 h-px transition-[width] duration-fast ease-linear"
          style={{ width: `${progress}%`, backgroundColor: styles.border }}
          role="progressbar"
          aria-valuenow={progress}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label="Time remaining"
        />
      )}
    </div>
  );
};
