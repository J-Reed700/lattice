export class PerformanceMonitor {
  private static metrics: Map<string, number[]> = new Map();

  static mark(name: string) {
    if (typeof performance !== 'undefined') {
      performance.mark(name);
    }
  }

  static measure(name: string, startMark: string, endMark?: string) {
    if (typeof performance === 'undefined') return;

    try {
      if (endMark) {
        performance.measure(name, startMark, endMark);
      } else {
        performance.measure(name, startMark);
      }

      const entries = performance.getEntriesByName(name, 'measure');
      if (entries.length > 0) {
        const duration = entries[entries.length - 1].duration;

        if (!this.metrics.has(name)) {
          this.metrics.set(name, []);
        }
        this.metrics.get(name)!.push(duration);

        if (process.env.NODE_ENV === 'development') {
          console.log(`⏱️ ${name}: ${duration.toFixed(2)}ms`);
        }
      }

      performance.clearMarks(startMark);
      if (endMark) performance.clearMarks(endMark);
      performance.clearMeasures(name);
    } catch (error) {
      console.error('Performance measurement error:', error);
    }
  }

  static getMetrics(name: string) {
    const values = this.metrics.get(name) || [];
    if (values.length === 0) {
      return null;
    }

    const sorted = [...values].sort((a, b) => a - b);
    const sum = values.reduce((a, b) => a + b, 0);

    return {
      count: values.length,
      min: sorted[0],
      max: sorted[sorted.length - 1],
      mean: sum / values.length,
      median: sorted[Math.floor(sorted.length / 2)],
      p95: sorted[Math.floor(sorted.length * 0.95)],
      p99: sorted[Math.floor(sorted.length * 0.99)],
    };
  }

  static logSummary() {
    console.log('📊 Performance Summary:');
    this.metrics.forEach((_, name) => {
      const stats = this.getMetrics(name);
      if (stats) {
        console.log(`  ${name}:`, {
          count: stats.count,
          mean: `${stats.mean.toFixed(2)}ms`,
          p95: `${stats.p95.toFixed(2)}ms`,
          p99: `${stats.p99.toFixed(2)}ms`,
        });
      }
    });
  }

  static clear() {
    this.metrics.clear();
    if (typeof performance !== 'undefined') {
      performance.clearMarks();
      performance.clearMeasures();
    }
  }
}

export function measureRender(componentName: string) {
  const startTime = performance.now();

  return () => {
    const duration = performance.now() - startTime;

    if (process.env.NODE_ENV === 'development') {
      if (duration > 16) {
        console.warn(`⚠️ ${componentName} took ${duration.toFixed(2)}ms to render (>16ms)`);
      }
    }

    return duration;
  };
}

export function logRenderInfo(componentName: string, props?: Record<string, unknown>) {
  if (process.env.NODE_ENV === 'development') {
    console.log(`🔄 ${componentName} rendered`, props ? { props } : '');
  }
}
