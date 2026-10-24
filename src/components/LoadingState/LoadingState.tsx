import { type ReactNode } from 'react';

export interface LoadingStateProps {
  type?: 'spinner' | 'skeleton' | 'dots';
  size?: 'sm' | 'md' | 'lg';
  message?: string;
  className?: string;
  centered?: boolean;
}

export function LoadingState({
  type = 'spinner',
  size = 'md',
  message,
  className = '',
  centered = true,
}: LoadingStateProps) {
  const container = centered
    ? 'flex flex-col items-center justify-center'
    : 'flex flex-col items-start';

  return (
    <div
      className={`${container} ${className}`}
      role="status"
      aria-live="polite"
      aria-label={message || 'Loading'}
    >
      {type === 'spinner' && <Spinner size={size} />}
      {type === 'skeleton' && <SkeletonLoader size={size} />}
      {type === 'dots' && <DotsLoader size={size} />}

      {message && (
        <p className="mt-4 text-sm text-[hsl(var(--text-secondary))]">
          {message}
        </p>
      )}

      <span className="sr-only">{message || 'Loading content'}</span>
    </div>
  );
}

function Spinner({ size }: { size: 'sm' | 'md' | 'lg' }) {
  const sizeClasses = {
    sm: 'w-4 h-4 border-2',
    md: 'w-8 h-8 border-2',
    lg: 'w-12 h-12 border-3',
  };

  return (
    <div
      className={`${sizeClasses[size]} rounded-full border-[hsl(var(--border-subtle))] border-t-[hsl(var(--accent))] animate-spin`}
      aria-hidden="true"
    />
  );
}

function DotsLoader({ size }: { size: 'sm' | 'md' | 'lg' }) {
  const dotSizes = {
    sm: 'w-1.5 h-1.5',
    md: 'w-2 h-2',
    lg: 'w-3 h-3',
  };

  const gapSizes = {
    sm: 'gap-1',
    md: 'gap-1.5',
    lg: 'gap-2',
  };

  return (
    <div className={`flex ${gapSizes[size]}`} aria-hidden="true">
      <div
        className={`${dotSizes[size]} rounded-full bg-[hsl(var(--accent))] animate-pulse`}
        style={{ animationDelay: '0ms' }}
      />
      <div
        className={`${dotSizes[size]} rounded-full bg-[hsl(var(--accent))] animate-pulse`}
        style={{ animationDelay: '150ms' }}
      />
      <div
        className={`${dotSizes[size]} rounded-full bg-[hsl(var(--accent))] animate-pulse`}
        style={{ animationDelay: '300ms' }}
      />
    </div>
  );
}

function SkeletonLoader({ size }: { size: 'sm' | 'md' | 'lg' }) {
  const heights = {
    sm: 'h-4',
    md: 'h-6',
    lg: 'h-8',
  };

  return (
    <div className="w-full space-y-3 animate-pulse" aria-hidden="true">
      <div className={`${heights[size]} bg-[hsl(var(--surface-raised))] rounded w-3/4`} />
      <div className={`${heights[size]} bg-[hsl(var(--surface-raised))] rounded w-full`} />
      <div className={`${heights[size]} bg-[hsl(var(--surface-raised))] rounded w-5/6`} />
    </div>
  );
}

export function InlineSpinner({ className = '' }: { className?: string }) {
  return (
    <span
      className={`inline-block w-4 h-4 border-2 border-[hsl(var(--border-subtle))] border-t-[hsl(var(--accent))] rounded-full animate-spin ${className}`}
      role="status"
      aria-label="Loading"
    >
      <span className="sr-only">Loading</span>
    </span>
  );
}

export function SkeletonCard() {
  return (
    <div
      className="bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))] p-6 animate-pulse"
      aria-hidden="true"
    >
      <div className="space-y-4">
        <div className="h-6 bg-[hsl(var(--surface-raised))] rounded w-3/4" />

        <div className="space-y-2">
          <div className="h-4 bg-[hsl(var(--surface-raised))] rounded w-full" />
          <div className="h-4 bg-[hsl(var(--surface-raised))] rounded w-5/6" />
          <div className="h-4 bg-[hsl(var(--surface-raised))] rounded w-4/5" />
        </div>

        <div className="flex gap-3 pt-2">
          <div className="h-8 bg-[hsl(var(--surface-raised))] rounded w-20" />
          <div className="h-8 bg-[hsl(var(--surface-raised))] rounded w-20" />
        </div>
      </div>
    </div>
  );
}

export function SkeletonList({ count = 5 }: { count?: number }) {
  return (
    <div className="space-y-4" aria-hidden="true">
      {Array.from({ length: count }).map((_, i) => (
        <div
          key={i}
          className="flex items-center gap-4 p-4 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))] animate-pulse"
        >
          <div className="w-10 h-10 bg-[hsl(var(--surface-raised))] rounded" />

          <div className="flex-1 space-y-2">
            <div className="h-4 bg-[hsl(var(--surface-raised))] rounded w-3/4" />
            <div className="h-3 bg-[hsl(var(--surface-raised))] rounded w-1/2" />
          </div>
        </div>
      ))}
    </div>
  );
}

export function FullPageLoading({
  message = 'Loading...',
}: {
  message?: string;
}) {
  return (
    <div className="flex flex-col items-center justify-center min-h-screen bg-[hsl(var(--surface))]">
      <LoadingState type="spinner" size="lg" message={message} />
    </div>
  );
}

export function SectionLoading({
  message,
  overlay = false,
}: {
  message?: string;
  overlay?: boolean;
}) {
  if (overlay) {
    return (
      <div className="absolute inset-0 flex items-center justify-center bg-[hsl(var(--overlay))] z-10">
        <LoadingState type="spinner" size="md" message={message} />
      </div>
    );
  }

  return (
    <div className="flex items-center justify-center py-12">
      <LoadingState type="spinner" size="md" message={message} />
    </div>
  );
}

export function ButtonLoading({ children }: { children?: ReactNode }) {
  return (
    <span className="flex items-center gap-2">
      <InlineSpinner />
      {children}
    </span>
  );
}
