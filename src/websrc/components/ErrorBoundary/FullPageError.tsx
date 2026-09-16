/**
 * FullPageError — the app-level failure screen used by RootErrorBoundary.
 *
 * One column, no tinted header, no icon circle. Says what happened, offers
 * the two things that can help, and keeps the technical detail one click
 * away.
 */

import React, { useState } from 'react';

import { getUserFriendlyMessage, sanitizeErrorMessage } from '../../types/errors';
import { downloadErrorLog } from '../../utils/errorLogger';

export interface FullPageErrorProps {
  error: Error;
  errorInfo?: React.ErrorInfo;
  resetError: () => void;
}

const secondaryButton =
  'inline-flex h-8 items-center rounded-md border border-border-default bg-surface px-3 text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-surface-raised disabled:opacity-50';
const ghostButton =
  'inline-flex h-8 items-center rounded-md px-2 text-sm text-text-secondary transition-colors duration-fast hover:bg-surface-raised hover:text-text-primary';

export function FullPageError({ error, errorInfo, resetError }: FullPageErrorProps) {
  const [showDetails, setShowDetails] = useState(false);
  const [copied, setCopied] = useState(false);
  const isDevelopment = process.env.NODE_ENV === 'development';

  const handleCopyError = () => {
    const errorDetails = [
      `Error: ${error.name}`,
      `Message: ${error.message}`,
      `Stack: ${error.stack || 'N/A'}`,
      `Component Stack: ${errorInfo?.componentStack || 'N/A'}`,
      `Timestamp: ${new Date().toISOString()}`,
      `URL: ${window.location.href}`,
      `User Agent: ${navigator.userAgent}`,
    ].join('\n');

    void navigator.clipboard.writeText(errorDetails).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const userFriendlyMessage = getUserFriendlyMessage(error);
  const displayMessage = isDevelopment ? error.message : sanitizeErrorMessage(error.message);

  return (
    <div className="flex min-h-screen items-center justify-center bg-bg p-6">
      <div className="w-full max-w-md">
        <h1 className="font-serif text-xl font-semibold text-text-primary">Lattice ran into a problem.</h1>
        <p className="mt-2 text-sm text-text-secondary">{userFriendlyMessage}</p>

        <div className="mt-6 flex items-center gap-2">
          <button type="button" onClick={resetError} className={secondaryButton}>
            Try again
          </button>
          <button type="button" onClick={() => window.location.reload()} className={secondaryButton}>
            Reload
          </button>
          <button type="button" onClick={handleCopyError} className={ghostButton}>
            {copied ? 'Copied' : 'Copy details'}
          </button>
          {isDevelopment ? (
            <button type="button" onClick={downloadErrorLog} className={ghostButton}>
              Download log
            </button>
          ) : null}
        </div>

        <div className="mt-8 border-t border-border-subtle pt-4">
          <button
            type="button"
            onClick={() => setShowDetails((value) => !value)}
            aria-expanded={showDetails}
            className="text-xs text-text-muted transition-colors duration-fast hover:text-text-primary"
          >
            {showDetails ? 'Hide details' : 'Show details'}
          </button>
          {showDetails ? (
            <div className="mt-3 space-y-3">
              <p className="font-mono text-xs text-text-secondary break-words">
                <span className="text-danger-fg">{error.name}</span> {displayMessage}
              </p>
              {isDevelopment && error.stack ? (
                <pre className="max-h-64 overflow-auto rounded-sm border border-border-subtle bg-surface p-3 font-mono text-xxs text-text-tertiary whitespace-pre-wrap">
                  {error.stack}
                </pre>
              ) : null}
              {isDevelopment && errorInfo?.componentStack ? (
                <pre className="max-h-64 overflow-auto rounded-sm border border-border-subtle bg-surface p-3 font-mono text-xxs text-text-tertiary whitespace-pre-wrap">
                  {errorInfo.componentStack}
                </pre>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
