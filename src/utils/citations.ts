import type { SourceWithMetadata } from '@/types/conversation';

/**
 * Map sources to their citation numbers.
 *
 * Creates a mapping from citation number to source metadata.
 * This allows quick lookup of source details when rendering citations.
 *
 * @param sources - Array of source metadata from search results
 * @returns Map of citation number to source metadata
 *
 * @example
 * const sources = [
 *   { file_name: "doc1.pdf", ... },
 *   { file_name: "doc2.pdf", ... }
 * ];
 * const map = createCitationMap(sources);
 * map.get(1) // { file_name: "doc1.pdf", ... }
 * map.get(2) // { file_name: "doc2.pdf", ... }
 */
export function createCitationMap(
  sources: SourceWithMetadata[]
): Map<number, SourceWithMetadata> {
  const map = new Map<number, SourceWithMetadata>();
  sources.forEach((source, index) => {
    // The number is the one the conversation named the source by. Array
    // position is only a fallback for sources stored without one.
    const key = source.citationId ?? index + 1;
    if (!map.has(key)) {
      map.set(key, source);
    }
  });
  return map;
}
