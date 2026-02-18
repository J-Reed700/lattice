/**
 * Convert any error type to user-friendly string message.
 * Handles Tauri error objects, Error instances, strings, and unknowns.
 *
 * @example
 * catch (err) {
 *   setError(toErrorMessage(err));
 * }
 */
export function toErrorMessage(err: unknown): string {
  if (err instanceof Error) {
    return err.message;
  }

  if (typeof err === 'string') {
    return err;
  }

  if (typeof err === 'object' && err !== null) {
    // Tauri error object - extract message if available
    if ('message' in err && typeof err.message === 'string') {
      return err.message;
    }
    // Otherwise stringify for debugging
    return JSON.stringify(err);
  }

  return 'An unknown error occurred';
}
