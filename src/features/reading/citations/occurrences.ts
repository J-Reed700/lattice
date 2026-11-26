import type { SourceWithMetadata } from '@/types/conversation';
import type { ReplyPiece } from '@/utils/replyPieces';

import type { AnchoredOccurrence } from './types';

/**
 * The anchored citation of each chip a reply draws, by source number.
 *
 * A chip is counted as `citationMarks` counts `data-cite-at`: the k-th mark of source n in reading
 * order, from zero. A chip without support has no entry but still takes its place in the count.
 */
export function anchoredOccurrences(pieces: readonly ReplyPiece[]): Map<number, AnchoredOccurrence[]> {
  const seen = new Map<number, number>();
  const found = new Map<number, AnchoredOccurrence[]>();
  for (const piece of pieces) {
    if (piece.type !== 'reference' || piece.reference.kind !== 'source') continue;
    const { number, support } = piece.reference;
    const occurrence = seen.get(number) ?? 0;
    seen.set(number, occurrence + 1);
    if (!support) continue;
    const list = found.get(number) ?? [];
    list.push({ occurrence, relation: support.relation, anchors: support.anchors });
    found.set(number, list);
  }
  return found;
}

/** Attaches each source's per-chip support occurrences parsed from `pieces`. */
export function attachAnchoredOccurrences(
  sources: readonly SourceWithMetadata[],
  pieces: readonly ReplyPiece[] | null | undefined,
): SourceWithMetadata[] {
  if (!pieces || pieces.length === 0 || sources.length === 0) return [...sources];
  const byNumber = anchoredOccurrences(pieces);
  if (byNumber.size === 0) return [...sources];
  return sources.map((source) => {
    const anchoredBy =
      source.citationId === undefined ? undefined : byNumber.get(source.citationId);
    return anchoredBy ? { ...source, anchoredBy } : source;
  });
}
