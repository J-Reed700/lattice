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

  it('reads an undefined error as text', () => {
    expect(getErrorMessage(undefined)).toBe('undefined');
  });

  it('contains failures while reading or formatting an error', () => {
    const error = {
      get message() { throw new Error('unreadable'); },
      toString() { throw new Error('unprintable'); },
    };
    expect(getErrorMessage(error)).toBe('Unknown error');
  });

  it('falls back to String() for values JSON cannot write', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(getErrorMessage(circular)).toBe('[object Object]');
    expect(getErrorMessage(10n)).toBe('10');
  });
});
