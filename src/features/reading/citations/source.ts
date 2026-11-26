import { articleBlocks } from '@/features/reading/model/articleBlocks';
import type { RelationDto } from '@/lib/bindings';
import type { SourceWithMetadata } from '@/types/conversation';

import { placeAnchors } from './anchors';
import { headingTrail } from './headingTrail';
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
  return source.content?.trim() ? source.content : (source.excerpt ?? '');
}

/**
 * Relation, anchor excerpts, and heading trail for a citation card.
 *
 * When a specific `occurrence` is named, only that chip counts; when omitted
 * (a whole-source note or footnote), the first chip carrying anchors — else the
 * first entry — stands for the source.
 */
export function anchoredCardDetails(
  source: Pick<SourceWithMetadata, 'anchoredBy' | 'anchorBase' | 'webSnapshot' | 'excerpt' | 'content'>,
  occurrence: number | null | undefined = null,
): {
  entry: AnchoredOccurrence | null;
  relation: RelationDto | null;
  excerpts: string[];
  trail: string[];
} {
  const entry =
    occurrence !== null && occurrence !== undefined
      ? anchoredEntry(source, occurrence)
      : (source.anchoredBy?.find((item) => item.anchors.length > 0) ?? source.anchoredBy?.[0] ?? null);
  if (!entry) return { entry: null, relation: null, excerpts: [], trail: [] };

  const excerpts = entry.anchors.map((anchor) => anchor.exact).filter(Boolean);
  const text = anchoredText(source);
  if (!text || entry.anchors.length === 0) {
    return { entry, relation: entry.relation, excerpts, trail: [] };
  }
  const { ranges } = placeAnchors(text, entry.anchors, source.anchorBase ?? 0);
  const first = ranges[0];
  const trail = first ? headingTrail(text, articleBlocks(text), first.start) : [];
  return { entry, relation: entry.relation, excerpts, trail };
}
