/**
 * Custom Error Types for Lattice/Lattice Application
 *
 * These error types enable specific error handling strategies
 * based on the nature of the failure.
 */

/**
 * Network-related errors (API calls, fetch failures)
 */
export class NetworkError extends Error {
  public readonly statusCode?: number;
  public readonly endpoint?: string;

  constructor(message: string, statusCode?: number, endpoint?: string) {
    super(message);
    this.name = 'NetworkError';
    this.statusCode = statusCode;
    this.endpoint = endpoint;
    Object.setPrototypeOf(this, NetworkError.prototype);
  }
}

/**
 * Authentication and authorization errors
 */
export class AuthenticationError extends Error {
  constructor(message: string = 'Authentication required') {
    super(message);
    this.name = 'AuthenticationError';
    Object.setPrototypeOf(this, AuthenticationError.prototype);
  }
}

/**
 * Database operation errors
 */
export class DatabaseError extends Error {
  public readonly operation?: string;
  public readonly table?: string;

  constructor(message: string, operation?: string, table?: string) {
    super(message);
    this.name = 'DatabaseError';
    this.operation = operation;
    this.table = table;
    Object.setPrototypeOf(this, DatabaseError.prototype);
  }
}

/**
 * Component rendering errors
 */
export class RenderError extends Error {
  public readonly component?: string;

  constructor(message: string, component?: string) {
    super(message);
    this.name = 'RenderError';
    this.component = component;
    Object.setPrototypeOf(this, RenderError.prototype);
  }
}

/**
 * File system operation errors
 */
export class FileSystemError extends Error {
  public readonly path?: string;
  public readonly operation?: 'read' | 'write' | 'delete' | 'list';

  constructor(message: string, operation?: 'read' | 'write' | 'delete' | 'list', path?: string) {
    super(message);
    this.name = 'FileSystemError';
    this.operation = operation;
    this.path = path;
    Object.setPrototypeOf(this, FileSystemError.prototype);
  }
}

/**
 * Search and indexing errors
 */
export class SearchError extends Error {
  public readonly query?: string;

  constructor(message: string, query?: string) {
    super(message);
    this.name = 'SearchError';
    this.query = query;
    Object.setPrototypeOf(this, SearchError.prototype);
  }
}

/**
 * Configuration and settings errors
 */
export class ConfigurationError extends Error {
  public readonly setting?: string;

  constructor(message: string, setting?: string) {
    super(message);
    this.name = 'ConfigurationError';
    this.setting = setting;
    Object.setPrototypeOf(this, ConfigurationError.prototype);
  }
}

/**
 * Type guard to check if error is a custom error type
 */
export function isCustomError(error: unknown): error is NetworkError | AuthenticationError | DatabaseError | RenderError | FileSystemError | SearchError | ConfigurationError {
  return error instanceof Error && (
    error instanceof NetworkError ||
    error instanceof AuthenticationError ||
    error instanceof DatabaseError ||
    error instanceof RenderError ||
    error instanceof FileSystemError ||
    error instanceof SearchError ||
    error instanceof ConfigurationError
  );
}

/**
 * Get user-friendly error message based on error type
 */
export function getUserFriendlyMessage(error: Error): string {
  if (error instanceof NetworkError) {
    return 'Unable to connect to the service. Please check your internet connection.';
  }

  if (error instanceof AuthenticationError) {
    return 'Authentication required. Please sign in to continue.';
  }

  if (error instanceof DatabaseError) {
    return 'Database operation failed. Your data is safe, but this action could not be completed.';
  }

  if (error instanceof FileSystemError) {
    return 'File operation failed. Please check file permissions and try again.';
  }

  if (error instanceof SearchError) {
    return 'Search operation failed. Please try a different query.';
  }

  if (error instanceof ConfigurationError) {
    return 'Configuration error detected. Please check your settings.';
  }

  // Generic fallback
  return 'An unexpected error occurred. Our team has been notified.';
}

/**
 * Sanitize error message for production (remove sensitive data)
 */
export function sanitizeErrorMessage(message: string): string {
  // Remove file paths
  let sanitized = message.replace(/[A-Z]:[\\/ ][\w\s\-\\.]+/gi, '[PATH]');
  sanitized = sanitized.replace(/\/[\w\s\-\\.]+/gi, '[PATH]');

  // Remove potential tokens/keys
  sanitized = sanitized.replace(/[a-f0-9]{32,}/gi, '[TOKEN]');

  // Remove email addresses
  sanitized = sanitized.replace(/[\w.-]+@[\w.-]+\.\w+/gi, '[EMAIL]');

  // Remove IP addresses
  sanitized = sanitized.replace(/\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b/g, '[IP]');

  return sanitized;
}
