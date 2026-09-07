import React from 'react';

import { RefreshCw } from 'lucide-react';

import { ErrorBoundary, type ErrorFallbackProps } from './ErrorBoundary';

interface FeatureErrorFallbackProps {
  error: Error;
  reset: () => void;
  featureName: string;
  icon?: React.ReactNode;
}

function FeatureErrorFallback({ error, reset, featureName }: FeatureErrorFallbackProps) {
  return (
    <div className="flex min-h-[320px] items-center justify-center p-8">
      <div className="max-w-sm text-center">
        <p className="text-sm text-text-secondary">Couldn't load {featureName}.</p>
        {error.message ? <p className="mt-1 text-xs text-text-muted break-words">{error.message}</p> : null}
        <button
          type="button"
          onClick={reset}
          className="mt-4 inline-flex h-8 items-center gap-2 rounded-md border border-border-default bg-surface px-3 text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-surface-raised"
        >
          <RefreshCw className="h-4 w-4" strokeWidth={1.75} />
          Try again
        </button>
      </div>
    </div>
  );
}

interface FeatureErrorBoundaryProps {
  children: React.ReactNode;
  featureName: string;
  icon?: React.ReactNode;
}

export function FeatureErrorBoundary({ children, featureName, icon }: FeatureErrorBoundaryProps) {
  const FallbackComponent = ({ error, resetError }: ErrorFallbackProps) => (
    <FeatureErrorFallback
      error={error}
      reset={resetError}
      featureName={featureName}
      icon={icon}
    />
  );

  return (
    <ErrorBoundary fallback={FallbackComponent}>
      {children}
    </ErrorBoundary>
  );
}

// Specific error boundaries for different features
export function SearchErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <FeatureErrorBoundary featureName="Search">
      {children}
    </FeatureErrorBoundary>
  );
}

export function IndexingErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <FeatureErrorBoundary featureName="Indexing">
      {children}
    </FeatureErrorBoundary>
  );
}

export function SettingsErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <FeatureErrorBoundary featureName="Settings">
      {children}
    </FeatureErrorBoundary>
  );
}

export function FilesErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <FeatureErrorBoundary featureName="File Browser">
      {children}
    </FeatureErrorBoundary>
  );
}

export function QAErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <FeatureErrorBoundary featureName="Chat">
      {children}
    </FeatureErrorBoundary>
  );
}
