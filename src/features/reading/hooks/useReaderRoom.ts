import { useEffect, useState, type DependencyList, type RefObject } from 'react';

/**
 * How much room a text and its docked reader have between them.
 *
 * Measured from the row rather than the window because that is what the two
 * of them actually share: collapsing a sidebar hands the reader 200-odd
 * pixels the viewport knows nothing about. `besides` is what else in the row
 * takes room (a sidebar); `deps` are the changes that move it without
 * resizing the row, so the effect re-runs and measures again.
 */
export function useReaderRoom(
  rowRef: RefObject<HTMLElement | null>,
  besides: ((row: HTMLElement) => number) | undefined,
  deps: DependencyList,
): { width: number; available: number } {
  const [row, setRow] = useState({ width: 0, available: 0 });

  useEffect(() => {
    const element = rowRef.current;
    if (!element || typeof ResizeObserver === 'undefined') return;

    const measure = () => {
      const width = element.clientWidth;
      const available = Math.max(0, width - (besides?.(element) ?? 0));
      setRow((current) =>
        current.width === width && current.available === available
          ? current
          : { width, available }
      );
    };

    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return row;
}
