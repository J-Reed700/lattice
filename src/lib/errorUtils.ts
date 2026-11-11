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
  try {
    if (typeof error === 'string') return error;
    if (error && typeof error === 'object' && 'message' in error) {
      return String(error.message);
    }
    const serialized = JSON.stringify(error);
    if (serialized !== undefined) return serialized;
  } catch {
    // Error properties and toJSON methods can themselves throw.
  }

  try {
    return String(error);
  } catch {
    return 'Unknown error';
  }
}
