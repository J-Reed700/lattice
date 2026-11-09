/**
 * Utility for extracting readable error messages from unknown error types.
 *
 * Handles various error formats to provide consistent, user-friendly error messages.
 */

/**
 * Extract a readable error message from an unknown error type.
 *
 * @param error - The error to extract a message from
 * @param fallback - Optional fallback message to return when error is null/undefined
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
export function getErrorMessage(error: unknown, fallback?: string): string {
  if (error == null && fallback !== undefined) {
    return fallback;
  }

  try {
    if (typeof error === 'string') return error;
    if (error && typeof error === 'object') {
      if ('message' in error && (error as { message: unknown }).message !== undefined) {
        return String((error as { message: unknown }).message);
      }
      if ('error' in error) {
        const inner = (error as { error: unknown }).error;
        if (typeof inner === 'string') return inner;
        if (inner && typeof inner === 'object' && 'message' in inner && (inner as { message: unknown }).message !== undefined) {
          return String((inner as { message: unknown }).message);
        }
      }
    }
    const serialized = JSON.stringify(error);
    if (serialized !== undefined) return serialized;
  } catch {
    // Error properties and toJSON methods can themselves throw.
  }

  try {
    return String(error);
  } catch {
    return fallback ?? 'Unknown error';
  }
}
