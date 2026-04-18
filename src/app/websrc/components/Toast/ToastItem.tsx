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

  // Get type-specific styling
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

  // Handle dismiss with exit animation
  const handleDismiss = () => {
    setIsExiting(true);
    setTimeout(() => {
      onDismiss(toast.id);
    }, 300); // Match animation duration
  };

  // Handle action click
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
        bg-[hsl(var(--surface-raised))]
        border-l-4
        rounded-md shadow-md
        transition-opacity duration-base ease-out
        ${isExiting ? 'opacity-0 translate-x-full scale-95' : 'opacity-100 translate-x-0 scale-100'}
        ${index > 0 ? 'mt-3' : ''}
      `}
      style={{
        borderLeftColor: styles.border,
        boxShadow: 'var(--shadow-md)',
      }}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      {/* Main content */}
      <div className="flex items-start gap-3 p-4">
        {/* Icon */}
        <div
          className="flex-shrink-0 w-10 h-10 rounded-full flex items-center justify-center"
          style={{ backgroundColor: styles.iconBg }}
        >
          {toast.icon || styles.icon}
        </div>

        {/* Text content */}
        <div className="flex-1 min-w-0 pt-0.5">
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))] mb-1">
            {toast.title}
          </h3>
          {toast.message && (
            <p className="text-sm text-[hsl(var(--text-secondary))] break-words">
              {toast.message}
            </p>
          )}

          {/* Action button */}
          {toast.action && (
            <button
              onClick={handleAction}
              className="mt-2 text-sm font-medium hover:underline"
              style={{ color: styles.border }}
            >
              {toast.action.label}
            </button>
          )}
        </div>

        {/* Close button */}
        {toast.dismissible && (
          <button
            onClick={handleDismiss}
            className="flex-shrink-0 p-1 rounded-md hover:bg-[hsl(var(--surface))] transition-colors duration-fast"
            aria-label="Dismiss notification"
          >
            <CloseIcon className="text-[hsl(var(--text-secondary))]" />
          </button>
        )}
      </div>

      {/* Progress bar */}
      {toast.duration && toast.duration > 0 && (
        <div
          className="absolute bottom-0 left-0 h-1 transition-[width] duration-fast ease-linear"
          style={{
            width: `${progress}%`,
            backgroundColor: styles.border,
          }}
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
