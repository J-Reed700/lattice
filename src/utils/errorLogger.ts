/**
 * Error Logging Utility for Lattice/Lattice Application
 *
 * Handles error logging to console (development) and external services (production).
 * Includes privacy protection and sanitization.
 */

import { sanitizeErrorMessage, isCustomError } from '../types/errors';

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
  private isDevelopment: boolean;
  private errorQueue: ErrorLogEntry[] = [];
  private maxQueueSize = 50;

  constructor() {
    this.isDevelopment = import.meta.env.DEV;
    this.initializeSentry();
  }

  /**
   * Initialize Sentry or other error tracking service
   * Currently a placeholder for future integration
   */
  private initializeSentry(): void {
    if (this.isDevelopment) {
      console.log('[ErrorLogger] Running in development mode - errors will be logged to console');
      return;
    }

    // TODO: Initialize Sentry
    // Sentry.init({
    //   dsn: 'YOUR_SENTRY_DSN',
    //   environment: import.meta.env.MODE,
    //   beforeSend(event) {
    //     // Sanitize sensitive data before sending
    //     return event;
    //   },
    // });
  }

  /**
   * Log an error with full context
   */
  public logError(
    error: Error,
    context?: ErrorContext,
    level: 'error' | 'warning' | 'info' = 'error'
  ): void {
    const logEntry: ErrorLogEntry = {
      message: this.isDevelopment ? error.message : sanitizeErrorMessage(error.message),
      stack: this.isDevelopment ? error.stack : undefined,
      componentStack: context?.component,
      timestamp: new Date().toISOString(),
      url: window.location.href,
      userAgent: navigator.userAgent,
      errorType: error.name,
      level,
      metadata: {
        ...context?.additionalData,
        customError: isCustomError(error),
      },
    };

    this.addToQueue(logEntry);

    // Console logging
    this.logToConsole(error, logEntry, context);

    // Send to external service (production only)
    if (!this.isDevelopment) {
      this.sendToExternalService(error, logEntry, context);
    }
  }

  /**
   * Add error to local queue for debugging
   */
  private addToQueue(entry: ErrorLogEntry): void {
    this.errorQueue.push(entry);
    if (this.errorQueue.length > this.maxQueueSize) {
      this.errorQueue.shift();
    }
  }

  /**
   * Log error to console with formatting
   */
  private logToConsole(error: Error, entry: ErrorLogEntry, context?: ErrorContext): void {
    const style = 'color: #ef4444; font-weight: bold;';

    console.group(`%c[${entry.level.toUpperCase()}] ${entry.errorType}`, style);
    console.error('Message:', error.message);

    if (error.stack) {
      console.error('Stack:', error.stack);
    }

    if (context) {
      console.info('Context:', context);
    }

    if (entry.metadata) {
      console.info('Metadata:', entry.metadata);
    }

    console.info('Timestamp:', entry.timestamp);
    console.groupEnd();
  }

  /**
   * Send error to external tracking service
   */
  private sendToExternalService(
    _error: Error,
    entry: ErrorLogEntry,
    _context?: ErrorContext
  ): void {
    // TODO: Send to Sentry
    // Sentry.captureException(error, {
    //   level: entry.level,
    //   contexts: {
    //     app: {
    //       component: context?.component,
    //       action: context?.action,
    //     },
    //   },
    //   extra: entry.metadata,
    // });

    // For now, just store in localStorage for debugging
    try {
      const recentErrors = this.getRecentErrors();
      recentErrors.push({
        message: entry.message,
        type: entry.errorType,
        timestamp: entry.timestamp,
      });

      // Keep last 20 errors
      const trimmed = recentErrors.slice(-20);
      localStorage.setItem('recall_error_log', JSON.stringify(trimmed));
    } catch (e) {
      // Ignore localStorage errors
      console.warn('Failed to store error in localStorage:', e);
    }
  }

  /**
   * Get recent errors from localStorage
   */
  public getRecentErrors(): Array<{ message: string; type: string; timestamp: string }> {
    try {
      const stored = localStorage.getItem('recall_error_log');
      return stored ? JSON.parse(stored) : [];
    } catch {
      return [];
    }
  }

  /**
   * Get errors from in-memory queue
   */
  public getErrorQueue(): ErrorLogEntry[] {
    return [...this.errorQueue];
  }

  /**
   * Clear error queue
   */
  public clearErrorQueue(): void {
    this.errorQueue = [];
  }

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
    const log = {
      queue: this.errorQueue,
      recent: this.getRecentErrors(),
      timestamp: new Date().toISOString(),
    };
    return JSON.stringify(log, null, 2);
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
