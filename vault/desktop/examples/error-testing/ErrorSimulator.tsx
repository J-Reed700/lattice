/**
 * ErrorSimulator Component
 *
 * Development tool for testing error boundaries.
 * Provides buttons to trigger different error types.
 *
 * ONLY AVAILABLE IN DEVELOPMENT MODE
 */

import React, { useState } from 'react';
import { Bug, X } from 'lucide-react';
import {
  NetworkError,
  AuthenticationError,
  DatabaseError,
  RenderError,
  FileSystemError,
  SearchError,
} from '../../types/errors';

export function ErrorSimulator() {
  const [isOpen, setIsOpen] = useState(false);
  const [shouldThrow, setShouldThrow] = useState<Error | null>(null);

  // Only show in development
  const isDev = process.env.NODE_ENV === 'development';
  if (!isDev) {
    return null;
  }

  // Throw error if requested (for testing error boundaries)
  if (shouldThrow) {
    throw shouldThrow;
  }

  const triggerError = (error: Error) => {
    setShouldThrow(error);
  };

  const errorTypes = [
    {
      name: 'Render Error',
      description: 'Simulates a component rendering failure',
      error: new RenderError('Failed to render component', 'ErrorSimulator'),
    },
    {
      name: 'Network Error',
      description: 'Simulates an API/network failure',
      error: new NetworkError('Failed to fetch data', 500, '/api/test'),
    },
    {
      name: 'Authentication Error',
      description: 'Simulates an authentication failure',
      error: new AuthenticationError('Session expired'),
    },
    {
      name: 'Database Error',
      description: 'Simulates a database operation failure',
      error: new DatabaseError('Query failed', 'SELECT', 'documents'),
    },
    {
      name: 'File System Error',
      description: 'Simulates a file operation failure',
      error: new FileSystemError('Permission denied', 'read', '/path/to/file'),
    },
    {
      name: 'Search Error',
      description: 'Simulates a search operation failure',
      error: new SearchError('Invalid search query', 'test query'),
    },
    {
      name: 'Generic Error',
      description: 'Simulates a generic JavaScript error',
      error: new Error('Something went wrong unexpectedly'),
    },
    {
      name: 'Async Error',
      description: 'Simulates an asynchronous error (Promise rejection)',
      error: new Error('Async operation failed'),
      async: true,
    },
  ];

  if (!isOpen) {
    return (
      <div className="fixed bottom-4 right-4 z-50">
        <button
          onClick={() => setIsOpen(true)}
          className="flex items-center gap-2 px-4 py-2 bg-[var(--warning)] hover:bg-[var(--warning)] text-yellow-950 rounded-lg shadow-lg font-medium transition-colors"
          title="Open Error Simulator (Dev Only)"
        >
          <Bug className="w-5 h-5" />
          Error Simulator
        </button>
      </div>
    );
  }

  return (
    <div className="fixed bottom-4 right-4 z-50 w-96 bg-[var(--surface-elevated)] rounded-lg shadow-2xl border border-[var(--border-color)] overflow-hidden">
      {/* Header */}
      <div className="bg-[var(--warning)] text-yellow-950 px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Bug className="w-5 h-5" />
          <h3 className="font-semibold">Error Simulator</h3>
        </div>
        <button
          onClick={() => setIsOpen(false)}
          className="p-1 hover:bg-[var(--warning)] rounded transition-colors"
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      {/* Content */}
      <div className="p-4 max-h-96 overflow-y-auto">
        <p className="text-sm text-[var(--text-secondary)] mb-4">
          Click a button to trigger an error and test error boundaries:
        </p>

        <div className="space-y-2">
          {errorTypes.map((errorType, index) => (
            <button
              key={index}
              onClick={() => {
                if (errorType.async) {
                  // Trigger async error
                  Promise.reject(errorType.error);
                } else {
                  // Trigger sync error
                  triggerError(errorType.error);
                }
              }}
              className="w-full text-left p-3 bg-[var(--surface-base)] hover:bg-[var(--surface-hover)] border border-[var(--border-color)] rounded-lg transition-colors"
            >
              <div className="flex items-start justify-between gap-2">
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-semibold text-[var(--text-primary)] mb-1">
                    {errorType.name}
                  </p>
                  <p className="text-xs text-[var(--text-secondary)]">
                    {errorType.description}
                  </p>
                </div>
                {errorType.async && (
                  <span className="flex-shrink-0 px-2 py-0.5 text-xs font-medium bg-purple-500/20 text-purple-600 rounded">
                    Async
                  </span>
                )}
              </div>
            </button>
          ))}
        </div>

        {/* Warning */}
        <div className="mt-4 p-3 bg-orange-500/10 border border-orange-500/20 rounded-lg">
          <p className="text-xs text-orange-600">
            <strong>Warning:</strong> Triggering errors will test your error boundaries.
            The app may crash if boundaries are not properly configured.
          </p>
        </div>
      </div>
    </div>
  );
}

/**
 * Component that throws error after mount (for testing)
 */
export function DelayedErrorComponent({ delayMs = 1000 }: { delayMs?: number }) {
  const [shouldThrow, setShouldThrow] = useState(false);

  React.useEffect(() => {
    const timer = setTimeout(() => {
      setShouldThrow(true);
    }, delayMs);

    return () => clearTimeout(timer);
  }, [delayMs]);

  if (shouldThrow) {
    throw new Error(`Delayed error after ${delayMs}ms`);
  }

  return <div>Loading... (will error in {delayMs}ms)</div>;
}

/**
 * Component that throws error on button click
 */
export function ClickToErrorComponent() {
  const [shouldThrow, setShouldThrow] = useState(false);

  if (shouldThrow) {
    throw new Error('Error triggered by button click');
  }

  return (
    <button
      onClick={() => setShouldThrow(true)}
      className="px-4 py-2 bg-[var(--error)] hover:bg-[var(--error)] text-white rounded-lg font-medium transition-colors"
    >
      Click to Trigger Error
    </button>
  );
}
