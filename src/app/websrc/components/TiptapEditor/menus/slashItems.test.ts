import { describe, expect, it } from 'vitest';

import { SLASH_ITEMS, filterSlashItems } from './slashItems';

const labels = (query: string) => filterSlashItems(SLASH_ITEMS, query).map((item) => item.label);

describe('filterSlashItems', () => {
  it('returns every item in declaration order for an empty query', () => {
    expect(labels('')).toEqual([
      'Heading 1',
      'Heading 2',
      'Heading 3',
      'Bulleted list',
      'Numbered list',
      'To-do list',
      'Table',
      'Code block',
      'Link to a note',
    ]);
  });

  it('matches a label prefix', () => {
    expect(labels('head')).toEqual(['Heading 1', 'Heading 2', 'Heading 3']);
  });

  it('matches a keyword', () => {
    expect(labels('h1')).toEqual(['Heading 1']);
    expect(labels('todo')).toEqual(['To-do list']);
  });

  it('puts prefix matches before substring matches', () => {
    expect(labels('list')).toEqual(['Bulleted list', 'Numbered list', 'To-do list']);
  });

  it('returns nothing when nothing matches', () => {
    expect(labels('zzz')).toEqual([]);
  });

  it('is case-insensitive', () => {
    expect(labels('HEAD')).toEqual(['Heading 1', 'Heading 2', 'Heading 3']);
  });

  it('never returns duplicates when a query matches label and keyword', () => {
    const result = labels('code');
    expect(result).toEqual(['Code block']);
    expect(new Set(result).size).toBe(result.length);
  });
});
