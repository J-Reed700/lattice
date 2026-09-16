/**
 * Promise Handler Utilities
 *
 * Provides safe wrappers for async operations in React components.
 * Addresses ESLint errors: @typescript-eslint/no-misused-promises and @typescript-eslint/no-floating-promises
 */

/**
 * Wraps an async function for use in event handlers.
 * Provides automatic error handling and logging.
 *
 * @example
 * ```tsx
 * // Before (ESLint error)
 * <button onClick={async () => await saveData()}>Save</button>
 *
 * // After
 * <button onClick={handleAsyncEvent(async () => {
 *   await saveData();
 * })}>Save</button>
 * ```
 *
 * @param fn - The async function to wrap
 * @param onError - Optional custom error handler
 * @returns A synchronous event handler that safely executes the async function
 */
export const handleAsyncEvent = <T extends unknown[]>(
  fn: (...args: T) => Promise<void>,
  onError?: (error: Error) => void
): ((...args: T) => void) => (...args: T): void => {
    const result = fn(...args);
    // Check if result is actually a promise
    if (result && typeof result.catch === 'function') {
      result.catch((error) => {
        console.error('Async event handler error:', error);
        onError?.(error);
      });
    }
  };

/**
 * Void wrapper for fire-and-forget async operations.
 * Use when you intentionally want to ignore the promise result.
 *
 * @example
 * ```tsx
 * // Before (ESLint error)
 * useEffect(() => {
 *   loadData();
 * }, []);
 *
 * // After
 * useEffect(() => {
 *   void loadData();
 * }, []);
 *
 * // Or with wrapper
 * useEffect(() => {
 *   voidAsync(loadData)();
 * }, []);
 * ```
 *
 * @param fn - The async function to wrap
 * @returns A synchronous function that executes the async function without waiting
 */
export const voidAsync = <T extends unknown[]>(
  fn: (...args: T) => Promise<unknown>
): ((...args: T) => void) => (...args: T): void => {
    void fn(...args);
  };

/**
 * Safe wrapper for async operations with error boundary.
 * Returns a result object with success/error status.
 *
 * @example
 * ```tsx
 * const result = await safeAsync(() => fetchData());
 * if (result.success) {
 *   console.log(result.data);
 * } else {
 *   console.error(result.error);
 * }
 * ```
 *
 * @param fn - The async function to execute
 * @returns Promise resolving to result object with success/error status
 */
export const safeAsync = async <T>(
  fn: () => Promise<T>
): Promise<{ success: true; data: T } | { success: false; error: Error }> => {
  try {
    const data = await fn();
    return { success: true, data };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error : new Error(String(error)),
    };
  }
};
