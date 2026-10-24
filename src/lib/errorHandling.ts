import { ErrorCode } from '../types/api/errorCodes';
import { type ApiError, type ApiFailure } from '../types/api/result';

const USER_MESSAGES: Record<ErrorCode, string> = {
  [ErrorCode.NOT_FOUND]: 'The requested resource was not found.',
  [ErrorCode.ALREADY_EXISTS]: 'This resource already exists.',
  [ErrorCode.GONE]: 'This resource has been deleted or is no longer available.',

  [ErrorCode.PERMISSION_DENIED]: 'You do not have permission to perform this action.',
  [ErrorCode.UNAUTHORIZED]: 'Authentication failed. Please log in again.',
  [ErrorCode.SECURITY_VIOLATION]: 'Security violation detected. Action blocked.',

  [ErrorCode.VALIDATION_ERROR]: 'The provided data is invalid.',
  [ErrorCode.INVALID_INPUT]: 'Invalid input provided.',
  [ErrorCode.INVALID_CONFIG]: 'Configuration error. Please check settings.',
  [ErrorCode.INVALID_STATE]: 'Operation cannot be performed in the current state.',
  [ErrorCode.CONSTRAINT_VIOLATION]: 'Operation violates data constraints.',

  [ErrorCode.FILE_NOT_FOUND]: 'File not found.',
  [ErrorCode.FILE_TOO_LARGE]: 'File is too large to process.',
  [ErrorCode.UNSUPPORTED_FILE_TYPE]: 'File type is not supported.',
  [ErrorCode.FILE_SYSTEM_ERROR]: 'File system error. Please check permissions.',
  [ErrorCode.FILE_READ_ERROR]: 'Failed to read file.',
  [ErrorCode.FILE_WRITE_ERROR]: 'Failed to write file.',

  [ErrorCode.DATABASE_ERROR]: 'Database operation failed. Please try again.',
  [ErrorCode.DATABASE_CONNECTION_ERROR]: 'Failed to connect to database.',
  [ErrorCode.MIGRATION_ERROR]: 'Database migration failed.',

  [ErrorCode.NETWORK_ERROR]: 'Network error. Please check your connection.',
  [ErrorCode.TIMEOUT]: 'Operation timed out. Please try again.',
  [ErrorCode.RATE_LIMIT_EXCEEDED]: 'Too many requests. Please try again later.',

  [ErrorCode.SERVICE_NOT_AVAILABLE]: 'Required service is not available.',
  [ErrorCode.SERVICE_INITIALIZATION_ERROR]: 'Failed to initialize service.',
  [ErrorCode.MODEL_NOT_LOADED]: 'Model not loaded. Please download it first.',
  [ErrorCode.MODEL_LOAD_ERROR]: 'Failed to load model. Please try again.',

  [ErrorCode.PROCESSING_ERROR]: 'Data processing failed.',
  [ErrorCode.SERIALIZATION_ERROR]: 'Failed to serialize data.',
  [ErrorCode.DESERIALIZATION_ERROR]: 'Failed to deserialize data.',
  [ErrorCode.PARSING_ERROR]: 'Failed to parse data.',
  [ErrorCode.EMBEDDING_ERROR]: 'Failed to generate embeddings.',
  [ErrorCode.EXTRACTION_ERROR]: 'Failed to extract content.',
  [ErrorCode.TOKENIZATION_ERROR]: 'Failed to tokenize content.',

  [ErrorCode.CONCURRENT_MODIFICATION]: 'Resource was modified by another process.',
  [ErrorCode.QUEUE_FULL]: 'Processing queue is full. Please try again later.',

  [ErrorCode.BACKUP_CREATION_FAILED]: 'Failed to create backup.',
  [ErrorCode.BACKUP_RESTORE_FAILED]: 'Failed to restore backup.',
  [ErrorCode.BACKUP_CORRUPTED]: 'Backup file is corrupted.',

  [ErrorCode.INTERNAL_ERROR]: 'Internal server error. Please try again.',
  [ErrorCode.NOT_IMPLEMENTED]: 'Feature not yet implemented.',
  [ErrorCode.UNKNOWN]: 'An unexpected error occurred.',
};

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

export function parseApiError(error: unknown): ApiError {
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

  if (error instanceof Error) {
    return {
      code: ErrorCode.UNKNOWN,
      message: error.message,
      details: error.stack ?? error.name,
    };
  }

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

export function handleApiError(error: ApiError, context?: string): void {
  void console.error('[API Error]', {
    code: error.code,
    message: error.message,
    details: error.details,
    context,
  });

  const userMessage =
    USER_MESSAGES[error.code] || USER_MESSAGES[ErrorCode.UNKNOWN];

  void console.error('[User Message]', userMessage);
}

export function apiErrorToException(error: ApiError): Error {
  const exception = new Error(error.message);
  exception.name = error.code;
  Object.assign(exception, { code: error.code });
  if (error.details) {
    void Object.assign(exception, { details: error.details });
  }
  return exception;
}
