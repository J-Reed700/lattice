import type React from 'react';

export interface StatusLineProps {
  busy?: string | null;
  error?: string | null;
  onRetry?: () => void;
  className?: string;
}

/**
 * Minimal Stage 1 non-panicking stub for StatusLine.
 * Returns null so tests asserting on busy/error status fail as expected.
 */
export function StatusLine(_props: StatusLineProps): JSX.Element | null {
  return null;
}

/**
 * Minimal Stage 1 non-panicking stub for PracticeLoadingState.
 */
export function PracticeLoadingState(_props: { label: string }): JSX.Element {
  return <div />;
}

/**
 * Minimal Stage 1 non-panicking stub for PracticeEmptyState.
 */
export function PracticeEmptyState(_props: {
  title: string;
  description?: string;
  action?: React.ReactNode;
}): JSX.Element {
  return <div />;
}
