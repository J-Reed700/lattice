import { createContext, useContext, useCallback, type ReactNode } from 'react';

import { useErrorToast, type ToastError, type ErrorSeverity } from '../components/ErrorToast';
import { toError } from '../lib/typeGuards';
import { createLogger } from '../utils/logger';

const logger = createLogger('ErrorContext');

interface ErrorContextType {
  showError: (_message: string, _options?: ErrorOptions) => string;
  dismissError: (_id: string) => void;
  clearAll: () => void;
  handleTauriError: (_error: string, _options?: ErrorOptions) => void;
  errors: ToastError[];
}

interface ErrorOptions {
  severity?: ErrorSeverity;
  duration?: number;
  action?: {
    label: string;
    onClick: () => void;
  };
  recoverable?: boolean;
}

const ErrorContext = createContext<ErrorContextType | undefined>(undefined);

interface ErrorProviderProps {
  children: ReactNode;
}

export function ErrorProvider({ children }: ErrorProviderProps) {
  const { errors, showError: showToastError, dismissError, clearAll } = useErrorToast();

  const showError = useCallback(
    (message: string, options?: ErrorOptions) => {
      logger.error(message, { action: 'showError', severity: options?.severity });
      return showToastError(message, options);
    },
    [showToastError]
  );

  const handleTauriError = useCallback(
    (error: string, options?: ErrorOptions) => {
      const errorMessage = parseErrorMessage(error);
      const severity = determineSeverity(error);
      const recoverable = isRecoverable(error);

      showError(errorMessage, {
        severity: options?.severity ?? severity,
        recoverable: options?.recoverable ?? recoverable,
        duration: options?.duration,
        action: options?.action ?? getSuggestedAction(error),
      });
    },
    [showError]
  );

  const value: ErrorContextType = {
    showError,
    dismissError,
    clearAll,
    handleTauriError,
    errors,
  };

  return <ErrorContext.Provider value={value}>{children}</ErrorContext.Provider>;
}

export function useError() {
  const context = useContext(ErrorContext);
  if (!context) {
    throw new Error('useError must be used within an ErrorProvider');
  }
  return context;
}

function parseErrorMessage(error: string): string {
  if (error.includes('Permission denied')) {
    return error.split('\n')[0];
  }

  if (error.includes('File not found')) {
    return error.split('\n')[0];
  }

  if (error.includes('Network')) {
    return 'Network error. Please check your internet connection.';
  }

  if (error.includes('Database')) {
    return 'Database error. Please try restarting the application.';
  }

  return error.split('\n')[0] || error;
}

function determineSeverity(error: string): ErrorSeverity {
  if (
    error.includes('corrupt') ||
    error.includes('fatal') ||
    error.includes('Database is corrupted')
  ) {
    return 'error';
  }

  if (
    error.includes('timeout') ||
    error.includes('Permission denied') ||
    error.includes('Unsupported')
  ) {
    return 'warning';
  }

  return 'info';
}

function isRecoverable(error: string): boolean {
  const recoverablePatterns = [
    'Network',
    'timeout',
    'queue is full',
    'Connection',
    'Download failed',
  ];

  return recoverablePatterns.some((pattern) =>
    error.toLowerCase().includes(pattern.toLowerCase())
  );
}

function getSuggestedAction(error: string): ToastError['action'] | undefined {
  if (error.includes('Permission denied')) {
    return {
      label: 'Learn More',
      onClick: () => {
        // Open external documentation for permission errors
        window.open('https://github.com/yourusername/lattice-desktop/wiki/Troubleshooting#permission-errors', '_blank');
      },
    };
  }

  if (error.includes('Model') && error.includes('not found')) {
    return {
      label: 'Download Model',
      onClick: () => {
        // Navigate to settings page where models can be downloaded
        // Using custom event since we don't have direct access to navigation
        window.dispatchEvent(new CustomEvent('navigate', { detail: { view: 'settings', section: 'models' } }));
      },
    };
  }

  if (error.includes('Network') || error.includes('timeout')) {
    return {
      label: 'Retry',
      onClick: () => {
        window.location.reload();
      },
    };
  }

  if (error.includes('queue is full')) {
    return {
      label: 'View Queue',
      onClick: () => {
        // Navigate to files view which shows indexing progress
        window.dispatchEvent(new CustomEvent('navigate', { detail: { view: 'files' } }));
      },
    };
  }

  return undefined;
}

export function withErrorHandling<
  TArgs extends unknown[],
  TReturn
>(
  fn: (..._args: TArgs) => Promise<TReturn>,
  errorHandler?: (_error: Error) => void
): (..._args: TArgs) => Promise<TReturn> {
  return async (...args: TArgs) => {
    try {
      return await fn(...args);
    } catch (error) {
      const err = toError(error);
      if (errorHandler) {
        errorHandler(err);
      } else {
        logger.error('Unhandled error in withErrorHandling wrapper', {}, err);
      }
      throw err;
    }
  };
}
