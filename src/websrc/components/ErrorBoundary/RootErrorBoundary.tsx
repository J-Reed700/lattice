/**
 * RootErrorBoundary Component
 *
 * Top-level error boundary that catches all uncaught errors in the application.
 * Displays FullPageError component with recovery options.
 */

import React from 'react';

import { ErrorBoundary, type ErrorBoundaryProps } from './ErrorBoundary';
import { FullPageError } from './FullPageError';

interface RootErrorBoundaryProps {
  children: React.ReactNode;
  onError?: ErrorBoundaryProps['onError'];
  onReset?: ErrorBoundaryProps['onReset'];
}

export function RootErrorBoundary({
  children,
  onError,
  onReset,
}: RootErrorBoundaryProps) {
  const handleError: ErrorBoundaryProps['onError'] = (error, errorInfo) => {
    // Log to console in development
    const isDev = process.env.NODE_ENV === 'development';
    if (isDev) {
      console.error('=== ROOT ERROR BOUNDARY ===');
      console.error('Error:', error);
      console.error('Error Info:', errorInfo);
      console.error('=========================');
    }

    if (onError) {
      onError(error, errorInfo);
    }
  };

  const handleReset = () => {
    // Clear any cached state
    try {
      // Clear React Query cache if present
      if ('queryClient' in window && typeof (window as { queryClient?: { clear: () => void } }).queryClient?.clear === 'function') {
        (window as { queryClient: { clear: () => void } }).queryClient.clear();
      }

      // Clear any application state
      sessionStorage.removeItem('app-state');
    } catch (e) {
      console.warn('Failed to clear state on reset:', e);
    }

    if (onReset) {
      onReset();
    }
  };

  return (
    <ErrorBoundary
      name="RootErrorBoundary"
      fallback={FullPageError}
      onError={handleError}
      onReset={handleReset}
      resetOnPropsChange={false}
    >
      {children}
    </ErrorBoundary>
  );
}
