import { useEffect, useRef } from 'react';

import type { PerformanceStats, PerformanceReport } from '../types/metadata';

/**
 * Performance Monitoring Utilities
 *
 * Purpose: Track and measure component render performance
 *
 * Features:
 * - Render count tracking
 * - Render time measurement
 * - Performance warnings for slow renders
 * - Development-only logging
 */

/**
 * Hook to track how many times a component renders
 *
 * Usage:
 * ```tsx
 * const SearchView = () => {
 *   useRenderCount('SearchView');
 *   // ... component code
 * };
 * ```
 */
export function useRenderCount(componentName: string) {
  const renderCount = useRef(0);

  useEffect(() => {
    renderCount.current += 1;

    if (process.env.NODE_ENV === 'development') {
      console.log(`[Performance] ${componentName} rendered ${renderCount.current} times`);
    }
  });

  return renderCount.current;
}

/**
 * Hook to measure render time and warn about slow renders
 *
 * Usage:
 * ```tsx
 * const SearchView = () => {
 *   useRenderTime('SearchView', 16); // Warn if render takes > 16ms (1 frame)
 *   // ... component code
 * };
 * ```
 */
export function useRenderTime(componentName: string, threshold: number = 16) {
  const startTime = useRef(performance.now());

  useEffect(() => {
    const renderTime = performance.now() - startTime.current;

    if (process.env.NODE_ENV === 'development') {
      if (renderTime > threshold) {
        console.warn(
          `[Performance] ${componentName} slow render: ${renderTime.toFixed(2)}ms (threshold: ${threshold}ms)`
        );
      } else {
        console.log(
          `[Performance] ${componentName} render time: ${renderTime.toFixed(2)}ms`
        );
      }
    }

    startTime.current = performance.now();
  });
}

/**
 * Hook to track why a component re-rendered
 *
 * Usage:
 * ```tsx
 * const SearchView = ({ results, isSearching }) => {
 *   useWhyDidYouUpdate('SearchView', { results, isSearching });
 *   // ... component code
 * };
 * ```
 */
export function useWhyDidYouUpdate(componentName: string, props: Record<string, unknown>) {
  const previousProps = useRef<Record<string, unknown>>({});

  useEffect(() => {
    if (previousProps.current && process.env.NODE_ENV === 'development') {
      const allKeys = Object.keys({ ...previousProps.current, ...props });
      const changedProps: Record<string, { from: unknown; to: unknown }> = {};

      allKeys.forEach((key) => {
        if (previousProps.current![key] !== props[key]) {
          changedProps[key] = {
            from: previousProps.current![key],
            to: props[key],
          };
        }
      });

      if (Object.keys(changedProps).length > 0) {
        console.log(`[Performance] ${componentName} re-rendered due to:`, changedProps);
      }
    }

    previousProps.current = props;
  });
}

/**
 * Performance profiler component wrapper
 *
 * Usage:
 * ```tsx
 * <PerformanceProfiler id="SearchView" onRender={handleRender}>
 *   <SearchView />
 * </PerformanceProfiler>
 * ```
 */
export function createRenderCallback(componentId: string) {
  return (
    _id: string,
    phase: 'mount' | 'update',
    actualDuration: number,
    baseDuration: number,
    startTime: number,
    commitTime: number
  ) => {
    if (process.env.NODE_ENV === 'development') {
      const message = `[Performance] ${componentId} ${phase}:`;
      const details = {
        actualDuration: `${actualDuration.toFixed(2)}ms`,
        baseDuration: `${baseDuration.toFixed(2)}ms`,
        startTime: `${startTime.toFixed(2)}ms`,
        commitTime: `${commitTime.toFixed(2)}ms`,
      };

      if (actualDuration > 16) {
        console.warn(message, details);
      } else {
        console.log(message, details);
      }
    }
  };
}

/**
 * Measure async operation performance
 *
 * Usage:
 * ```tsx
 * const result = await measureAsync('Search API', async () => {
 *   return await VaultAPI.search(query);
 * });
 * ```
 */
export async function measureAsync<T>(
  operationName: string,
  operation: () => Promise<T>
): Promise<T> {
  if (process.env.NODE_ENV === 'development') {
    const startTime = performance.now();

    try {
      const result = await operation();
      const duration = performance.now() - startTime;

      console.log(`[Performance] ${operationName}: ${duration.toFixed(2)}ms`);

      return result;
    } catch (error) {
      const duration = performance.now() - startTime;
      console.error(`[Performance] ${operationName} failed after ${duration.toFixed(2)}ms`);
      throw error;
    }
  }

  return operation();
}

/**
 * Performance metrics collector
 */
class PerformanceMetrics {
  private metrics: Map<string, number[]> = new Map();

  record(metricName: string, value: number) {
    if (!this.metrics.has(metricName)) {
      this.metrics.set(metricName, []);
    }

    this.metrics.get(metricName)!.push(value);
  }

  getStats(metricName: string): PerformanceStats | null {
    const values = this.metrics.get(metricName) || [];

    if (values.length === 0) {
      return null;
    }

    const sorted = [...values].sort((a, b) => a - b);
    const sum = values.reduce((a, b) => a + b, 0);

    return {
      count: values.length,
      min: sorted[0],
      max: sorted[sorted.length - 1],
      avg: sum / values.length,
      median: sorted[Math.floor(sorted.length / 2)],
      p95: sorted[Math.floor(sorted.length * 0.95)],
      p99: sorted[Math.floor(sorted.length * 0.99)],
    };
  }

  getReport(): PerformanceReport {
    const report: PerformanceReport = {};

    this.metrics.forEach((_, metricName) => {
      report[metricName] = this.getStats(metricName);
    });

    return report;
  }

  clear() {
    this.metrics.clear();
  }
}

export const performanceMetrics = new PerformanceMetrics();

/**
 * Export performance report to console
 */
export function logPerformanceReport() {
  if (process.env.NODE_ENV === 'development') {
    console.table(performanceMetrics.getReport());
  }
}
