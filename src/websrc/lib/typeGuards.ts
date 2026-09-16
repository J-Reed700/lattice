/**
 * Type guards and type conversion utilities
 */

/**
 * Converts an unknown value to an Error object
 * @param error - Unknown error value
 * @returns Error object
 */
export function toError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }

  if (typeof error === 'string') {
    return new Error(error);
  }

  if (typeof error === 'object' && error !== null) {
    if ('message' in error && typeof error.message === 'string') {
      const err = new Error(error.message);
      // Preserve stack if available
      if ('stack' in error && typeof error.stack === 'string') {
        err.stack = error.stack;
      }
      return err;
    }

    // Try to stringify the object
    try {
      return new Error(JSON.stringify(error));
    } catch {
      return new Error('Unknown error object');
    }
  }

  // Fallback for other types
  return new Error(String(error));
}

/**
 * Type guard to check if a value is an Error
 * @param value - Value to check
 * @returns True if value is an Error
 */
export function isError(value: unknown): value is Error {
  return value instanceof Error;
}

/**
 * Type guard to check if a value is a string
 * @param value - Value to check
 * @returns True if value is a string
 */
export function isString(value: unknown): value is string {
  return typeof value === 'string';
}

/**
 * Type guard to check if a value is a number
 * @param value - Value to check
 * @returns True if value is a number
 */
export function isNumber(value: unknown): value is number {
  return typeof value === 'number' && !isNaN(value);
}

/**
 * Type guard to check if a value is defined (not null or undefined)
 * @param value - Value to check
 * @returns True if value is defined
 */
export function isDefined<T>(value: T | null | undefined): value is T {
  return value !== null && value !== undefined;
}
