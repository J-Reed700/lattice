import type { SourceWithMetadata } from '@/types/conversation';

/**
 * Map sources to their backend citation numbers.
 *
 * Keys each source by its positive integer `citationId` and skips unnumbered
 * sources instead of falling back to positional array indices.
 *
 * @param sources - Array of source metadata from search results
 * @returns Map of citation number to source metadata
 */
export function createCitationMap(
  sources: readonly SourceWithMetadata[],
): Map<number, SourceWithMetadata> {
  const map = new Map<number, SourceWithMetadata>();
  for (const source of sources) {
    const key = source.citationId;
    if (typeof key === 'number' && Number.isInteger(key) && key > 0 && !map.has(key)) {
      map.set(key, source);
    }
  }
  return map;
}
