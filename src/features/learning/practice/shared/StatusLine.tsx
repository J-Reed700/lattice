import type React from 'react';
import { AlertCircle, CheckCircle2, Inbox, Info, Loader2, RotateCcw } from 'lucide-react';

import { cn } from '@/lib/utils';

export type StatusTone = 'busy' | 'info' | 'success' | 'error';
export type VoicePhase = 'off' | 'starting' | 'listening' | 'hearing' | 'transcribing' | 'thinking' | 'speaking';

export interface StatusLineProps {
  tone?: StatusTone;
  children?: React.ReactNode;
  busy?: string | null;
  error?: string | null;
  onRetry?: () => void;
  className?: string;
  retryLabel?: string;
  retryAriaLabel?: string;
}

/**
 * Shared status line announcing busy, info, success, and error states via aria-live.
 */
export function StatusLine({
  tone,
  children,
  busy,
  error,
  onRetry,
  className,
  retryLabel,
  retryAriaLabel,
}: StatusLineProps) {
  const effectiveTone: StatusTone | null = tone ?? (error ? 'error' : busy ? 'busy' : children ? 'info' : null);
  const content = children ?? error ?? busy;

  if (!effectiveTone || !content) {
    return null;
  }

  if (effectiveTone === 'busy') {
    return (
      <div
        role="status"
        aria-live="polite"
        className={cn('flex items-center gap-2 text-xs text-text-muted', className)}
      >
        <Loader2 className="h-3.5 w-3.5 animate-spin text-accent" aria-hidden="true" />
        <span>{content}</span>
      </div>
    );
  }

  if (effectiveTone === 'error') {
    return (
      <div
        role="alert"
        aria-live="assertive"
        className={cn(
          'flex flex-wrap items-center justify-between gap-2 rounded-lg bg-danger-muted px-3 py-2 text-xs text-danger-fg',
          className,
        )}
      >
        <div className="flex items-center gap-2">
          <AlertCircle className="h-4 w-4 shrink-0" aria-hidden="true" />
          <span>{content}</span>
        </div>
        {onRetry && (
          <button
            type="button"
            aria-label={retryAriaLabel}
            onClick={onRetry}
            className="inline-flex items-center gap-1 font-medium underline hover:text-text-primary focus:outline-hidden focus:ring-2 focus:ring-ring"
          >
            <RotateCcw className="h-3 w-3" aria-hidden="true" />
            <span>{retryLabel ?? 'Try again'}</span>
          </button>
        )}
      </div>
    );
  }

  if (effectiveTone === 'success') {
    return (
      <div
        role="status"
        aria-live="polite"
        className={cn(
          'flex items-center gap-2 rounded-lg bg-success-muted px-3 py-2 text-xs text-success-fg',
          className,
        )}
      >
        <CheckCircle2 className="h-4 w-4 shrink-0" aria-hidden="true" />
        <span>{content}</span>
      </div>
    );
  }

  return (
    <div
      role="status"
      aria-live="polite"
      className={cn('flex items-center gap-2 text-xs text-text-secondary', className)}
    >
      <Info className="h-3.5 w-3.5 shrink-0 text-accent" aria-hidden="true" />
      <span>{content}</span>
    </div>
  );
}

/**
 * Centered accessible loading placeholder for practice panels and attempts.
 */
export function PracticeLoadingState({ label }: { label: string }) {
  return (
    <div
      role="status"
      aria-live="polite"
      className="flex min-h-[160px] flex-col items-center justify-center gap-3 p-6 text-center text-xs text-text-secondary"
    >
      <Loader2 className="h-6 w-6 animate-spin text-accent" aria-hidden="true" />
      <span>{label}</span>
    </div>
  );
}

export const LoadingState = PracticeLoadingState;

/**
 * Shared empty state for practice sections.
 */
export function PracticeEmptyState({
  title,
  description,
  children,
  action,
}: {
  title: string;
  description?: string;
  children?: React.ReactNode;
  action?: React.ReactNode;
}) {
  const body = children ?? description;
  return (
    <div className="flex min-h-[160px] flex-col items-center justify-center gap-2 rounded-xl border border-border-subtle bg-surface p-6 text-center">
      <Inbox className="h-7 w-7 text-text-muted" aria-hidden="true" />
      <h4 className="text-sm font-semibold text-text-primary">{title}</h4>
      {body && <div className="text-xs text-text-secondary">{body}</div>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export const EmptyState = PracticeEmptyState;
