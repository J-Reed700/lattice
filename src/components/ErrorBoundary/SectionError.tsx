/**
 * SectionError — inline fallback for a failed surface (Search, Library, …).
 * The rest of the app keeps working; this says so in one line and offers
 * a retry.
 */

import React, { useState } from 'react';

import { getUserFriendlyMessage } from '../../types/errors';

export interface SectionErrorProps {
  error: Error;
  errorInfo?: React.ErrorInfo;
  resetError: () => void;
  sectionName?: string;
  icon?: React.ReactNode;
  onDismiss?: () => void;
}

export function SectionError({ error, errorInfo, resetError, sectionName = 'this section', onDismiss }: SectionErrorProps) {
  const [showDetails, setShowDetails] = useState(false);
  const isDevelopment = import.meta.env.DEV;
  const userFriendlyMessage = getUserFriendlyMessage(error);
  const hasTechnicalDetails = isDevelopment && Boolean(error.stack || errorInfo?.componentStack);

  return (
    <div className="flex min-h-[320px] items-center justify-center p-8">
      <div className="w-full max-w-md text-center">
        <p className="text-sm text-text-secondary">Couldn't load {sectionName}.</p>
        <p className="mt-1 text-xs text-text-muted break-words">{userFriendlyMessage}</p>

        <div className="mt-4 flex items-center justify-center gap-2">
          <button
            type="button"
            onClick={resetError}
            className="inline-flex h-8 items-center rounded-md border border-border-default bg-surface px-3 text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-surface-raised"
          >
            Try again
          </button>
          {onDismiss ? (
            <button
              type="button"
              onClick={onDismiss}
              className="inline-flex h-8 items-center rounded-md px-2 text-sm text-text-secondary transition-colors duration-fast hover:bg-surface-raised hover:text-text-primary"
            >
              Dismiss
            </button>
          ) : null}
        </div>

        {hasTechnicalDetails ? (
          <div className="mt-6 text-left">
            <button
              type="button"
              onClick={() => setShowDetails((value) => !value)}
              aria-expanded={showDetails}
              className="text-xs text-text-muted transition-colors duration-fast hover:text-text-primary"
            >
              {showDetails ? 'Hide details' : 'Show details'}
            </button>
            {showDetails ? (
              <div className="mt-2 space-y-2">
                <p className="font-mono text-xs text-text-secondary break-words">
                  <span className="text-danger-fg">{error.name}</span> {error.message}
                </p>
                {error.stack ? (
                  <pre className="max-h-48 overflow-auto rounded-sm border border-border-subtle bg-surface p-3 font-mono text-xxs text-text-tertiary whitespace-pre-wrap">
                    {error.stack}
                  </pre>
                ) : null}
                {errorInfo?.componentStack ? (
                  <pre className="max-h-48 overflow-auto rounded-sm border border-border-subtle bg-surface p-3 font-mono text-xxs text-text-tertiary whitespace-pre-wrap">
                    {errorInfo.componentStack}
                  </pre>
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
