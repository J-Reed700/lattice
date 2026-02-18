/**
 * useErrorRecovery Hook
 *
 * Provides error recovery strategies with exponential backoff
 */

import { useState, useCallback, useRef } from 'react';

interface ErrorRecoveryOptions {
  maxRetries?: number;
  baseDelay?: number;
  maxDelay?: number;
  onError?: (error: Error, attempt: number) => void;
  onSuccess?: () => void;
}

export function useErrorRecovery(options: ErrorRecoveryOptions = {}) {
  const {
    maxRetries = 3,
    baseDelay = 1000,
    maxDelay = 10000,
    onError,
    onSuccess,
  } = options;

  const [isRetrying, setIsRetrying] = useState(false);
  const [retryCount, setRetryCount] = useState(0);
  const [lastError, setLastError] = useState<Error | null>(null);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);

  /**
   * Calculate delay with exponential backoff
   */
  const calculateDelay = useCallback((attempt: number): number => {
    const delay = baseDelay * Math.pow(2, attempt);
    return Math.min(delay, maxDelay);
  }, [baseDelay, maxDelay]);

  /**
   * Execute function with automatic retry
   */
  const executeWithRetry = useCallback(async <T>(
    fn: () => Promise<T>,
    currentAttempt = 0
  ): Promise<T> => {
    try {
      setIsRetrying(currentAttempt > 0);
      const result = await fn();

      // Success
      setRetryCount(0);
      setLastError(null);
      setIsRetrying(false);

      if (onSuccess) {
        onSuccess();
      }

      return result;
    } catch (error) {
      const errorObj = error instanceof Error ? error : new Error(String(error));
      setLastError(errorObj);

      if (onError) {
        onError(errorObj, currentAttempt);
      }

      // Check if we should retry
      if (currentAttempt < maxRetries) {
        const delay = calculateDelay(currentAttempt);
        setRetryCount(currentAttempt + 1);

        // Wait and retry
        await new Promise((resolve) => {
          timeoutRef.current = setTimeout(resolve, delay);
        });

        return executeWithRetry(fn, currentAttempt + 1);
      }

      // Max retries reached
      setIsRetrying(false);
      throw errorObj;
    }
  }, [maxRetries, calculateDelay, onError, onSuccess]);

  /**
   * Reset retry state
   */
  const reset = useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }
    setIsRetrying(false);
    setRetryCount(0);
    setLastError(null);
  }, []);

  /**
   * Manual retry trigger
   */
  const retry = useCallback(async <T>(fn: () => Promise<T>): Promise<T> => {
    reset();
    return executeWithRetry(fn);
  }, [reset, executeWithRetry]);

  return {
    executeWithRetry,
    retry,
    reset,
    isRetrying,
    retryCount,
    lastError,
    canRetry: retryCount < maxRetries,
  };
}
