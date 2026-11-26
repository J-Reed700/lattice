import type { RelationDto } from '@/lib/bindings';

/** The words a card or a line puts before the source's title, by how a statement uses its source. */
export const RELATION_LABELS: Record<RelationDto, string> = {
  quotes: 'Quoted from',
  restates: 'Paraphrased from',
  summarizes: 'Summarized from',
  infers: 'Inferred from',
  agrees: 'Consistent with',
  backs: 'Supported by',
  cites: 'Cited (not checked)',
  contradicts: 'Contradicted by',
};

/** What a relation reads as in a name that joins it to the source's title. */
const NAME_PHRASES: Record<RelationDto, string> = {
  quotes: 'quoted from',
  restates: 'paraphrased from',
  summarizes: 'summarized from',
  infers: 'inferred from',
  agrees: 'consistent with',
  backs: 'supported by',
  cites: 'cited (not checked) in',
  contradicts: 'contradicted by',
};

export function relationLabel(relation: RelationDto): string {
  return RELATION_LABELS[relation];
}

/** The name a screen reader gives a citation: its number, how it is used when known, and the source's title. */
export function citationAccessibleName(number: number, title: string, relation?: RelationDto | null): string {
  return relation ? `Source ${number}, ${NAME_PHRASES[relation]} ${title}` : `Source ${number}, ${title}`;
}
