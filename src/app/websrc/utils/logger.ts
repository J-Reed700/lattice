/**
 * Frontend Logger Utility
 *
 * Provides structured logging with different log levels and context.
 * Automatically disabled in production builds unless explicitly enabled.
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogContext {
  component?: string;
  action?: string;
  [key: string]: unknown;
}

class Logger {
  private enabled: boolean;
  private logLevel: LogLevel;

  constructor() {
    this.enabled = import.meta.env.DEV || import.meta.env.VITE_ENABLE_LOGGING === 'true';
    this.logLevel = (import.meta.env.VITE_LOG_LEVEL as LogLevel) || 'debug';
  }

  private shouldLog(level: LogLevel): boolean {
    if (!this.enabled) return false;

    const levels: LogLevel[] = ['debug', 'info', 'warn', 'error'];
    const currentLevelIndex = levels.indexOf(this.logLevel);
    const messageLevelIndex = levels.indexOf(level);

    return messageLevelIndex >= currentLevelIndex;
  }

  private formatMessage(level: LogLevel, message: string, context?: LogContext): string {
    const timestamp = new Date().toISOString();
    const emoji = {
      debug: '🔍',
      info: 'ℹ️',
      warn: '⚠️',
      error: '❌',
    };

    let formatted = `${emoji[level]} [${timestamp}] ${level.toUpperCase()}`;

    if (context?.component) {
      formatted += ` [${context.component}]`;
    }

    if (context?.action) {
      formatted += ` [${context.action}]`;
    }

    formatted += `: ${message}`;

    return formatted;
  }

  private logWithContext(
    level: LogLevel,
    message: string,
    context?: LogContext,
    ...args: unknown[]
  ): void {
    if (!this.shouldLog(level)) return;

    const formatted = this.formatMessage(level, message, context);

    // Extract context for structured logging
    const { component, action, ...additionalContext } = context || {};
    const hasAdditionalContext = Object.keys(additionalContext).length > 0;

    // Avoid unused variable warnings
    void component;
    void action;

    switch (level) {
      case 'debug':
        console.debug(formatted, hasAdditionalContext ? additionalContext : '', ...args);
        break;
      case 'info':
        console.info(formatted, hasAdditionalContext ? additionalContext : '', ...args);
        break;
      case 'warn':
        console.warn(formatted, hasAdditionalContext ? additionalContext : '', ...args);
        break;
      case 'error':
        console.error(formatted, hasAdditionalContext ? additionalContext : '', ...args);
        break;
    }
  }

  /**
   * Log debug message (development only)
   */
  debug(message: string, context?: LogContext, ...args: unknown[]): void {
    this.logWithContext('debug', message, context, ...args);
  }

  /**
   * Log informational message
   */
  info(message: string, context?: LogContext, ...args: unknown[]): void {
    this.logWithContext('info', message, context, ...args);
  }

  /**
   * Log warning message
   */
  warn(message: string, context?: LogContext, ...args: unknown[]): void {
    this.logWithContext('warn', message, context, ...args);
  }

  /**
   * Log error message
   */
  error(message: string, context?: LogContext, error?: Error, ...args: unknown[]): void {
    const errorContext = error
      ? { ...context, error: error.message, stack: error.stack }
      : context;
    this.logWithContext('error', message, errorContext, ...args);
  }

  /**
   * Create a scoped logger for a specific component
   */
  scope(component: string): ScopedLogger {
    return new ScopedLogger(this, component);
  }

  /**
   * Enable/disable logging at runtime
   */
  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
  }

  /**
   * Set log level at runtime
   */
  setLogLevel(level: LogLevel): void {
    this.logLevel = level;
  }

  /**
   * Get current log level
   */
  getLogLevel(): LogLevel {
    return this.logLevel;
  }

  /**
   * Check if logging is enabled
   */
  isEnabled(): boolean {
    return this.enabled;
  }
}

/**
 * Scoped logger for a specific component
 */
class ScopedLogger {
  constructor(
    private logger: Logger,
    private component: string
  ) {}

  debug(message: string, context?: Omit<LogContext, 'component'>, ...args: unknown[]): void {
    this.logger.debug(message, { component: this.component, ...context }, ...args);
  }

  info(message: string, context?: Omit<LogContext, 'component'>, ...args: unknown[]): void {
    this.logger.info(message, { component: this.component, ...context }, ...args);
  }

  warn(message: string, context?: Omit<LogContext, 'component'>, ...args: unknown[]): void {
    this.logger.warn(message, { component: this.component, ...context }, ...args);
  }

  error(
    message: string,
    context?: Omit<LogContext, 'component'>,
    error?: Error,
    ...args: unknown[]
  ): void {
    this.logger.error(message, { component: this.component, ...context }, error, ...args);
  }
}

// Export singleton instance
export const logger = new Logger();

// Export convenience function for creating scoped loggers
export function createLogger(component: string): ScopedLogger {
  return logger.scope(component);
}

