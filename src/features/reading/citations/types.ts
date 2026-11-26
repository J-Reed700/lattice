import type { AnchorDto, RelationDto } from '@/lib/bindings';

/** How one chip of a reply uses its source: the chip's place among its source's chips, the relation, and the sentences it rests on. */
export interface AnchoredOccurrence {
  occurrence: number;
  relation: RelationDto;
  anchors: AnchorDto[];
}
