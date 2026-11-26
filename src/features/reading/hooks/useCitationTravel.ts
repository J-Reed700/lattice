import { useCallback, useEffect, useMemo } from 'react';

import { isAnchored } from '@/features/reading/citations/source';
import type { SourceWithMetadata } from '@/types/conversation';

/**
 * The citations `[` / `]` and the rail step over: all of them, until a source says how its chips use
 * it, and then only the sources with a chip that has support.
 */
function steppable(citations: SourceWithMetadata[] | undefined): number[] {
  const all = (citations ?? []).map((_citation, index) => index);
  if (!citations?.some((citation) => citation.anchoredBy !== undefined)) return all;
  return all.filter((index) => isAnchored(citations[index]!));
}

/**
 * `[` / `]` travel between the citations on a message: whether the reader can
 * travel at all, and the step to the next or previous one it travels over.
 * `position` and `total` count among those, for the rail's "Citation i of n".
 */
export function useCitationTravel(
  citations: SourceWithMetadata[] | undefined,
  citationIndex: number | undefined,
  onCitationIndexChange: ((_index: number) => void) | undefined
) {
  const stops = useMemo(() => steppable(citations), [citations]);
  const total = stops.length;
  const position = typeof citationIndex === 'number' ? stops.indexOf(citationIndex) : -1;
  const canTravel = Boolean(onCitationIndexChange) && total > 1 && typeof citationIndex === 'number';

  const goToCitation = useCallback(
    (next: number) => {
      if (!onCitationIndexChange || (citations?.length ?? 0) === 0) return;
      onCitationIndexChange(Math.min((citations?.length ?? 1) - 1, Math.max(0, next)));
    },
    [onCitationIndexChange, citations]
  );

  const stepCitation = useCallback(
    (direction: 1 | -1) => {
      if (!onCitationIndexChange || typeof citationIndex !== 'number') return;
      const next = direction === 1
        ? stops.find((index) => index > citationIndex)
        : [...stops].reverse().find((index) => index < citationIndex);
      // Past the first or last stop the step stays where it is.
      onCitationIndexChange(next ?? citationIndex);
    },
    [onCitationIndexChange, citationIndex, stops]
  );

  useEffect(() => {
    if (!canTravel) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== '[' && event.key !== ']') return;
      // ⌘[ and ⌘] belong to the Explorer's Back and Forward.
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest('input, textarea, select, [contenteditable="true"]')) {
        return;
      }
      event.preventDefault();
      stepCitation(event.key === ']' ? 1 : -1);
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [canTravel, stepCitation]);

  return { canTravel, goToCitation, stepCitation, position, total };
}
