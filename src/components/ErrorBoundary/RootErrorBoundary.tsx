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
    if (import.meta.env.DEV) {
      console.error('RootErrorBoundary caught error:', error, errorInfo);
    }

    if (onError) {
      onError(error, errorInfo);
    }
  };

  const handleReset = () => {
    try {
      if ('queryClient' in window && typeof (window as { queryClient?: { clear: () => void } }).queryClient?.clear === 'function') {
        (window as { queryClient: { clear: () => void } }).queryClient.clear();
      }

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
