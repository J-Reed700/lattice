import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

const SEPARATOR = '\u0000';

/**
 * The review items the user has selected for one action, in the order the
 * list shows them. A shift-click selects the range from the last click; items
 * no longer listed drop out of the selection.
 */
export function useReviewSelection(orderedKeys: readonly string[]) {
  const listed = orderedKeys.join(SEPARATOR);
  const order = useMemo(() => (listed ? listed.split(SEPARATOR) : []), [listed]);
  const [chosen, setChosen] = useState<ReadonlySet<string>>(() => new Set());
  const anchor = useRef<string | null>(null);

  useEffect(() => {
    const present = new Set(order);
    setChosen((current) => ([...current].every((key) => present.has(key)) ? current : new Set([...current].filter((key) => present.has(key)))));
    if (anchor.current !== null && !present.has(anchor.current)) anchor.current = null;
  }, [order]);

  const selected = useMemo<ReadonlySet<string>>(() => new Set(order.filter((key) => chosen.has(key))), [order, chosen]);

  const isSelected = useCallback((key: string) => selected.has(key), [selected]);

  const setMany = useCallback((keys: readonly string[], on: boolean) => {
    const present = new Set(order);
    setChosen((current) => {
      const next = new Set(current);
      for (const key of keys) {
        if (!present.has(key)) continue;
        if (on) next.add(key); else next.delete(key);
      }
      return next;
    });
  }, [order]);

  const toggle = useCallback((key: string, options?: { shiftKey?: boolean }) => {
    const at = order.indexOf(key);
    if (at < 0) return;
    const on = !selected.has(key);
    const from = options?.shiftKey && anchor.current !== null ? order.indexOf(anchor.current) : -1;
    anchor.current = key;
    if (from < 0) {
      setMany([key], on);
      return;
    }
    setMany(order.slice(Math.min(from, at), Math.max(from, at) + 1), on);
  }, [order, selected, setMany]);

  const clear = useCallback(() => {
    anchor.current = null;
    setChosen(new Set());
  }, []);

  return { selected, isSelected, toggle, setMany, clear };
}
