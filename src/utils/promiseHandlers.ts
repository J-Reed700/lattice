import { getErrorMessage } from '@/lib/errorUtils';

/** Runs an event handler immediately and contains synchronous and asynchronous failures. */
export const handleAsyncEvent = <T extends unknown[]>(
  fn: (...args: T) => Promise<unknown> | void,
  onError?: (error: Error) => void
): ((...args: T) => void) => {
  const report = (error: unknown) => {
    const failure = error instanceof Error ? error : new Error(getErrorMessage(error));
    console.error('Async event handler error:', failure);
    try {
      onError?.(failure);
    } catch (callbackError) {
      console.error('Async event error callback failed:', callbackError);
    }
  };
  return (...args: T): void => {
    try {
      // Event handlers may need to preventDefault before returning.
      void Promise.resolve(fn(...args)).catch(report);
    } catch (error) {
      report(error);
    }
  };
};

/** Starts a background operation and reports a rejected result. */
export const voidAsync = <T extends unknown[]>(
  fn: (...args: T) => Promise<unknown>
): ((...args: T) => void) => handleAsyncEvent(fn);

/** Returns an explicit success or failure without rejecting. */
export const safeAsync = async <T>(
  fn: () => Promise<T>
): Promise<{ success: true; data: T } | { success: false; error: Error }> => {
  try {
    const data = await fn();
    return { success: true, data };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error : new Error(getErrorMessage(error)),
    };
  }
};
