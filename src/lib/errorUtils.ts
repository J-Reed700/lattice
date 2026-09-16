/**
 * Utility for extracting readable error messages from unknown error types.
 *
 * Handles various error formats to provide consistent, user-friendly error messages.
 */

/**
 * Extract a readable error message from an unknown error type.
 *
 * @param error - The error to extract a message from
 * @returns A human-readable error message string
 *
 * @example
 * try {
 *   await someOperation();
 * } catch (error) {
 *   const message = getErrorMessage(error);
 *   console.error(message); // "Actual error message" instead of "[object Object]"
 * }
 */
export function getErrorMessage(error: unknown): string {
  // Standard Error object
  if (error instanceof Error) {
    return error.message;
  }

  // String error
  if (typeof error === 'string') {
    return error;
  }

  // Object with message property
  if (error && typeof error === 'object' && 'message' in error) {
    return String(error.message);
  }

  // Fallback: stringify the error
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}
