/**
 * Simple debounce utility for search and other operations
 *
 * This utility prevents excessive function calls by delaying execution
 * until after a specified period of inactivity.
 */

export interface DebouncedFunction<TArgs extends unknown[]> {
  (...args: TArgs): void;
  cancel: () => void;
  flush: () => void;
}

/**
 * Creates a debounced version of a function
 *
 * @param func - The function to debounce
 * @param delay - The delay in milliseconds
 * @returns A debounced version of the function with cancel and flush methods
 */
export function debounce<TArgs extends unknown[]>(
  func: (...args: TArgs) => unknown,
  delay: number
): DebouncedFunction<TArgs> {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  let lastArgs: TArgs | null = null;

  const debounced = (...args: TArgs) => {
    lastArgs = args;

    if (timeoutId !== null) {
      clearTimeout(timeoutId);
    }

    timeoutId = setTimeout(() => {
      if (lastArgs) {
        func(...lastArgs);
        lastArgs = null;
      }
      timeoutId = null;
    }, delay);
  };

  debounced.cancel = () => {
    if (timeoutId !== null) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }
    lastArgs = null;
  };

  debounced.flush = () => {
    if (timeoutId !== null) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }
    if (lastArgs) {
      func(...lastArgs);
      lastArgs = null;
    }
  };

  return debounced;
}
