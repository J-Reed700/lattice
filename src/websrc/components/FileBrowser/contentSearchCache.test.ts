import { expect, it } from 'vitest';

import { ContentSearchCache } from './contentSearchCache';

it('evicts least recently used text and never retains oversized files', () => {
  const cache = new ContentSearchCache(6);
  cache.set('a', '1', 'aaa');
  cache.set('b', '1', 'bbb');
  expect(cache.get('a', '1')).toBe('aaa');
  cache.set('c', '1', 'ccc');
  expect(cache.get('b', '1')).toBeUndefined();
  cache.set('huge', '1', '1234567');
  expect(cache.get('huge', '1')).toBeUndefined();
  expect(cache.get('a', '1')).toBe('aaa');
});

it('invalidates changed and removed documents', () => {
  const cache = new ContentSearchCache(6);
  cache.set('a', '1', 'aaa');
  expect(cache.get('a', '2')).toBeUndefined();
  cache.set('b', '1', 'bbbbbb');
  cache.retain(new Set());
  expect(cache.get('b', '1')).toBeUndefined();
  cache.set('c', '1', 'cccccc');
  expect(cache.get('c', '1')).toBe('cccccc');
});
