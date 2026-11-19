import { useEffect, useRef } from 'react';

import { revealInFolder } from '@/features/chat/model/folderThreadHost';
import type { FileTarget } from '@/utils/replyPieces';

/**
 * While a folder thread's answer streams, show the newest file reference it
 * has made. Only a new reference moves the view; one already shown stays put
 * so the reader can scroll away from it. The pass after streaming ends still
 * takes a reference that arrived with the last piece.
 */
export function useRevealStreamingRef(files: readonly FileTarget[] | undefined, active: boolean): void {
  const shownRef = useRef<string | null>(null);
  const wasActiveRef = useRef(false);

  useEffect(() => {
    const was = wasActiveRef.current;
    wasActiveRef.current = active;
    if (!active && !was) return;
    const newest = files?.[files.length - 1];
    if (!newest?.lines) return;
    const { first, last } = newest.lines;
    const key = `${newest.name}:${first}-${last}`;
    if (key === shownRef.current) return;
    shownRef.current = key;
    revealInFolder(newest.name, { startLine: first, endLine: last });
  }, [files, active]);
}
