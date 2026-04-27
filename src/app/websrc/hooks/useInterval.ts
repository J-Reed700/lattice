import { useEffect, useRef, useCallback, useState } from 'react';

/**
 * Hook for running a callback at regular intervals
 */
export function useInterval(callback: () => void, delay: number | null) {
  const savedCallback = useRef(callback);

  useEffect(() => {
    savedCallback.current = callback;
  }, [callback]);

  useEffect(() => {
    if (delay === null) return;

    const id = setInterval(() => savedCallback.current(), delay);
    return () => clearInterval(id);
  }, [delay]);
}

/**
 * Simple interval hook without ref optimization
 */
export function useIntervalSimple(callback: () => void, delay: number | null) {
  useEffect(() => {
    if (delay === null) return;

    const id = setInterval(callback, delay);
    return () => clearInterval(id);
  }, [callback, delay]);
}

/**
 * Dynamic interval hook with control methods
 */
export function useDynamicInterval(callback: () => void, initialDelay: number | null = null) {
  const [delay, setDelay] = useState(initialDelay);
  const savedCallback = useRef(callback);

  useEffect(() => {
    savedCallback.current = callback;
  }, [callback]);

  useEffect(() => {
    if (delay === null) return;

    const id = setInterval(() => savedCallback.current(), delay);
    return () => clearInterval(id);
  }, [delay]);

  const start = useCallback((newDelay?: number) => {
    setDelay(newDelay ?? initialDelay);
  }, [initialDelay]);

  const stop = useCallback(() => {
    setDelay(null);
  }, []);

  const updateDelay = useCallback((newDelay: number) => {
    setDelay(newDelay);
  }, []);

  return {
    start,
    stop,
    updateDelay,
    isRunning: delay !== null,
  };
}

/**
 * Countdown timer hook
 */
export function useCountdown(initialCount: number, interval: number = 1000) {
  const [count, setCount] = useState(initialCount);
  const [isActive, setIsActive] = useState(false);

  useEffect(() => {
    if (!isActive || count <= 0) return;

    const id = setInterval(() => {
      setCount((c) => {
        if (c <= 1) {
          setIsActive(false);
          return 0;
        }
        return c - 1;
      });
    }, interval);

    return () => clearInterval(id);
  }, [count, interval, isActive]);

  const start = useCallback(() => {
    setIsActive(true);
  }, []);

  const stop = useCallback(() => {
    setIsActive(false);
  }, []);

  const reset = useCallback(() => {
    setCount(initialCount);
    setIsActive(false);
  }, [initialCount]);

  return {
    count,
    isActive,
    start,
    stop,
    reset,
  };
}