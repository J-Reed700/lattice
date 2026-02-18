import React from 'react';

import { AlertCircle, RefreshCw } from 'lucide-react';

import { ErrorBoundary, type ErrorFallbackProps } from './ErrorBoundary';

interface FeatureErrorFallbackProps {
  error: Error;
  reset: () => void;
  featureName: string;
  icon?: React.ReactNode;
}

function FeatureErrorFallback({ error, reset, featureName, icon }: FeatureErrorFallbackProps) {
  return (
    <div className="flex items-center justify-center min-h-[400px] p-8">
      <div className="text-center max-w-md">
        <div className="mx-auto w-16 h-16 bg-[var(--error-light)]/20 rounded-full flex items-center justify-center mb-4">
          {icon || <AlertCircle className="w-8 h-8 text-[var(--error)]" />}
        </div>
        <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-2">
          {featureName} Error
        </h3>
        <p className="text-sm text-[var(--text-secondary)] mb-4">
          {error.message || `Something went wrong with ${featureName.toLowerCase()}`}
        </p>
        <button
          onClick={reset}
          className="inline-flex items-center gap-2 px-4 py-2 bg-[var(--accent-primary)] hover:bg-[var(--accent-hover)] text-white rounded-lg font-medium transition-colors"
        >
          <RefreshCw className="w-4 h-4" />
          Try Again
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