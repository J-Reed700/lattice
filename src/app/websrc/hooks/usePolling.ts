import { useEffect, useRef, useCallback, useState } from 'react';

interface PollingOptions {
  onSuccess?: (data: any) => void;
  onError?: (error: Error) => void;
  immediate?: boolean;
  retryOnError?: boolean;
  maxRetries?: number;
}

/**
 * Hook for polling an async function at regular intervals
 */
export function usePolling<T>(
  asyncFunction: () => Promise<T>,
  interval: number | null,
  options: PollingOptions = {}
) {
  const {
    onSuccess,
    onError,
    immediate = false,
    retryOnError = true,
    maxRetries = 3,
  } = options;

  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [isPolling, setIsPolling] = useState(false);
  const retryCount = useRef(0);
  const intervalRef = useRef<NodeJS.Timeout | null>(null);

  const poll = useCallback(async () => {
    try {
      setIsPolling(true);
      const result = await asyncFunction();
      setData(result);
      setError(null);
      retryCount.current = 0;
      onSuccess?.(result);
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      onError?.(error);

      if (retryOnError && retryCount.current < maxRetries) {
        retryCount.current++;
      } else if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    } finally {
      setIsPolling(false);
    }
  }, [asyncFunction, onSuccess, onError, retryOnError, maxRetries]);

  useEffect(() => {
    if (interval === null) {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
      return;
    }

    if (immediate) {
      poll();
    }

    intervalRef.current = setInterval(poll, interval);

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [interval, poll, immediate]);

  const start = useCallback(() => {
    if (interval !== null && !intervalRef.current) {
      intervalRef.current = setInterval(poll, interval);
    }
  }, [interval, poll]);

  const stop = useCallback(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
  }, []);

  const reset = useCallback(() => {
    retryCount.current = 0;
    setData(null);
    setError(null);
  }, []);

  return {
    data,
    error,
    isPolling,
    start,
    stop,
    reset,
  };
}

/**
 * Simple polling hook without data management
 */
export function useSimplePolling(
  callback: () => void | Promise<void>,
  interval: number | null,
  immediate = false
) {
  const savedCallback = useRef(callback);

  useEffect(() => {
    savedCallback.current = callback;
  }, [callback]);

  useEffect(() => {
    if (interval === null) return;

    const tick = async () => {
      await savedCallback.current();
    };

    if (immediate) {
      tick();
    }

    const id = setInterval(tick, interval);
    return () => clearInterval(id);
  }, [interval, immediate]);
}