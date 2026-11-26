import type { ReplyPiece } from '@/utils/replyPieces';

import type { AnchoredOccurrence } from './types';

/**
 * The anchored citation of each chip a reply draws, by source number.
 *
 * A chip is counted as `citationMarks` counts `data-cite-at`: the k-th mark of source n in reading
 * order, from zero. A chip without support has no entry but still takes its place in the count.
 */
export function anchoredOccurrences(_pieces: readonly ReplyPiece[]): Map<number, AnchoredOccurrence[]> {
  return new Map();
}
