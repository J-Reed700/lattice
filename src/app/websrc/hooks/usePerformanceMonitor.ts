import { useEffect } from 'react';

import { measureRender } from '../utils/performanceMonitor';

export function usePerformanceMonitor(componentName: string, enabled: boolean = true) {
  useEffect(() => {
    if (!enabled || process.env.NODE_ENV !== 'development') {
      return;
    }

    const end = measureRender(componentName);
    return () => {
      end();
    };
  });
}
