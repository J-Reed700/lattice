/**
 * Enhanced ErrorBoundary Component
 *
 * A robust error boundary with:
 * - Reset keys for automatic error recovery
 * - Custom fallback support
 * - Error logging integration
 * - Lifecycle callbacks
 */

import React, { Component, type ErrorInfo, type ReactNode } from 'react';

import { logComponentError } from '../../utils/errorLogger';

export interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: React.ComponentType<ErrorFallbackProps>;
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
  onReset?: () => void;
  resetKeys?: unknown[];
  resetOnPropsChange?: boolean;
  isolate?: boolean;
  name?: string;
}

export interface ErrorFallbackProps {
  error: Error;
  errorInfo?: ErrorInfo;
  resetError: () => void;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
  errorCount: number;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  private resetTimeoutId: NodeJS.Timeout | null = null;
  private previousResetKeys: unknown[] = [];

  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = {
      hasError: false,
      error: null,
      errorInfo: null,
      errorCount: 0,
    };

    if (props.resetKeys) {
      this.previousResetKeys = [...props.resetKeys];
    }
  }

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    const { onError, name } = this.props;

    this.setState((prevState) => ({
      error,
      errorInfo,
      errorCount: prevState.errorCount + 1,
    }));

    // Log error
    logComponentError(error, errorInfo, name);

    if (onError) {
      try {
        onError(error, errorInfo);
      } catch (err) {
        console.error('Error in onError handler:', err);
      }
    }

    // Prevent infinite error loops
    if (this.state.errorCount > 5) {
      console.error(
        'ErrorBoundary: Too many errors detected. Preventing further resets to avoid infinite loop.'
      );
    }
  }

  componentDidUpdate(prevProps: ErrorBoundaryProps): void {
    const { resetKeys, resetOnPropsChange } = this.props;
    const { hasError } = this.state;

    // Auto-reset on resetKeys change
    if (hasError && resetKeys) {
      const hasResetKeysChanged = resetKeys.some(
        (key, index) => key !== this.previousResetKeys[index]
      );

      if (hasResetKeysChanged) {
        this.previousResetKeys = [...resetKeys];
        this.reset();
      }
    }

    // Auto-reset on any props change
    if (hasError && resetOnPropsChange && prevProps !== this.props) {
      this.reset();
    }
  }

  componentWillUnmount(): void {
    if (this.resetTimeoutId) {
      clearTimeout(this.resetTimeoutId);
    }
  }

  reset = (): void => {
    const { onReset } = this.props;
    const { errorCount } = this.state;

    // Prevent infinite reset loops
    if (errorCount > 5) {
      console.warn('ErrorBoundary: Maximum error count reached. Not resetting.');
      return;
    }

    if (onReset) {
      try {
        onReset();
      } catch (resetError) {
        console.error('Error in onReset handler:', resetError);
      }
    }

    this.setState({
      hasError: false,
      error: null,
      errorInfo: null,
    });
  };

  /**
   * Automatically retry after a delay
   */
  scheduleReset(delayMs: number = 3000): void {
    if (this.resetTimeoutId) {
      clearTimeout(this.resetTimeoutId);
    }

    this.resetTimeoutId = setTimeout(() => {
      this.reset();
    }, delayMs);
  }

  render(): ReactNode {
    const { hasError, error, errorInfo } = this.state;
    const { children, fallback: FallbackComponent, isolate } = this.props;

    if (hasError) {
      const fallbackError = error ?? new Error('Unknown error');

      // Use custom fallback if provided
      if (FallbackComponent) {
        return (
          <FallbackComponent
            error={fallbackError}
            errorInfo={errorInfo || undefined}
            resetError={this.reset}
          />
        );
      }

      // Default fallback with isolation option
      if (isolate) {
        return (
          <div className="p-4 bg-[hsl(var(--danger-muted))] border-l-4 border-[hsl(var(--danger-fg))] rounded">
            <p className="text-sm font-semibold text-[hsl(var(--danger-fg))] mb-2">
              Error in {this.props.name || 'component'}
            </p>
            <p className="text-xs text-[hsl(var(--danger-fg))] mb-3">
              {fallbackError.message}
            </p>
            <button
              onClick={this.reset}
              className="px-3 py-1 text-sm font-medium text-[hsl(var(--danger-fg))] bg-[hsl(var(--danger-muted))] hover:opacity-90 rounded transition-opacity duration-fast"
            >
              Retry
            </button>
          </div>
        );
      }

      // Fallback to minimal error display
      return null;
    }

    return children;
  }
}

/**
 * Hook-based error boundary wrapper
 */
export function withErrorBoundary<P extends object>(
  Component: React.ComponentType<P>,
  errorBoundaryProps?: Omit<ErrorBoundaryProps, 'children'>
): React.FC<P> {
  const WrappedComponent: React.FC<P> = (props) => (
    <ErrorBoundary {...errorBoundaryProps}>
      <Component {...props} />
    </ErrorBoundary>
  );

  WrappedComponent.displayName = `withErrorBoundary(${Component.displayName || Component.name || 'Component'})`;

  return WrappedComponent;
}
