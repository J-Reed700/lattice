/**
 * Centralized Error Handling
 *
 * Utilities for parsing, handling, and converting API errors.
 * Provides consistent error handling across the frontend.
 */

import { ErrorCode } from '../types/api/errorCodes';
import { type ApiError } from '../types/api/result';

/**
 * User-friendly error messages mapped to error codes
 * SYNCHRONIZED WITH RUST BACKEND - All 42 error codes
 */
const USER_MESSAGES: Record<ErrorCode, string> = {
  // Resource Errors
  [ErrorCode.NOT_FOUND]: 'The requested resource was not found.',
  [ErrorCode.ALREADY_EXISTS]: 'This resource already exists.',
  [ErrorCode.GONE]: 'This resource has been deleted or is no longer available.',

  // Permission & Security Errors
  [ErrorCode.PERMISSION_DENIED]: 'You do not have permission to perform this action.',
  [ErrorCode.UNAUTHORIZED]: 'Authentication failed. Please log in again.',
  [ErrorCode.SECURITY_VIOLATION]: 'Security violation detected. Action blocked.',

  // Validation Errors
  [ErrorCode.VALIDATION_ERROR]: 'The provided data is invalid.',
  [ErrorCode.INVALID_INPUT]: 'Invalid input provided.',
  [ErrorCode.INVALID_CONFIG]: 'Configuration error. Please check settings.',
  [ErrorCode.INVALID_STATE]: 'Operation cannot be performed in the current state.',
  [ErrorCode.CONSTRAINT_VIOLATION]: 'Operation violates data constraints.',

  // File System Errors
  [ErrorCode.FILE_NOT_FOUND]: 'File not found.',
  [ErrorCode.FILE_TOO_LARGE]: 'File is too large to process.',
  [ErrorCode.UNSUPPORTED_FILE_TYPE]: 'File type is not supported.',
  [ErrorCode.FILE_SYSTEM_ERROR]: 'File system error. Please check permissions.',
  [ErrorCode.FILE_READ_ERROR]: 'Failed to read file.',
  [ErrorCode.FILE_WRITE_ERROR]: 'Failed to write file.',

  // Database Errors
  [ErrorCode.DATABASE_ERROR]: 'Database operation failed. Please try again.',
  [ErrorCode.DATABASE_CONNECTION_ERROR]: 'Failed to connect to database.',
  [ErrorCode.MIGRATION_ERROR]: 'Database migration failed.',

  // Network Errors
  [ErrorCode.NETWORK_ERROR]: 'Network error. Please check your connection.',
  [ErrorCode.TIMEOUT]: 'Operation timed out. Please try again.',
  [ErrorCode.RATE_LIMIT_EXCEEDED]: 'Too many requests. Please try again later.',

  // Service Errors
  [ErrorCode.SERVICE_NOT_AVAILABLE]: 'Required service is not available.',
  [ErrorCode.SERVICE_INITIALIZATION_ERROR]: 'Failed to initialize service.',
  [ErrorCode.MODEL_NOT_LOADED]: 'Model not loaded. Please download it first.',
  [ErrorCode.MODEL_LOAD_ERROR]: 'Failed to load model. Please try again.',

  // Processing Errors
  [ErrorCode.PROCESSING_ERROR]: 'Data processing failed.',
  [ErrorCode.SERIALIZATION_ERROR]: 'Failed to serialize data.',
  [ErrorCode.DESERIALIZATION_ERROR]: 'Failed to deserialize data.',
  [ErrorCode.PARSING_ERROR]: 'Failed to parse data.',
  [ErrorCode.EMBEDDING_ERROR]: 'Failed to generate embeddings.',
  [ErrorCode.EXTRACTION_ERROR]: 'Failed to extract content.',
  [ErrorCode.TOKENIZATION_ERROR]: 'Failed to tokenize content.',

  // Concurrency Errors
  [ErrorCode.CONCURRENT_MODIFICATION]: 'Resource was modified by another process.',
  [ErrorCode.QUEUE_FULL]: 'Processing queue is full. Please try again later.',

  // Backup & Recovery Errors
  [ErrorCode.BACKUP_CREATION_FAILED]: 'Failed to create backup.',
  [ErrorCode.BACKUP_RESTORE_FAILED]: 'Failed to restore backup.',
  [ErrorCode.BACKUP_CORRUPTED]: 'Backup file is corrupted.',

  // Generic Errors
  [ErrorCode.INTERNAL_ERROR]: 'Internal server error. Please try again.',
  [ErrorCode.NOT_IMPLEMENTED]: 'Feature not yet implemented.',
  [ErrorCode.UNKNOWN]: 'An unexpected error occurred.',
};

/**
 * Parse unknown error to structured ApiError
 *
 * Handles 4 cases:
 * 1. Structured backend error: { code, message, details? }
 * 2. JavaScript Error object: Extract message + stack
 * 3. String errors: Wrap with UNKNOWN code
 * 4. Unknown types: Safe fallback
 */
export function parseApiError(error: unknown): ApiError {
  // Case 1: Structured backend error
  if (
    error &&
    typeof error === 'object' &&
    'code' in error &&
    'message' in error
  ) {
    const structured = error as Record<string, unknown>;
    return {
      code: String(structured.code || ErrorCode.UNKNOWN),
      message: String(structured.message || 'Unknown error'),
      details: structured.details as Record<string, unknown> | undefined,
    };
  }

  // Case 2: JavaScript Error object
  if (error instanceof Error) {
    return {
      code: ErrorCode.UNKNOWN,
      message: error.message,
      details: {
        stack: error.stack,
        name: error.name,
      },
    };
  }

  // Case 3: String error
  if (typeof error === 'string') {
    return {
      code: ErrorCode.UNKNOWN,
      message: error,
    };
  }

  // Case 4: Unknown type - safe fallback
  return {
    code: ErrorCode.UNKNOWN,
    message: 'An unexpected error occurred',
    details: {
      rawError: String(error),
    },
  };
}

/**
 * Centralized error handler
 *
 * Logs error with full context to console.
 * Never throws - safe to call in any context.
 *
 * Note: UI layer (hooks/components) should handle user notifications.
 */
export function handleApiError(error: ApiError, context?: string): void {
  // Log full error details for debugging
  void console.error('[API Error]', {
    code: error.code,
    message: error.message,
    details: error.details,
    context,
  });

  // Extract user-friendly message (fallback to UNKNOWN for unrecognized codes)
  const userMessage =
    USER_MESSAGES[error.code as keyof typeof USER_MESSAGES] || USER_MESSAGES[ErrorCode.UNKNOWN];

  void console.error('[User Message]', userMessage);
}

/**
 * Convert ApiError to custom Error classes
 *
 * Useful for rethrowing errors in specific contexts.
 */
export function apiErrorToException(error: ApiError): Error {
  const exception = new Error(error.message);
  exception.name = error.code;
  if (error.details) {
    void Object.assign(exception, { details: error.details });
  }
  return exception;
}
