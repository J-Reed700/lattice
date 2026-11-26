import { describe, expect, it } from 'vitest';

import type { RelationDto } from '@/lib/bindings';

import { citationAccessibleName, relationLabel, RELATION_LABELS } from '../relation';

const LABELS: Record<RelationDto, string> = {
  quotes: 'Quoted from',
  restates: 'Paraphrased from',
  summarizes: 'Summarized from',
  infers: 'Inferred from',
  agrees: 'Consistent with',
  backs: 'Supported by',
  cites: 'Cited (not checked)',
  contradicts: 'Contradicted by',
};

describe('relation words', () => {
  it.each(Object.entries(LABELS))('labels %s as "%s"', (relation, label) => {
    expect(relationLabel(relation as RelationDto)).toBe(label);
    expect(RELATION_LABELS[relation as RelationDto]).toBe(label);
  });

  it('names a citation by its number, how it is used and the source title', () => {
    expect(citationAccessibleName(2, 'The Quillet Guide', 'restates')).toBe('Source 2, paraphrased from The Quillet Guide');
    expect(citationAccessibleName(1, 'The Quillet Guide', 'contradicts')).toBe(
      'Source 1, contradicted by The Quillet Guide',
    );
  });

  it('leaves the relation out of the name when there is none', () => {
    expect(citationAccessibleName(2, 'The Quillet Guide')).toBe('Source 2, The Quillet Guide');
    expect(citationAccessibleName(2, 'The Quillet Guide', null)).toBe('Source 2, The Quillet Guide');
  });
});
