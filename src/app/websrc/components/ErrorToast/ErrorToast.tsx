import { useEffect, useState } from 'react';

export type ErrorSeverity = 'error' | 'warning' | 'info';

export interface ToastError {
  id: string;
  message: string;
  severity: ErrorSeverity;
  action?: {
    label: string;
    onClick: () => void;
  };
  duration?: number;
  recoverable?: boolean;
}

interface ErrorToastProps {
  error: ToastError;
  onDismiss: (id: string) => void;
}

const severityStyles = {
  error: {
    bg: 'bg-[var(--error-light)]/20 border-[var(--error-light)]',
    icon: 'text-[var(--error)]',
    text: 'text-[var(--error)]',
  },
  warning: {
    bg: 'bg-[var(--warning-light)]/20 border-[var(--warning-light)]',
    icon: 'text-[var(--warning)]',
    text: 'text-[var(--warning)]',
  },
  info: {
    bg: 'bg-[var(--accent-light)]/20 border-[var(--accent-light)]',
    icon: 'text-[var(--accent-primary)]',
    text: 'text-[var(--accent-primary)]',
  },
};

const severityIcons = {
  error: (
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
    />
  ),
  warning: (
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
    />
  ),
  info: (
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
    />
  ),
};

export function ErrorToast({ error, onDismiss }: ErrorToastProps) {
  const [progress, setProgress] = useState(100);
  const [isVisible, setIsVisible] = useState(false);
  const styles = severityStyles[error.severity];
  const duration = error.duration ?? 5000;

  // Trigger entrance animation
  useEffect(() => {
    setTimeout(() => setIsVisible(true), 10);
  }, []);

  useEffect(() => {
    if (duration === 0) return;

    const startTime = Date.now();
    const interval = setInterval(() => {
      const elapsed = Date.now() - startTime;
      const remaining = Math.max(0, 100 - (elapsed / duration) * 100);
      setProgress(remaining);

      if (remaining === 0) {
        onDismiss(error.id);
      }
    }, 50);

    return () => clearInterval(interval);
  }, [duration, error.id, onDismiss]);

  return (
    <div
      className={`max-w-md w-full border rounded-lg shadow-lg overflow-hidden transition-all duration-200 ease-out ${styles.bg} ${
        isVisible ? 'opacity-100 translate-y-0 scale-100' : 'opacity-0 -translate-y-5 scale-95'
      }`}
    >
      <div className="p-4">
        <div className="flex items-start gap-3">
          <div className="flex-shrink-0">
            <svg
              className={`w-5 h-5 ${styles.icon}`}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              {severityIcons[error.severity]}
            </svg>
          </div>

          <div className="flex-1 min-w-0">
            <p className={`text-sm font-medium ${styles.text}`}>{error.message}</p>

            {error.action && (
              <button
                onClick={() => {
                  error.action?.onClick();
                  onDismiss(error.id);
                }}
                className={`mt-2 text-sm font-medium ${styles.icon} hover:underline`}
              >
                {error.action.label}
              </button>
            )}
          </div>

          <button
            onClick={() => onDismiss(error.id)}
            className={`flex-shrink-0 ${styles.icon} hover:opacity-70 transition-opacity`}
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>
      </div>

      {duration > 0 && (
        <div className="h-1 bg-[var(--bg-tertiary)]">
          <div
            className={`h-full transition-all duration-50 ease-linear ${
              error.severity === 'error'
                ? 'bg-[var(--error)]'
                : error.severity === 'warning'
                ? 'bg-[var(--warning)]'
                : 'bg-[var(--accent-primary)]'
            }`}
            style={{ width: `${progress}%` }}
          />
        </div>
      )}
    </div>
  );
}

interface ErrorToastContainerProps {
  errors: ToastError[];
  onDismiss: (id: string) => void;
  position?: 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left' | 'top-center';
}

const positionStyles = {
  'top-right': 'top-4 right-4',
  'top-left': 'top-4 left-4',
  'bottom-right': 'bottom-4 right-4',
  'bottom-left': 'bottom-4 left-4',
  'top-center': 'top-4 left-1/2 -translate-x-1/2',
};

export function ErrorToastContainer({
  errors,
  onDismiss,
  position = 'top-right',
}: ErrorToastContainerProps) {
  return (
    <div className={`fixed ${positionStyles[position]} z-50 flex flex-col gap-2`}>
      {errors.map((error) => (
        <ErrorToast key={error.id} error={error} onDismiss={onDismiss} />
      ))}
    </div>
  );
}

export function useErrorToast() {
  const [errors, setErrors] = useState<ToastError[]>([]);

  const showError = (
    message: string,
    options?: {
      severity?: ErrorSeverity;
      duration?: number;
      action?: ToastError['action'];
      recoverable?: boolean;
    }
  ) => {
    const error: ToastError = {
      id: `${Date.now()}-${Math.random()}`,
      message,
      severity: options?.severity ?? 'error',
      duration: options?.duration,
      action: options?.action,
      recoverable: options?.recoverable,
    };

    setErrors((prev) => [...prev, error]);
    return error.id;
  };

  const dismissError = (id: string) => {
    setErrors((prev) => prev.filter((error) => error.id !== id));
  };

  const clearAll = () => {
    setErrors([]);
  };

  return {
    errors,
    showError,
    dismissError,
    clearAll,
  };
}
