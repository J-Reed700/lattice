/** Wraps an async function for use in event handlers with error catching. */
export const handleAsyncEvent = <T extends unknown[]>(
  fn: (...args: T) => Promise<void>,
  onError?: (error: Error) => void
): ((...args: T) => void) => (...args: T): void => {
    const result = fn(...args);
    if (result && typeof result.catch === 'function') {
      result.catch((error) => {
        console.error('Async event handler error:', error);
        onError?.(error);
      });
    }
  };

/** Void wrapper for fire-and-forget async operations. */
export const voidAsync = <T extends unknown[]>(
  fn: (...args: T) => Promise<unknown>
): ((...args: T) => void) => (...args: T): void => {
    void fn(...args);
  };

/** Safe wrapper for async operations returning success/error result. */
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
