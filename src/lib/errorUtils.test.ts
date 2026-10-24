import { describe, expect, it } from 'vitest';

import { getErrorMessage } from './errorUtils';

describe('getErrorMessage', () => {
  it.each([
    [new Error('oven failed'), 'oven failed'],
    ['plain text', 'plain text'],
    [{ message: 'from an object' }, 'from an object'],
    [{ message: 42 }, '42'],
    [{ code: 7 }, '{"code":7}'],
    [null, 'null'],
  ])('reads %o as %j', (error, message) => {
    expect(getErrorMessage(error)).toBe(message);
  });

  // Bug: JSON.stringify(undefined) returns undefined, so the "always a string"
  // contract breaks for a bare `throw undefined` (errorUtils.ts:39).
  it.fails('reads an undefined error as text', () => {
    expect(typeof getErrorMessage(undefined)).toBe('string');
  });

  it('falls back to String() for values JSON cannot write', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(getErrorMessage(circular)).toBe('[object Object]');
    expect(getErrorMessage(10n)).toBe('10');
  });
});
