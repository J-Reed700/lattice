import { useRef, useEffect } from 'react';

export function useRenderCount(componentName: string, logToConsole: boolean = false) {
  const renderCount = useRef(0);

  useEffect(() => {
    renderCount.current += 1;

    if (logToConsole && process.env.NODE_ENV === 'development') {
      console.log(`🔄 ${componentName} rendered ${renderCount.current} times`);
    }
  });

  return renderCount.current;
}
