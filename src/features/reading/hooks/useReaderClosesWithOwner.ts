import { useEffect } from 'react';

import { useReaderStore } from '@/features/reading/stores/readerStore';

/**
 * Closes the reader when the text that opened it leaves the screen. A null
 * owner does nothing, for surfaces that open the reader on someone else's behalf.
 */
export function useReaderClosesWithOwner(ownerKey: string | null): void {
  const release = useReaderStore((state) => state.release);
  useEffect(() => {
    if (ownerKey === null) return undefined;
    return () => release(ownerKey);
  }, [ownerKey, release]);
}
