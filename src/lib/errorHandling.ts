/**
 * Centralized Error Handling
 *
 * Utilities for parsing, handling, and converting API errors.
 * Provides consistent error handling across the frontend.
 */

import { ErrorCode } from '../types/api/errorCodes';
import { type ApiError, type ApiFailure } from '../types/api/result';

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
  [ErrorCode.DATABASE_ERROR]: "Lattice couldn't save your work. Try again.",
  [ErrorCode.DATABASE_CONNECTION_ERROR]: 'Failed to connect to database.',
  [ErrorCode.MIGRATION_ERROR]: 'Database migration failed.',

  // Network Errors
  [ErrorCode.NETWORK_ERROR]: "Lattice couldn't reach the model server.",
  [ErrorCode.TIMEOUT]: 'Operation timed out. Please try again.',
  [ErrorCode.RATE_LIMIT_EXCEEDED]: 'The model is getting too many requests. Wait a minute and try again.',

  // Service Errors
  [ErrorCode.SERVICE_NOT_AVAILABLE]: 'Required service is not available.',
  [ErrorCode.SERVICE_INITIALIZATION_ERROR]: 'Failed to initialize service.',
  [ErrorCode.MODEL_NOT_LOADED]: 'Model not loaded. Please download it first.',
  [ErrorCode.MODEL_LOAD_ERROR]: "The model server couldn't start. Reinstall Lattice, or choose another model in Settings.",
  [ErrorCode.PROMPT_EXCEEDS_WINDOW]: 'That is too much for the model to read at once. Try a smaller course or a model with a larger context window.',
  [ErrorCode.LEARNING_MODEL_NOT_SET]: 'Courses need a model to write them.',

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
  [ErrorCode.UNKNOWN]: 'Something went wrong.',
};

const WAITING_MESSAGES: Partial<Record<ErrorCode, string>> = {
  [ErrorCode.NETWORK_ERROR]: "Can't reach the model server. Lattice keeps trying.",
  [ErrorCode.RATE_LIMIT_EXCEEDED]: 'The model is getting too many requests. Lattice keeps trying.',
  [ErrorCode.SERVICE_NOT_AVAILABLE]: 'The model server is busy or starting. Lattice keeps trying.',
};

/** What a learner reads while work waits out an outage and retries, from the code of the error it is waiting on. */
export function waitingSentence(code: ErrorCode | null | undefined): string {
  return (code && WAITING_MESSAGES[code]) ?? "The model server isn't answering. Lattice keeps trying.";
}

const KNOWN_CODES: ReadonlySet<string> = new Set(Object.values(ErrorCode));

/** A backend code, or UNKNOWN for anything that is not one. */
function errorCode(value: unknown): ErrorCode {
  return typeof value === 'string' && KNOWN_CODES.has(value)
    ? (value as ErrorCode)
    : ErrorCode.UNKNOWN;
}

/** Backend details are text; anything else is carried as its JSON. */
function detailText(value: unknown): string | null {
  if (value === undefined || value === null) return null;
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/**
 * Parse unknown error to structured ApiError
 *
 * Handles 4 cases:
 * 1. Structured backend error: { code, message, details? }
 * 2. JavaScript Error object: message, with the stack as details
 * 3. String errors: a serialized backend error, or text wrapped as UNKNOWN
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
      code: errorCode(structured.code),
      message: String(structured.message || 'Unknown error'),
      details: detailText(structured.details),
    };
  }

  // Case 2: JavaScript Error object
  if (error instanceof Error) {
    return {
      code: ErrorCode.UNKNOWN,
      message: error.message,
      details: error.stack ?? error.name,
    };
  }

  // Case 3: String error
  if (typeof error === 'string') {
    try {
      const parsed: unknown = JSON.parse(error);
      if (parsed && typeof parsed === 'object' && 'message' in parsed && 'code' in parsed) return parseApiError(parsed);
    } catch { /* Plain-text backend errors are also valid. */ }
    return {
      code: ErrorCode.UNKNOWN,
      message: error,
    };
  }

  // Case 4: Unknown type - safe fallback
  return {
    code: ErrorCode.UNKNOWN,
    message: 'An unexpected error occurred',
    details: String(error),
  };
}

/**
 * One line for the user from a failed call: the backend's message, then its
 * detail when the message does not already carry it. A generic message such
 * as "Network error" only helps with the reason after it.
 */
export function describeFailure(failure: ApiFailure): string {
  const detail = failure.details?.details;
  return detail && !failure.error.includes(detail)
    ? `${failure.error}: ${detail}`
    : failure.error;
}

/** The sentence a refused call carries, without the category the backend puts before it. */
export function refusalSentence(error: { message: string; code?: unknown }): string {
  return error.message;
}

/** The error a failed call carries to a component, keeping its code and the backend's own text. */
export function failureToError(failure: ApiFailure): Error {
  return apiErrorToException(failure.details ?? { code: ErrorCode.UNKNOWN, message: failure.error });
}

/** The error a job recorded, with its code when the job store kept one. */
export function jobErrorToError(job: { error: string | null; errorCode?: ErrorCode | null }): Error {
  return apiErrorToException({ code: job.errorCode ?? ErrorCode.UNKNOWN, message: job.error ?? '' });
}

/** A learner's first sentence for an error, from its code alone, and the backend's own text for Details. */
export function learnerError(error: unknown): { headline: string; raw: string } {
  const carried = typeof error === 'object' && error !== null ? error as { code?: unknown; details?: unknown; message?: unknown } : {};
  const details = typeof carried.details === 'string' ? carried.details.trim() : '';
  const message = typeof carried.message === 'string' ? carried.message : String(error ?? '');
  return { headline: USER_MESSAGES[errorCode(carried.code)], raw: details || message };
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
    USER_MESSAGES[error.code] || USER_MESSAGES[ErrorCode.UNKNOWN];

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
  Object.assign(exception, { code: error.code });
  if (error.details) {
    void Object.assign(exception, { details: error.details });
  }
  return exception;
}
