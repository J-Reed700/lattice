/**
 * SectionError Component
 *
 * Displays an inline error card for section-level failures.
 * Used by SectionErrorBoundary to isolate feature errors.
 */

import React, { useState } from 'react';

import './ErrorBoundary.animations.css';
import { AlertCircle, RefreshCw, ChevronDown, ChevronUp, X } from 'lucide-react';

import { getUserFriendlyMessage } from '../../types/errors';

export interface SectionErrorProps {
  error: Error;
  errorInfo?: React.ErrorInfo;
  resetError: () => void;
  sectionName?: string;
  icon?: React.ReactNode;
  onDismiss?: () => void;
}

export function SectionError({
  error,
  errorInfo,
  resetError,
  sectionName = 'Section',
  icon,
  onDismiss,
}: SectionErrorProps) {
  const [showDetails, setShowDetails] = useState(false);
  const isDevelopment = process.env.NODE_ENV === 'development';
  const userFriendlyMessage = getUserFriendlyMessage(error);

  return (
    <div className="flex items-center justify-center min-h-[400px] p-6 animate-fade-in">
      <div className="max-w-2xl w-full">
        {/* Error Card */}
        <div className="bg-[var(--surface-elevated)] rounded-lg shadow-lg border-2 border-[var(--error)]/20 overflow-hidden">
          {/* Header */}
          <div className="gradient-error border-b border-[var(--border-color)] p-6">
            <div className="flex items-start justify-between gap-4">
              <div className="flex items-start gap-3 flex-1 min-w-0">
                {/* Icon */}
                <div className="flex-shrink-0">
                  <div className="w-10 h-10 bg-[var(--error)]/20 rounded-lg flex items-center justify-center">
                    {icon || <AlertCircle className="w-5 h-5 text-[var(--error)]" />}
                  </div>
                </div>

                {/* Title */}
                <div className="flex-1 min-w-0">
                  <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-1">
                    {sectionName} Error
                  </h3>
                  <p className="text-sm text-[var(--text-secondary)] leading-relaxed">
                    {userFriendlyMessage}
                  </p>
                </div>
              </div>

              {/* Dismiss Button */}
              {onDismiss && (
                <button
                  onClick={onDismiss}
                  className="flex-shrink-0 p-1.5 text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-hover)] rounded-lg transition-colors"
                  aria-label="Dismiss error"
                >
                  <X className="w-5 h-5" />
                </button>
              )}
            </div>
          </div>

          {/* Content */}
          <div className="p-6 space-y-4">
            {/* Error Message */}
            <div className="bg-[var(--surface-base)] rounded-lg p-4 border border-[var(--border-color)]">
              <p className="text-sm text-[var(--text-primary)] font-medium mb-1">
                {error.name}
              </p>
              <p className="text-sm text-[var(--text-secondary)] break-words">
                {error.message}
              </p>
            </div>

            {/* Details Toggle (Development Only) */}
            {isDevelopment && (error.stack || errorInfo?.componentStack) && (
              <div>
                <button
                  onClick={() => setShowDetails(!showDetails)}
                  className="flex items-center gap-2 text-sm font-medium text-[var(--text-primary)] hover:text-[var(--accent-primary)] transition-colors"
                >
                  {showDetails ? (
                    <ChevronUp className="w-4 h-4" />
                  ) : (
                    <ChevronDown className="w-4 h-4" />
                  )}
                  {showDetails ? 'Hide' : 'Show'} technical details
                </button>

                {showDetails && (
                  <div className="mt-3 space-y-3">
                    {error.stack && (
                      <div>
                        <p className="text-xs font-semibold text-[var(--text-secondary)] mb-2">
                          Stack Trace:
                        </p>
                        <div className="bg-[var(--surface-base)] rounded-lg p-3 border border-[var(--border-color)] overflow-x-auto">
                          <pre className="text-xs font-mono text-[var(--text-secondary)] whitespace-pre-wrap">
                            {error.stack}
                          </pre>
                        </div>
                      </div>
                    )}

                    {errorInfo?.componentStack && (
                      <div>
                        <p className="text-xs font-semibold text-[var(--text-secondary)] mb-2">
                          Component Stack:
                        </p>
                        <div className="bg-[var(--surface-base)] rounded-lg p-3 border border-[var(--border-color)] overflow-x-auto">
                          <pre className="text-xs font-mono text-[var(--text-secondary)] whitespace-pre-wrap">
                            {errorInfo.componentStack}
                          </pre>
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* Action Button */}
            <div className="flex items-center gap-3 pt-2">
              <button
                onClick={resetError}
                className="inline-flex items-center justify-center gap-2 px-4 py-2.5 bg-[var(--accent-primary)] hover:bg-[var(--accent-hover)] text-white rounded-lg font-medium transition-all duration-150 shadow-sm hover:shadow-md"
              >
                <RefreshCw className="w-4 h-4" />
                Retry
              </button>
              <p className="text-xs text-[var(--text-secondary)]">
                Other sections of the app are still working
              </p>
            </div>
          </div>

          {/* Footer Help */}
          {!isDevelopment && (
            <div className="bg-[var(--surface-base)] border-t border-[var(--border-color)] px-6 py-4">
              <p className="text-xs text-[var(--text-secondary)]">
                If this problem persists, try refreshing the page or{' '}
                <button
                  onClick={() => window.location.reload()}
                  className="text-[var(--accent-primary)] hover:underline font-medium"
                >
                  reloading the app
                </button>
                .
              </p>
            </div>
          )}
        </div>

        {/* Development Badge */}
        {isDevelopment && (
          <div className="mt-3 text-center">
            <span className="inline-block px-2 py-1 text-xs font-medium bg-[var(--warning)]/20 text-[var(--warning)] rounded">
              Development Mode
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
