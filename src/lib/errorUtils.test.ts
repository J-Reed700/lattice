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

  it('unwraps nested error objects with an inner message', () => {
    expect(getErrorMessage({ error: { message: 'Inner error message' } })).toBe('Inner error message');
  });

  it('uses fallback when provided and error cannot provide a meaningful message', () => {
    expect(getErrorMessage(null, 'Default fallback error')).toBe('Default fallback error');
    expect(getErrorMessage(undefined, 'Default fallback error')).toBe('Default fallback error');
  });

  it('falls back to String() for values JSON cannot write', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(getErrorMessage(circular)).toBe('[object Object]');
    expect(getErrorMessage(10n)).toBe('10');
  });
});
