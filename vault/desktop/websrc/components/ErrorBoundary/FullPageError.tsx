/**
 * FullPageError Component
 *
 * Displays a full-screen error page when critical errors occur.
 * Used by RootErrorBoundary for app-level failures.
 */

import React, { useState } from 'react';

import './ErrorBoundary.animations.css';
import { AlertTriangle, Copy, RefreshCw, Home, Bug, ChevronDown, ChevronUp } from 'lucide-react';

import { getUserFriendlyMessage, sanitizeErrorMessage } from '../../types/errors';
import { downloadErrorLog } from '../../utils/errorLogger';

export interface FullPageErrorProps {
  error: Error;
  errorInfo?: React.ErrorInfo;
  resetError: () => void;
}

export function FullPageError({ error, errorInfo, resetError }: FullPageErrorProps) {
  const [showDetails, setShowDetails] = useState(false);
  const [copied, setCopied] = useState(false);
  const isDevelopment = process.env.NODE_ENV === 'development';

  const handleCopyError = () => {
    const errorDetails = `
Error: ${error.name}
Message: ${error.message}
Stack: ${error.stack || 'N/A'}
Component Stack: ${errorInfo?.componentStack || 'N/A'}
Timestamp: ${new Date().toISOString()}
URL: ${window.location.href}
User Agent: ${navigator.userAgent}
    `.trim();

    navigator.clipboard.writeText(errorDetails).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const handleReloadApp = () => {
    window.location.reload();
  };

  const handleReportBug = () => {
    // Open GitHub issues or bug reporting system
    const title = encodeURIComponent(`Bug: ${error.name}`);
    const body = encodeURIComponent(`
## Error Description
${getUserFriendlyMessage(error)}

## Error Type
${error.name}

## Environment
- Timestamp: ${new Date().toISOString()}
- User Agent: ${navigator.userAgent}

## Additional Context
<!-- Please describe what you were doing when this error occurred -->
    `);
    window.open(`https://github.com/your-repo/recall/issues/new?title=${title}&body=${body}`, '_blank');
  };

  const userFriendlyMessage = getUserFriendlyMessage(error);
  const displayMessage = isDevelopment ? error.message : sanitizeErrorMessage(error.message);

  return (
    <div className="min-h-screen flex items-center justify-center bg-[var(--bg-primary)] p-4">
      <div className="max-w-2xl w-full">
        {/* Error Card */}
        <div className="bg-[var(--surface-elevated)] rounded-xl shadow-2xl border border-[var(--border-color)] overflow-hidden">
          {/* Header */}
          <div className="gradient-error border-b border-[var(--border-color)] p-8">
            <div className="flex items-start gap-4">
              {/* Icon */}
              <div className="flex-shrink-0">
                <div className="w-16 h-16 bg-[var(--error)]/20 rounded-full flex items-center justify-center animate-pulse">
                  <AlertTriangle className="w-8 h-8 text-[var(--error)]" />
                </div>
              </div>

              {/* Title and Description */}
              <div className="flex-1 min-w-0">
                <h1 className="text-2xl font-bold text-[var(--text-primary)] mb-2">
                  Something went wrong
                </h1>
                <p className="text-[var(--text-secondary)] text-sm leading-relaxed">
                  {userFriendlyMessage}
                </p>
              </div>
            </div>
          </div>

          {/* Content */}
          <div className="p-8 space-y-6">
            {/* Error Message */}
            <div>
              <div className="flex items-center justify-between mb-2">
                <h2 className="text-sm font-semibold text-[var(--text-primary)]">
                  Error Details
                </h2>
                <button
                  onClick={handleCopyError}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] bg-[var(--surface-base)] hover:bg-[var(--surface-hover)] rounded-lg border border-[var(--border-color)] transition-colors"
                  title="Copy error details"
                >
                  <Copy className="w-3.5 h-3.5" />
                  {copied ? 'Copied!' : 'Copy'}
                </button>
              </div>
              <div className="bg-[var(--surface-base)] rounded-lg p-4 border border-[var(--border-color)]">
                <div className="flex items-start gap-2">
                  <span className="text-xs font-semibold text-[var(--error)] uppercase tracking-wide">
                    {error.name}
                  </span>
                </div>
                <p className="text-sm font-mono text-[var(--text-primary)] mt-2 break-words">
                  {displayMessage}
                </p>
              </div>
            </div>

            {/* Stack Trace (Development Only) */}
            {isDevelopment && error.stack && (
              <div>
                <button
                  onClick={() => setShowDetails(!showDetails)}
                  className="flex items-center gap-2 text-sm font-semibold text-[var(--text-primary)] hover:text-[var(--accent-primary)] transition-colors mb-2"
                >
                  {showDetails ? (
                    <ChevronUp className="w-4 h-4" />
                  ) : (
                    <ChevronDown className="w-4 h-4" />
                  )}
                  Stack Trace
                </button>
                {showDetails && (
                  <div className="bg-[var(--surface-base)] rounded-lg p-4 border border-[var(--border-color)] overflow-x-auto">
                    <pre className="text-xs font-mono text-[var(--text-secondary)] whitespace-pre-wrap">
                      {error.stack}
                    </pre>
                  </div>
                )}
              </div>
            )}

            {/* Component Stack (Development Only) */}
            {isDevelopment && errorInfo?.componentStack && (
              <div>
                <button
                  onClick={() => setShowDetails(!showDetails)}
                  className="flex items-center gap-2 text-sm font-semibold text-[var(--text-primary)] hover:text-[var(--accent-primary)] transition-colors mb-2"
                >
                  Component Stack
                </button>
                {showDetails && (
                  <div className="bg-[var(--surface-base)] rounded-lg p-4 border border-[var(--border-color)] overflow-x-auto">
                    <pre className="text-xs font-mono text-[var(--text-secondary)] whitespace-pre-wrap">
                      {errorInfo.componentStack}
                    </pre>
                  </div>
                )}
              </div>
            )}

            {/* Action Buttons */}
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-4">
              <button
                onClick={handleReloadApp}
                className="inline-flex items-center justify-center gap-2 px-4 py-3 bg-[var(--accent-primary)] hover:bg-[var(--accent-hover)] text-white rounded-lg font-medium transition-all duration-150 shadow-sm hover:shadow-md"
              >
                <RefreshCw className="w-4 h-4" />
                Reload App
              </button>
              <button
                onClick={resetError}
                className="inline-flex items-center justify-center gap-2 px-4 py-3 bg-[var(--surface-base)] hover:bg-[var(--surface-hover)] text-[var(--text-primary)] rounded-lg font-medium border border-[var(--border-color)] transition-all duration-150"
              >
                <Home className="w-4 h-4" />
                Try Again
              </button>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <button
                onClick={handleReportBug}
                className="inline-flex items-center justify-center gap-2 px-4 py-3 bg-[var(--surface-base)] hover:bg-[var(--surface-hover)] text-[var(--text-primary)] rounded-lg font-medium border border-[var(--border-color)] transition-all duration-150"
              >
                <Bug className="w-4 h-4" />
                Report Issue
              </button>
              {isDevelopment && (
                <button
                  onClick={downloadErrorLog}
                  className="inline-flex items-center justify-center gap-2 px-4 py-3 bg-[var(--surface-base)] hover:bg-[var(--surface-hover)] text-[var(--text-primary)] rounded-lg font-medium border border-[var(--border-color)] transition-all duration-150"
                >
                  <Copy className="w-4 h-4" />
                  Download Log
                </button>
              )}
            </div>
          </div>

          {/* Footer */}
          <div className="bg-[var(--surface-base)] border-t border-[var(--border-color)] p-6">
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0">
                <div className="w-8 h-8 bg-[var(--accent-primary)]/20 rounded-full flex items-center justify-center">
                  <span className="text-[var(--accent-primary)] text-sm font-semibold">i</span>
                </div>
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-sm text-[var(--text-secondary)] leading-relaxed">
                  <strong className="text-[var(--text-primary)]">Need help?</strong>
                  {' '}If this error persists, try reloading the app or clearing your cache.
                  You can also check the{' '}
                  <a
                    href="https://github.com/your-repo/recall/issues"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-[var(--accent-primary)] hover:underline"
                  >
                    issue tracker
                  </a>
                  {' '}for known issues.
                </p>
              </div>
            </div>
          </div>
        </div>

        {/* Development Info */}
        {isDevelopment && (
          <div className="mt-4 text-center">
            <p className="text-xs text-[var(--text-secondary)]">
              Development Mode - Full error details are shown
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
