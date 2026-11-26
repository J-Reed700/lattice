import type { RelationDto } from '@/lib/bindings';

/** The words a card or a line puts before the source's title, by how a statement uses its source. */
export const RELATION_LABELS: Record<RelationDto, string> = {
  quotes: '',
  restates: '',
  summarizes: '',
  infers: '',
  agrees: '',
  backs: '',
  cites: '',
  contradicts: '',
};

export function relationLabel(_relation: RelationDto): string {
  return '';
}

/** The name a screen reader gives a citation: its number, how it is used when known, and the source's title. */
export function citationAccessibleName(_number: number, _title: string, _relation?: RelationDto | null): string {
  return '';
}
