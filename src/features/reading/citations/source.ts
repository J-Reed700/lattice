import type { SourceWithMetadata } from '@/types/conversation';

import type { AnchoredOccurrence } from './types';

/** Whether a source carries chips that say how they use it. */
export function isAnchored(source: Pick<SourceWithMetadata, 'anchoredBy'>): boolean {
  return (source.anchoredBy?.length ?? 0) > 0;
}

/** The anchored citation of the chip `occurrence` of a source, if that chip has support. */
export function anchoredEntry(
  source: Pick<SourceWithMetadata, 'anchoredBy'>,
  occurrence: number | null | undefined,
): AnchoredOccurrence | null {
  if (occurrence === null || occurrence === undefined) return null;
  return source.anchoredBy?.find((entry) => entry.occurrence === occurrence) ?? null;
}

/** The saved text a source's anchors are placed in, exactly as saved. */
export function anchoredText(source: Pick<SourceWithMetadata, 'webSnapshot' | 'excerpt' | 'content'>): string {
  if (source.webSnapshot?.text.trim()) return source.webSnapshot.text;
  return source.excerpt?.trim() ? source.excerpt : (source.content ?? '');
}
