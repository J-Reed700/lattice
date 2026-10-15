import { useCallback, useEffect } from 'react';

import type { SourceWithMetadata } from '@/types/conversation';

/**
 * `[` / `]` travel between the citations on a message: whether the reader can
 * travel at all, and the step that clamps to the first and last citation.
 */
export function useCitationTravel(
  citations: SourceWithMetadata[] | undefined,
  citationIndex: number | undefined,
  onCitationIndexChange: ((_index: number) => void) | undefined
) {
  const total = citations?.length ?? 0;
  const canTravel = Boolean(onCitationIndexChange) && total > 1 && typeof citationIndex === 'number';

  const goToCitation = useCallback(
    (next: number) => {
      if (!onCitationIndexChange || total === 0) return;
      onCitationIndexChange(Math.min(total - 1, Math.max(0, next)));
    },
    [onCitationIndexChange, total]
  );

  useEffect(() => {
    if (!canTravel) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== '[' && event.key !== ']') return;
      // ⌘[ and ⌘] belong to the Explorer's Back and Forward.
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      if (
        target?.closest('input, textarea, select, [contenteditable="true"]')
      ) {
        return;
      }
      event.preventDefault();
      goToCitation((citationIndex ?? 0) + (event.key === ']' ? 1 : -1));
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [canTravel, citationIndex, goToCitation]);

  return { canTravel, goToCitation };
}
