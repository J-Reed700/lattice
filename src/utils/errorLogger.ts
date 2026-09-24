import { diagnostics } from './diagnostics';
/**
 * Error Logging Utility for Lattice/Lattice Application
 *
 * Compatibility facade for the local diagnostics store.
 * Includes privacy protection and sanitization.
 */


export interface ErrorLogEntry {
  message: string;
  stack?: string;
  componentStack?: string;
  timestamp: string;
  url: string;
  userAgent: string;
  errorType: string;
  level: 'error' | 'warning' | 'info';
  metadata?: Record<string, unknown>;
}

export interface ErrorContext {
  component?: string;
  action?: string;
  userId?: string;
  sessionId?: string;
  additionalData?: Record<string, unknown>;
}

class ErrorLogger {
  public logError(error: Error, context?: ErrorContext, level: 'error' | 'warning' | 'info' = 'error'): void {
    diagnostics.record(level === 'warning' ? 'warn' : level, error.message, context?.component ?? context?.action ?? 'Application', { error, context });
  }

  public getRecentErrors(): Array<{ message: string; type: string; timestamp: string }> {
    return diagnostics.getSnapshot().filter(entry => entry.level === 'error').map(entry => ({ message: entry.message, type: entry.source, timestamp: entry.timestamp }));
  }

  public getErrorQueue(): ErrorLogEntry[] {
    return diagnostics.getSnapshot().filter(entry => entry.level === 'error').map(entry => ({
      message: entry.message, timestamp: entry.timestamp, errorType: entry.source,
      level: 'error', url: '', userAgent: '', metadata: { details: entry.details },
    }));
  }

  public clearErrorQueue(): void { diagnostics.clear(); }

  /**
   * Log a component error (from Error Boundary)
   */
  public logComponentError(
    error: Error,
    errorInfo: React.ErrorInfo,
    componentName?: string
  ): void {
    this.logError(error, {
      component: componentName,
      action: 'render',
      additionalData: {
        componentStack: errorInfo.componentStack,
      },
    });
  }

  /**
   * Log a network error
   */
  public logNetworkError(
    error: Error,
    endpoint: string,
    statusCode?: number
  ): void {
    this.logError(error, {
      action: 'network_request',
      additionalData: {
        endpoint,
        statusCode,
      },
    });
  }

  /**
   * Log a database error
   */
  public logDatabaseError(
    error: Error,
    operation: string,
    table?: string
  ): void {
    this.logError(error, {
      action: 'database_operation',
      additionalData: {
        operation,
        table,
      },
    });
  }

  /**
   * Export error log for debugging
   */
  public exportErrorLog(): string {
    return diagnostics.export();
  }

  /**
   * Download error log as file
   */
  public downloadErrorLog(): void {
    const log = this.exportErrorLog();
    const blob = new Blob([log], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `lattice-error-log-${new Date().toISOString()}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  }
}

// Singleton instance
export const errorLogger = new ErrorLogger();

export const logError = errorLogger.logError.bind(errorLogger);
export const logComponentError = errorLogger.logComponentError.bind(errorLogger);
export const logNetworkError = errorLogger.logNetworkError.bind(errorLogger);
export const logDatabaseError = errorLogger.logDatabaseError.bind(errorLogger);
export const getRecentErrors = errorLogger.getRecentErrors.bind(errorLogger);
export const getErrorQueue = errorLogger.getErrorQueue.bind(errorLogger);
export const clearErrorQueue = errorLogger.clearErrorQueue.bind(errorLogger);
export const exportErrorLog = errorLogger.exportErrorLog.bind(errorLogger);
export const downloadErrorLog = errorLogger.downloadErrorLog.bind(errorLogger);
