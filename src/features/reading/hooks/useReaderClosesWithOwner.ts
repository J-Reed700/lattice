import { useEffect } from 'react';

import { useReaderStore } from '@/features/reading/stores/readerStore';

/** Closes the reader when the text that opened it leaves the screen. */
export function useReaderClosesWithOwner(ownerKey: string): void {
  const release = useReaderStore((state) => state.release);
  useEffect(() => () => release(ownerKey), [ownerKey, release]);
}
